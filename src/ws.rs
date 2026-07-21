//! 内嵌 WebSocket 双通道（前端 <-> Rust）。
//! - /rpc：请求-响应（id 关联，支持并发调用）
//! - /stream：订阅-推送（topic 过滤，server 主动推）
//! 端口启动时随机分配，经 window eval 注入前端。

use futures::{SinkExt, StreamExt};
use kxen_app::llm::Message;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::doctor::doctor_report;
use crate::AppState;

// ---------------- 协议 ----------------

#[derive(Debug, Deserialize)]
struct RpcRequest {
    id: String,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum StreamCtl {
    Subscribe { topics: Vec<String> },
    Unsubscribe { topics: Vec<String> },
}

#[derive(Debug, Clone, Serialize)]
struct StreamPush {
    topic: String,
    payload: Value,
}

// ---------------- 启动 ----------------

pub async fn serve(app: AppHandle) -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(dispatch(stream, app.clone()));
        }
    });
    Ok(port)
}

async fn dispatch(stream: TcpStream, app: AppHandle) {
    use std::sync::Mutex as StdMutex;
    use tokio_tungstenite::tungstenite::handshake::server::{Callback, ErrorResponse, Request, Response};

    struct PathCb(Arc<StdMutex<String>>);
    impl Callback for PathCb {
        fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
            if let Ok(mut path) = self.0.lock() {
                *path = request.uri().path().to_string();
            }
            Ok(response)
        }
    }

    let path = Arc::new(StdMutex::new(String::new()));
    let Ok(ws) = tokio_tungstenite::accept_hdr_async(stream, PathCb(path.clone())).await else {
        return;
    };
    let route = path.lock().map(|p| p.clone()).unwrap_or_default();
    match route.as_str() {
        "/rpc" => handle_rpc(ws, app).await,
        _ => handle_stream(ws, app).await,
    }
}



// ---------------- RPC 通道 ----------------

async fn handle_rpc(
    ws: tokio_tungstenite::WebSocketStream<TcpStream>,
    app: AppHandle,
) {
    let (mut tx, mut rx) = ws.split();
    while let Some(Ok(msg)) = rx.next().await {
        let WsMessage::Text(text) = msg else { continue };
        let Ok(req) = serde_json::from_str::<RpcRequest>(&text) else {
            let resp = RpcResponse { id: String::new(), ok: false, result: None, error: Some("bad rpc frame".into()) };
            let _ = tx.send(WsMessage::Text(serde_json::to_string(&resp).unwrap().into())).await;
            continue;
        };
        let result = rpc_call(&req.method, req.params, &app).await;
        let resp = match result {
            Ok(value) => RpcResponse { id: req.id, ok: true, result: Some(value), error: None },
            Err(e) => RpcResponse { id: req.id, ok: false, result: None, error: Some(e) },
        };
        let Ok(text) = serde_json::to_string(&resp) else { break };
        if tx.send(WsMessage::Text(text.into())).await.is_err() {
            break;
        }
    }
}

async fn rpc_call(method: &str, params: Value, app: &AppHandle) -> Result<Value, String> {
    match method {
        "doctor" => Ok(serde_json::to_value(doctor_report()).map_err(|e| e.to_string())?),
        "current_model" => {
            let state = app.state::<Arc<AppState>>();
            let model = state.model.lock().map_err(|e| e.to_string())?.clone();
            Ok(json!({ "provider": model.provider, "model": model.model }))
        }
        "set_model" => {
            let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
            let model = params.get("model").and_then(Value::as_str).ok_or("missing model")?;
            let state = app.state::<Arc<AppState>>();
            *state.model.lock().map_err(|e| e.to_string())? = kxen_app::llm::ModelRef::new(provider, model);
            Ok(json!({ "provider": provider, "model": model }))
        }
        m if m.starts_with("goal.") => crate::goal_rpc::call(m, params),
        "send_message" => {
            let p: SendMessageParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            tokio::spawn(run_llm(p.text, p.history, app.clone()));
            Ok(Value::Null)
        }
        other => Err(format!("unknown method: {other}")),
    }
}

