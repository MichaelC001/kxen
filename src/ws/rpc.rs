//! RPC 通道：请求-响应（id 关联，支持并发调用）。

use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use super::llm_task::run_llm;
use super::settings::{set_role, statusline_report};
use crate::doctor::doctor_report;
use crate::AppState;


pub(super) async fn rpc_call(method: &str, params: Value, app: &AppHandle) -> Result<Value, String> {
    // 领域分组先走 ops.rs（voice/knowledge/provider/mrm/test_dispatch）
    if let Some(result) = super::ops::try_handle(method, &params, app).await {
        return result;
    }
    match method {
        "doctor" => {
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(doctor_report(&store)).map_err(|e| e.to_string())?)
        }
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
        "workspace.list" => Ok(json!(kxen_app::core::workspace::list(&kxen_app::core::paths::data_dir()))),
        "session.list" => {
            // 全量返回（侧栏树按 workspace 分组，过滤在前端）
            Ok(json!(kxen_app::core::session::list(&kxen_app::core::paths::sessions_dir())))
        }
        "workspace.current" => {
            let state = app.state::<Arc<AppState>>();
            let active = state.active_workspace.read().expect("workspace").to_string_lossy().into_owned();
            Ok(json!(active))
        }
        "workspace.add" => {
            let path = params.get("path").and_then(Value::as_str).ok_or("missing path")?;
            let dir = std::path::PathBuf::from(path);
            if !dir.is_dir() {
                return Err(format!("directory not found: {path}"));
            }
            kxen_app::core::workspace::touch(&kxen_app::core::paths::data_dir(), path).map_err(|e| e.to_string())?;
            Ok(json!(path))
        }
        "workspace.switch" => {
            let path = params.get("path").and_then(Value::as_str).ok_or("missing path")?;
            let dir = std::path::PathBuf::from(path);
            if !dir.is_dir() {
                return Err(format!("directory not found: {path}"));
            }
            let state = app.state::<Arc<AppState>>();
            *state.active_workspace.write().expect("workspace") = dir;
            kxen_app::core::workspace::touch(&kxen_app::core::paths::data_dir(), path).map_err(|e| e.to_string())?;
            Ok(json!(path))
        }
        "session.create" => {
            let state = app.state::<Arc<AppState>>();
            let directory = params
                .get("directory")
                .and_then(Value::as_str)
                .map(String::from)
                .unwrap_or_else(|| state.active_workspace.read().expect("workspace").to_string_lossy().into_owned());
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
            let dir = state.active_workspace.read().expect("workspace").clone();
            Ok(json!(kxen_app::tools::worktree::status(&dir).await?))
        }
        "diff.file" => {
            let path = params.get("path").and_then(Value::as_str).ok_or("missing path")?;
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            let text = kxen_app::tools::worktree::diff_file(&dir, path).await?;
            Ok(json!(text))
        }
        "send_message" => {
            let p: SendMessageParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            // 分配 run 流 id（JSON-RPC 3.0：增量走 stream chunk 下发）
            let stream_id = super::protocol::stream_id("run");
            kxen_app::core::shared::lock(&app.state::<Arc<AppState>>().run_streams).insert(stream_id.clone(), p.session_id.clone());
            tokio::spawn(run_llm(stream_id.clone(), p.session_id, p.text, p.context, p.images, app.clone()));
            Ok(json!({ "stream_id": stream_id }))
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
            let account = params.get("account").and_then(Value::as_str);
            let state = app.state::<Arc<AppState>>();
            set_role(role, provider, model, fallback, account, &state)
        }
        "fs.complete" => {
            let query = params.get("query").and_then(Value::as_str).unwrap_or("");
            let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            Ok(json!(kxen_app::tools::search::complete(query, &dir, limit)))
        }
        "command.list" => {
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            let mut commands = kxen_app::agent::commands::list(&dir);
            // skills 并入弹窗（kind=skill，标注是否 user-invocable）
            commands.extend(kxen_app::agent::skills::scan(&dir).into_iter().filter(|s| s.user_invocable).map(|s| {
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
