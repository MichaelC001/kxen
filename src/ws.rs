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
        "task.list" => {
            let state = app.state::<Arc<AppState>>();
            Ok(json!(state.registry.list()))
        }
        "task.kill" => {
            let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
            let state = app.state::<Arc<AppState>>();
            Ok(json!(state.registry.kill(id).await))
        }
        "set_model" => {
            let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
            let model = params.get("model").and_then(Value::as_str).ok_or("missing model")?;
            let state = app.state::<Arc<AppState>>();
            *state.model.lock().map_err(|e| e.to_string())? = kxen_app::llm::ModelRef::new(provider, model);
            Ok(json!({ "provider": provider, "model": model }))
        }
        m if m.starts_with("goal.") => crate::goal_rpc::call(m, params),
        "session.list" => Ok(json!(kxen_app::core::session::list(&kxen_app::core::paths::sessions_dir()))),
        "session.create" => {
            let state = app.state::<Arc<AppState>>();
            let directory = params
                .get("directory")
                .and_then(Value::as_str)
                .map(String::from)
                .unwrap_or_else(|| state.workdir.to_string_lossy().into_owned());
            let session = kxen_app::core::session::create(&kxen_app::core::paths::sessions_dir(), &directory).map_err(|e| e.to_string())?;
            Ok(json!(session))
        }
        "session.messages" => {
            let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
            Ok(json!(kxen_app::core::session::load_messages(&kxen_app::core::paths::sessions_dir(), id)))
        }
        "session.delete" => {
            let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
            kxen_app::core::session::remove(&kxen_app::core::paths::sessions_dir(), id);
            Ok(Value::Null)
        }
        "diff.status" => {
            let state = app.state::<Arc<AppState>>();
            Ok(json!(kxen_app::tools::worktree::status(&state.workdir).await?))
        }
        "diff.file" => {
            let path = params.get("path").and_then(Value::as_str).ok_or("missing path")?;
            let state = app.state::<Arc<AppState>>();
            let text = kxen_app::tools::worktree::diff_file(&state.workdir, path).await?;
            Ok(json!(text))
        }
        "send_message" => {
            let p: SendMessageParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            tokio::spawn(run_llm(p.session_id, p.text, p.context, app.clone()));
            Ok(Value::Null)
        }
        "session.abort" => {
            let id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
            let state = app.state::<Arc<AppState>>();
            let token = kxen_app::core::shared::lock(&state.active_runs).get(id).cloned();
            Ok(json!(token.map(|t| t.cancel()).is_some()))
        }
        "team.list" => {
            let id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
            let state = app.state::<Arc<AppState>>();
            Ok(state.team.list_json(id))
        }
        "team.message" => {
            let session_id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
            let name = params.get("name").and_then(Value::as_str).ok_or("missing name")?;
            let text = params.get("text").and_then(Value::as_str).ok_or("missing text")?;
            let state = app.state::<Arc<AppState>>();
            state.team.lead_action(session_id, &json!({ "action": "message", "name": name, "text": text })).await.map(Value::String)
        }
        "statusline" => {
            let session_id = params.get("session_id").and_then(Value::as_str).unwrap_or("");
            let state = app.state::<Arc<AppState>>();
            Ok(statusline_report(session_id, &state))
        }
        "config.get" => {
            let config = kxen_app::core::config::Config::load(
                &kxen_app::core::paths::config_dir().join("config.toml"),
                None,
            )
            .map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(config).map_err(|e| e.to_string())?)
        }
        "config.set_role" => {
            let role = params.get("role").and_then(Value::as_str).ok_or("missing role")?;
            let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
            let model = params.get("model").and_then(Value::as_str).ok_or("missing model")?;
            let fallback = params.get("fallback").and_then(Value::as_str);
            let state = app.state::<Arc<AppState>>();
            set_role(role, provider, model, fallback, &state)
        }
        "fs.complete" => {
            let query = params.get("query").and_then(Value::as_str).unwrap_or("");
            let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
            let state = app.state::<Arc<AppState>>();
            Ok(json!(kxen_app::tools::search::complete(query, &state.workdir, limit)))
        }
        "command.list" => {
            let state = app.state::<Arc<AppState>>();
            let mut commands = kxen_app::agent::commands::list(&state.workdir);
            // skills 并入弹窗（kind=skill，标注是否 user-invocable）
            commands.extend(kxen_app::agent::skills::scan(&state.workdir).into_iter().filter(|s| s.user_invocable).map(|s| {
                kxen_app::agent::commands::CommandInfo {
                    name: s.name,
                    description: s.description,
                    kind: "skill",
                    argument_hint: if s.arguments.is_empty() { None } else { Some(s.arguments.join(" ")) },
                }
            }));
            Ok(json!(commands))
        }
        other => Err(format!("unknown method: {other}")),
    }
}