#[derive(Debug, Deserialize)]
struct SendMessageParams {
    text: String,
    #[serde(default)]
    history: Vec<HistoryMsg>,
}

#[derive(Debug, Deserialize)]
struct HistoryMsg {
    role: HistoryRole,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum HistoryRole {
    System,
    Assistant,
    User,
}

// ---------------- Stream 通道 ----------------

async fn handle_stream(
    ws: tokio_tungstenite::WebSocketStream<TcpStream>,
    app: AppHandle,
) {
    let (mut tx, mut rx) = ws.split();
    let mut topics: HashSet<String> = ["llm.delta", "task.update", "goal.update", "notification"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut bus_rx = app.state::<Arc<AppState>>().bus.subscribe();

    loop {
        tokio::select! {
            // client 控制帧
            msg = rx.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Ok(ctl) = serde_json::from_str::<StreamCtl>(&text) {
                            match ctl {
                                StreamCtl::Subscribe { topics: t } => topics.extend(t),
                                StreamCtl::Unsubscribe { topics: t } => {
                                    for topic in t {
                                        topics.remove(&topic);
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    _ => {}
                }
            }
            // 内部事件桥 -> topic 推送
            event = bus_rx.recv() => {
                let Ok(event) = event else { break };
                let (topic, payload) = map_event(event);
                if !topics.contains(topic) {
                    continue;
                }
                let push = StreamPush { topic: topic.to_string(), payload };
                let Ok(text) = serde_json::to_string(&push) else { continue };
                if tx.send(WsMessage::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

fn map_event(event: kxen_app::core::event::Event) -> (&'static str, Value) {
    use kxen_app::core::event::Event;
    match event {
        Event::LlmDelta(payload) => ("llm.delta", payload),
        Event::ToolCall { name, summary } => ("llm.delta", json!({ "tool": name, "summary": summary })),
        Event::TaskUpdate { id, status } => ("task.update", json!({ "id": id, "status": status })),
        Event::GoalUpdate { id, status } => ("goal.update", json!({ "id": id, "status": status })),
        Event::Notification(text) => ("notification", json!({ "text": text })),
    }
}

// ---------------- LLM 任务 ----------------

async fn run_llm(text: String, history: Vec<HistoryMsg>, app: AppHandle) {
    let state = app.state::<Arc<AppState>>();
    let (model, store, registry, workdir, bus) = {
        let store = state.auth_store.lock().map(|s| s.clone()).unwrap_or_default();
        (
            state.model.lock().map(|m| m.clone()).unwrap_or_default(),
            store,
            state.registry.clone(),
            state.workdir.clone(),
            state.bus.clone(),
        )
    };

    let mut messages: Vec<Message> = history
        .into_iter()
        .map(|m| match m.role {
            HistoryRole::System => Message::system(m.content),
            HistoryRole::Assistant => Message::assistant(m.content),
            HistoryRole::User => Message::user(m.content),
        })
        .collect();
    messages.push(Message::user(text));

    let mut ctx = kxen_app::agent::agent_loop::AgentContext {
        registry,
        tracker: kxen_app::tools::fs_tool::FileTracker::default(),
        workdir,
        model,
        store,
        max_turns: 32,
        mrm: Some(state.mrm.clone()),
        allowed_tools: None,
        extras: Some(state.extras.clone()),
        hooks: Some(state.hooks.clone()),
        loop_detector: kxen_app::agent::loop_detect::LoopDetector::new(),
        on_event: Arc::new(move |event| {
            let payload = match serde_json::to_value(&event) {
                Ok(v) => v,
                Err(_) => return,
            };
            bus.publish(kxen_app::core::event::Event::LlmDelta(payload));
        }),
    };
    kxen_app::agent::agent_loop::run_turn(&mut ctx, messages).await;
}
