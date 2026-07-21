//! RPC 通道：请求-响应（id 关联，支持并发调用）。

use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use super::llm_task::run_llm;
use super::settings::{set_role, statusline_report};
use super::{RpcRequest, RpcResponse};
use crate::doctor::doctor_report;
use crate::AppState;

pub(super) async fn handle_rpc(
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
            tokio::spawn(run_llm(p.session_id, p.text, p.context, p.images, app.clone()));
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
        "agents.list" => {
            let session_id = params.get("session_id").and_then(Value::as_str).unwrap_or("");
            let state = app.state::<Arc<AppState>>();
            Ok(json!(state.agents.list(session_id)))
        }
        "agents.transcript" => {
            let session_id = params.get("session_id").and_then(Value::as_str).unwrap_or("");
            let name = params.get("name").and_then(Value::as_str).ok_or("missing name")?;
            let state = app.state::<Arc<AppState>>();
            Ok(json!(state.agents.transcript(session_id, name)))
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
    #[serde(default)]
    images: Vec<kxen_app::llm::types::ImagePart>,
}