#[derive(Debug, Deserialize)]
struct SendMessageParams {
    session_id: String,
    text: String,
    #[serde(default)]
    context: Vec<kxen_app::agent::context::ContextItem>,
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

async fn run_llm(session_id: String, text: String, context: Vec<kxen_app::agent::context::ContextItem>, app: AppHandle) {
    use kxen_app::core::session as ses;

    let state = app.state::<Arc<AppState>>();
    let sessions_dir = kxen_app::core::paths::sessions_dir();

    // @ 引用注入：chip -> 上下文块（文件/目录/Web/Docs），追加在用户消息尾部
    let context_block = if context.is_empty() {
        String::new()
    } else {
        kxen_app::agent::context::build_context(&context, &state.workdir).await
    };
    let text = if context_block.is_empty() { text } else { format!("{text}\n{context_block}") };

    // 用户消息落盘（LLM 历史以后端会话存储为准，前端不再传 history）
    let user_msg = ses::new_message(&session_id, ses::Role::User, vec![ses::Part::Text { text: text.clone() }]);
    if let Err(e) = ses::append_message(&sessions_dir, &user_msg) {
        tracing::error!(error = %e, "session append failed");
        return;
    }

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

    // 历史：存储里的 user/assistant 文本
    let mut messages: Vec<Message> = ses::load_messages(&sessions_dir, &session_id)
        .into_iter()
        .filter_map(|m| {
            let text: String = m
                .parts
                .iter()
                .filter_map(|p| match p {
                    ses::Part::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                return None;
            }
            Some(match m.role {
                ses::Role::User => Message::user(text),
                ses::Role::Assistant => Message::assistant(text),
                ses::Role::System => Message::system(text),
            })
        })
        .collect();
    // lead inbox：teammate 来信作为用户角色消息注入（排在本轮新消息之前）
    let inbox = state.team.drain_lead_inbox(&session_id);
    for (from, note) in inbox {
        messages.push(Message::user(format!("[teammate {from}] {note}")));
    }
    if messages.is_empty() {
        messages.push(Message::user(text));
    }

    // 转录件：run 结束后整条 assistant 消息（文本 + 工具调用）落盘
    let transcript = Arc::new(std::sync::Mutex::new(Vec::<ses::Part>::new()));
    let transcript_writer = transcript.clone();
    let sid = session_id.clone();

    // 取消令牌：注册到 active_runs，run 结束移除（session.abort 可达）
    let cancel = kxen_app::agent::cancel::CancelToken::new();
    kxen_app::core::shared::lock(&state.active_runs).insert(session_id.clone(), cancel.clone());

    let mut ctx = kxen_app::agent::agent_loop::AgentContext {
        registry,
        tracker: kxen_app::tools::fs_tool::FileTracker::default(),
        workdir,
        model,
        store,
        max_turns: 32,
        mrm: Some(state.mrm.read().expect("mrm lock").clone()),
        allowed_tools: None,
        extras: Some(state.extras.clone()),
        hooks: Some(state.hooks.clone()),
        loop_detector: kxen_app::agent::loop_detect::LoopDetector::new(),
        cancel: Some(cancel.clone()),
        team: Some(state.team.clone()),
        team_identity: None,
        session_id: Some(session_id.clone()),
        on_event: Arc::new(move |event| {
            use kxen_app::agent::agent_loop::AgentEvent as AE;
            match &event {
                AE::ToolCall { name, summary } => {
                    transcript_writer
                        .lock()
                        .expect("transcript")
                        .push(ses::Part::ToolCall { name: name.clone(), input: json!(summary), output: String::new() });
                }
                AE::ToolResult { name, summary } => {
                    let mut guard = transcript_writer.lock().expect("transcript");
                    if let Some(ses::Part::ToolCall { output, .. }) =
                        guard.iter_mut().rev().find(|p| matches!(p, ses::Part::ToolCall { name: n, output, .. } if n == name && output.is_empty()))
                    {
                        *output = summary.clone();
                    }
                }
                _ => {}
            }
            let mut payload = match serde_json::to_value(&event) {
                Ok(v) => v,
                Err(_) => return,
            };
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("session_id".into(), json!(sid));
            }
            bus.publish(kxen_app::core::event::Event::LlmDelta(payload));
        }),
    };
    let outcome = kxen_app::agent::agent_loop::run_turn(&mut ctx, messages).await;
    kxen_app::core::shared::lock(&state.active_runs).remove(&session_id);
    // 用量累计（状态栏 tokens 段）
    if let Some(stats) = outcome.stats {
        let mut map = kxen_app::core::shared::lock(&state.session_tokens);
        let entry = map.entry(session_id.clone()).or_insert((0, 0));
        entry.0 += stats.input_tokens;
        entry.1 += stats.output_tokens;
    }

    let mut parts = transcript.lock().expect("transcript").clone();
    if !outcome.final_text.is_empty() {
        parts.push(ses::Part::Text { text: outcome.final_text });
    }
    if outcome.aborted {
        parts.push(ses::Part::Text { text: "(已中断)".into() });
    }
    if !parts.is_empty() {
        let assistant_msg = ses::new_message(&session_id, ses::Role::Assistant, parts);
        if let Err(e) = ses::append_message(&sessions_dir, &assistant_msg) {
            tracing::error!(error = %e, "session append failed");
        }
    }
}

// ---------------- 状态栏与设置 ----------------

fn statusline_report(session_id: &str, state: &Arc<AppState>) -> Value {
    let items = kxen_app::core::shared::lock(&state.statusline_items).clone();

    // git 分支（5s 缓存）
    let git_branch = {
        let mut cache = kxen_app::core::shared::lock(&state.git_cache);
        if cache.0.elapsed() > std::time::Duration::from_secs(5) {
            let branch = std::process::Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(&*state.workdir)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            *cache = (std::time::Instant::now(), branch);
        }
        cache.1.clone()
    };

    let focus = kxen_app::core::goal::Goal::focus(&kxen_app::core::paths::goals_dir());
    let tasks_running = state.registry.list().iter().filter(|t| matches!(t.status, kxen_app::tools::task::TaskStatus::Running)).count();
    let tokens = kxen_app::core::shared::lock(&state.session_tokens).get(session_id).copied().unwrap_or((0, 0));
    let model = state.model.lock().map(|m| m.clone()).unwrap_or_default();

    json!({
        "items": items,
        "workdir": state.workdir.to_string_lossy(),
        "git_branch": git_branch,
        "goal": focus.map(|g| json!({ "id": g.id, "status": format!("{:?}", g.status).to_lowercase() })),
        "tasks_running": tasks_running,
        "tokens": { "input": tokens.0, "output": tokens.1 },
        "model": format!("{}/{}", model.provider, model.model),
    })
}

/// 非破坏写回：toml::Value 上改 roles[role]，保留文件其余内容；随后重建 MRM 热换 Arc。
fn set_role(role: &str, provider: &str, model: &str, fallback: Option<&str>, state: &Arc<AppState>) -> Result<Value, String> {
    let path = kxen_app::core::paths::config_dir().join("config.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc: toml::Value = if text.trim().is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        text.parse().map_err(|e| format!("config.toml parse: {e}"))?
    };
    let table = doc.as_table_mut().ok_or("config.toml root is not a table")?;
    let roles = table.entry(String::from("roles")).or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let roles_table = roles.as_table_mut().ok_or("roles is not a table")?;
    let mut binding = toml::map::Map::new();
    binding.insert("provider".into(), toml::Value::String(provider.into()));
    binding.insert("model".into(), toml::Value::String(model.into()));
    if let Some(f) = fallback {
        binding.insert("fallback".into(), toml::Value::String(f.into()));
    }
    roles_table.insert(role.into(), toml::Value::Table(binding));

    std::fs::create_dir_all(kxen_app::core::paths::config_dir()).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, doc.to_string()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;

    // 重建 MRM 热换
    let config = kxen_app::core::config::Config::load(&path, None).map_err(|e| e.to_string())?;
    *state.mrm.write().expect("mrm lock") = std::sync::Arc::new(kxen_app::llm::mrm::ModelResourceManager::new(config));
    Ok(json!({ "role": role, "provider": provider, "model": model }))
}
