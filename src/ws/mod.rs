//! 内嵌 WebSocket 单端点（前端 <-> Rust）：JSON-RPC 3.0 单连接多路复用。
//! - 请求-响应：{jsonrpc:"3.0", id, method, params} -> {id, resId, result|error}
//! - 服务端流：stream:{id, seq, mode:"server", complete?}（run 流 / 订阅流）
//! - 系统方法：rpc.subscribe / rpc.unsubscribe / rpc.cancelStream / rpc.heartbeat
//! 端口启动时随机分配，前端经 ws_port command 获取。

mod llm_task;
mod ops;
mod ops_provider;
pub mod protocol;
mod rpc;
mod settings;
mod stream;

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::AppState;
use protocol::{Request, Response};

/// 全局流序号表（stream_id -> 已用 seq，跨连接共享保证单调）。
static STREAM_SEQ: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

pub fn next_seq(stream_id: &str) -> u64 {
    let mut guard = STREAM_SEQ.lock().expect("stream seq");
    let map = guard.get_or_insert_with(HashMap::new);
    let seq = map.entry(stream_id.to_string()).or_insert(0);
    let current = *seq;
    *seq += 1;
    current
}

/// 连接级订阅绑定：topic -> sub stream_id。
struct SubBinding {
    stream_id: String,
    topics: HashSet<String>,
}

// ---------------- 启动 ----------------

pub async fn serve(app: AppHandle) -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(handle_mux(stream, app.clone()));
        }
    });
    Ok(port)
}

/// 单连接多路复用（JSON-RPC 3.0）。
async fn handle_mux(stream: TcpStream, app: AppHandle) {
    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let (mut tx, mut rx) = ws.split();
    let mut subs: Vec<SubBinding> = Vec::new();
    let mut bus_rx = app.state::<Arc<AppState>>().bus.subscribe();

    loop {
        tokio::select! {
            msg = rx.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        let Some(resp) = handle_client_frame(&text, &mut subs, &app).await else { continue };
                        if tx.send(WsMessage::Text(resp.into())).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    _ => {}
                }
            }
            event = bus_rx.recv() => {
                let Ok(event) = event else { break };
                for chunk in stream::event_to_chunks(event, &subs) {
                    let Ok(text) = serde_json::to_string(&chunk) else { continue };
                    if tx.send(WsMessage::Text(text.into())).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}

/// 处理一条客户端帧：3.0 请求 -> 响应文本（heartbeat/无响应型返回 None 由调用方跳过）。
async fn handle_client_frame(text: &str, subs: &mut Vec<SubBinding>, app: &AppHandle) -> Option<String> {
    let Ok(req) = serde_json::from_str::<Request>(text) else {
        let resp = Response::err(Value::Null, protocol::PARSE_ERROR, "invalid json-rpc frame");
        return serde_json::to_string(&resp).ok();
    };
    match req.method.as_str() {
        protocol::M_HEARTBEAT => {
            let resp = Response::ok(req.id, json!({ "alive": true }));
            return serde_json::to_string(&resp).ok();
        }
        protocol::M_SUBSCRIBE => {
            let topics: HashSet<String> = req
                .params
                .get("topics")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let stream_id = protocol::stream_id("sub");
            subs.push(SubBinding { stream_id: stream_id.clone(), topics });
            let resp = Response::ok(req.id, json!({ "stream_id": stream_id }));
            return serde_json::to_string(&resp).ok();
        }
        protocol::M_UNSUBSCRIBE => {
            let stream_id = req.params.get("stream_id").and_then(Value::as_str).unwrap_or("");
            subs.retain(|b| b.stream_id != stream_id);
            let resp = Response::ok(req.id, json!(true));
            return serde_json::to_string(&resp).ok();
        }
        protocol::M_CANCEL_STREAM => {
            let stream_id = req.params.get("stream_id").and_then(Value::as_str).unwrap_or("");
            let cancelled = cancel_stream(stream_id, subs, app);
            let resp = Response::ok(req.id, json!(cancelled));
            return serde_json::to_string(&resp).ok();
        }
        _ => {}
    }

    let result = rpc::rpc_call(&req.method, req.params, &app).await;
    let resp = match result {
        Ok(value) => Response::ok(req.id, value),
        Err(e) => Response::err(req.id, protocol::INTERNAL_ERROR, e),
    };
    serde_json::to_string(&resp).ok()
}

/// cancelStream：run 流找 session cancel；sub 流退订。
fn cancel_stream(stream_id: &str, subs: &mut Vec<SubBinding>, app: &AppHandle) -> bool {
    if stream_id.starts_with("sub-") {
        subs.retain(|b| b.stream_id != stream_id);
        return true;
    }
    let state = app.state::<Arc<AppState>>();
    let session_id = kxen_app::core::shared::lock(&state.run_streams).get(stream_id).cloned();
    if let Some(session_id) = session_id {
        let token = kxen_app::core::shared::lock(&state.active_runs).get(&session_id).cloned();
        if let Some(token) = token {
            token.cancel();
            return true;
        }
    }
    false
}
