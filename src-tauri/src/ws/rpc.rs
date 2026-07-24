//! RPC 通道：请求-响应（id 关联，支持并发调用）。

use serde_json::{Value, json};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use super::llm_task::run_llm;
use super::settings::{set_role, statusline_report};
use crate::AppState;
use crate::doctor::doctor_report;

pub(super) async fn rpc_call(method: &str, params: Value, app: &AppHandle) -> Result<Value, String> {
    // 领域分组先走 ops.rs（voice/knowledge/provider/mrm/test_dispatch）
    if let Some(result) = super::ops::try_handle(method, &params, app).await {
        return result;
    }
    match method {
        "doctor" => {
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?.clone();
            let mut report = doctor_report(&store);
            report.system = Some(crate::doctor::system_health(&state).await);
            Ok(serde_json::to_value(report).map_err(|e| e.to_string())?)
        }
        "current_model" => {
            // 带 session_id 返回该会话生效模型（覆盖 > 全局默认）；不传同旧行为
            let state = app.state::<Arc<AppState>>();
            let sid = params.get("session_id").and_then(Value::as_str);
            let model = super::session_ops::effective_session_model(sid, &state);
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
        m if m.starts_with("goal.") => crate::goal_rpc::call(m, params, &app.state::<Arc<AppState>>().bus),
        "workspace.list" => Ok(json!(kxen_app::core::workspace::list(&kxen_app::core::paths::data_dir()))),
        "session.list" => {
            // 全量返回（侧栏树按 workspace 分组，过滤在前端）；附运行中标记
            let state = app.state::<Arc<AppState>>();
            let running: std::collections::HashSet<String> = kxen_app::core::shared::lock(&state.active_runs).keys().cloned().collect();
            let sessions = kxen_app::core::session::list(&kxen_app::core::paths::sessions_dir());
            Ok(json!(
                sessions
                    .into_iter()
                    .map(|s| {
                        let running_flag = running.contains(&s.id);
                        let mut v = serde_json::to_value(&s).unwrap_or_default();
                        v.as_object_mut().map(|o| o.insert("running".into(), json!(running_flag)));
                        v
                    })
                    .collect::<Vec<_>>()
            ))
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
            *state.active_workspace.write().expect("workspace") = dir.clone();
            kxen_app::core::workspace::touch(&kxen_app::core::paths::data_dir(), path).map_err(|e| e.to_string())?;
            // 信任门：未信任且含知识/项目配置 -> 后台审批；hooks 按信任重载（实现见 core/trust.rs）
            kxen_app::core::trust::gate_async(&dir, &state.approvals, &state.bus);
            kxen_app::core::trust::reload_hooks_for_workspace(&dir, &state.hooks);
            // LSP per-workspace 重建：杀旧 server，新根首个 diagnostics 请求再懒启动
            let old_lsp = std::mem::replace(&mut *state.lsp.write().expect("lsp"), kxen_app::lsp::LspManager::new(dir.clone()));
            old_lsp.shutdown().await;
            // MCP 随 workspace 换批：server 冷启动可至 60s，后台 spawn 不卡 RPC
            {
                let mcp = state.mcp.clone();
                let dir2 = dir.clone();
                tokio::spawn(async move {
                    kxen_app::mcp::reload_for_workspace(&dir2, &mcp).await;
                });
            }
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
            // session_start hook（fire-and-log；Ask 档走审批通道，临时值活到语句结束可安全借用）
            let _ = state
                .hooks
                .run_named_with_approval(
                    "session_start",
                    &session.id,
                    &json!({ "id": session.id, "directory": directory }),
                    kxen_app::tools::exec::ApprovalCtx::new(
                        Some(state.approvals.as_ref()),
                        Some(&state.bus),
                        None,
                        Some(session.id.as_str()),
                    )
                    .as_ref(),
                )
                .await
                .inspect_err(|e| tracing::warn!(error = %e, "session_start hook failed"));
            Ok(json!(session))
        }
        "session.messages" => {
            let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
            Ok(json!(kxen_app::core::session::load_messages(&kxen_app::core::paths::sessions_dir(), id)))
        }
        "session.delete" => super::session_ops::session_delete(&params, &app.state::<Arc<AppState>>()).await,
        "session.update_meta" => super::session_ops::session_update_meta(&params),
        "session.set_model" => super::session_ops::session_set_model(&params),
        "session.foreground" => {
            let id = params.get("id").and_then(Value::as_str).unwrap_or("");
            *app.state::<Arc<AppState>>().foreground_session.write().expect("foreground") = id.to_string();
            Ok(Value::Null)
        }
        "session.fork" => {
            let session_id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
            let message_id = params.get("message_id").and_then(Value::as_str).ok_or("missing message_id")?;
            let session =
                kxen_app::core::session::fork(&kxen_app::core::paths::sessions_dir(), session_id, message_id).map_err(|e| e.to_string())?;
            Ok(json!(session))
        }
        "session.rewind" => super::session_ops::session_rewind(&params, &app.state::<Arc<AppState>>()),
        "session.pending_list" => {
            let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
            let state = app.state::<Arc<AppState>>();
            Ok(json!(state.pending_messages.texts(id)))
        }
        "session.pending_clear" => {
            let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
            let state = app.state::<Arc<AppState>>();
            let n = state.pending_messages.clear(id);
            Ok(json!({ "cleared": n }))
        }
        "session.export" => {
            let session_id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
            let out = params.get("path").and_then(Value::as_str).map(std::path::PathBuf::from);
            let path = kxen_app::core::session_export::export_to_file(&kxen_app::core::paths::sessions_dir(), session_id, out.as_deref())
                .map_err(|e| e.to_string())?;
            Ok(json!({ "path": path.to_string_lossy() }))
        }
        "worktree.list" => {
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            let infos = kxen_app::tools::worktree::list(&dir).await?;
            Ok(json!(
                infos.iter().map(|i| json!({ "name": i.name, "path": i.path.to_string_lossy(), "branch": i.branch })).collect::<Vec<_>>()
            ))
        }
        "worktree.create" => {
            let name = params.get("name").and_then(Value::as_str).ok_or("missing name")?;
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            let info = kxen_app::tools::worktree::create(&dir, name).await?;
            Ok(json!({ "name": info.name, "path": info.path.to_string_lossy(), "branch": info.branch }))
        }
        "worktree.remove" => {
            let name = params.get("name").and_then(Value::as_str).ok_or("missing name")?;
            let delete_branch = params.get("delete_branch").and_then(Value::as_bool).unwrap_or(false);
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            let appr = kxen_app::tools::exec::ApprovalCtx::new(Some(state.approvals.as_ref()), Some(&state.bus), None, None);
            kxen_app::tools::worktree::remove_with_approval(&dir, name, delete_branch, appr.as_ref()).await?;
            Ok(json!(true))
        }
        "worktree.status" => {
            // 单棵 worktree 的脏文件清单（看板行内计数数据源）
            let path = params.get("path").and_then(Value::as_str).ok_or("missing path")?;
            let entries = kxen_app::tools::worktree::status(std::path::Path::new(path)).await?;
            Ok(json!(entries))
        }
        "diff.status" => {
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            Ok(json!(kxen_app::tools::worktree::status(&dir).await?))
        }
        "diff.agent_status" => {
            // 本会话 agent 改动（快照口径），与 git status 无关
            let id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
            let state = app.state::<Arc<AppState>>();
            let entries = kxen_app::core::shared::lock(&state.session_snapshots).get(id).map(|s| s.status()).unwrap_or_default();
            Ok(serde_json::to_value(entries).map_err(|e| e.to_string())?)
        }
        "diff.agent_file" => {
            let id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
            let path = params.get("path").and_then(Value::as_str).ok_or("missing path")?;
            let state = app.state::<Arc<AppState>>();
            let store = kxen_app::core::shared::lock(&state.session_snapshots).get(id).cloned();
            let p = std::path::Path::new(path);
            let text = store.and_then(|s| s.diff(p).or_else(|| s.diff_created(p)));
            Ok(json!({ "text": text.unwrap_or_default() }))
        }
        "diff.file" => {
            let path = params.get("path").and_then(Value::as_str).ok_or("missing path")?;
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            let text = kxen_app::tools::worktree::diff_file(&dir, path).await?;
            Ok(json!(text))
        }
        "send_message" => {
            let p: super::session_ops::SendMessageParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
            let state = app.state::<Arc<AppState>>();
            // run 进行中：默认入队（queue）；config.send_when_running=interrupt 时打断当前立即发送
            if kxen_app::core::shared::lock(&state.active_runs).contains_key(&p.session_id) {
                let cfg = kxen_app::core::config::Config::load(&kxen_app::core::paths::config_dir().join("config.toml"), None)
                    .unwrap_or_default();
                let policy = if cfg.send_when_running.is_empty() { "queue" } else { cfg.send_when_running.as_str() };
                if policy != "interrupt" {
                    let n = state.pending_messages.enqueue(&p.session_id, p.text, p.context, p.images);
                    state.bus.publish(kxen_app::core::event::Event::Notification(format!("运行中，消息已排队（第 {n} 条）")));
                    return Ok(json!({ "queued": true }));
                }
                if let Some(token) = kxen_app::core::shared::lock(&state.active_runs).get(&p.session_id).cloned() {
                    token.cancel();
                }
            }
            // 分配 run 流 id（JSON-RPC 3.0：增量走 stream chunk 下发）
            let stream_id = super::protocol::stream_id("run");
            kxen_app::core::shared::lock(&state.run_streams).insert(stream_id.clone(), p.session_id.clone());
            tokio::spawn(run_llm(stream_id.clone(), p.session_id, p.text, p.context, p.images, app.clone()));
            Ok(json!({ "stream_id": stream_id }))
        }
        "session.abort" => {
            let id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
            let state = app.state::<Arc<AppState>>();
            // abort = 停当前 + 清队列（否则 abort 完队列立刻续跑，等于没停）
            state.pending_messages.clear(id);
            let token = kxen_app::core::shared::lock(&state.active_runs).get(id).cloned();
            Ok(json!(token.map(|t| t.cancel()).is_some()))
        }
        "approval.respond" => {
            let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
            let allow = params.get("allow").and_then(Value::as_bool).ok_or("missing allow")?;
            Ok(json!({ "resolved": app.state::<Arc<AppState>>().approvals.respond(id, allow) }))
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
        "agents.stop" => super::ops_agents::agents_stop(&params, &app.state::<Arc<AppState>>()).await,
        "statusline" => {
            let session_id = params.get("session_id").and_then(Value::as_str).unwrap_or("");
            let state = app.state::<Arc<AppState>>();
            Ok(statusline_report(session_id, &state))
        }
        "config.get" => {
            let config = kxen_app::core::config::Config::load(&kxen_app::core::paths::config_dir().join("config.toml"), None)
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
        "fs.resolve_name" => {
            let name = params.get("name").and_then(Value::as_str).ok_or("missing name")?;
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            Ok(json!(kxen_app::tools::search::find_by_name(name, &dir)))
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
