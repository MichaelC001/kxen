//! session 域辅助：rewind / send_message 参数 / 会话级模型与 meta 更新（rpc.rs 拆出，350 门禁）。

use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
pub(super) struct SendMessageParams {
    pub session_id: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub context: Vec<kxen_app::agent::context::ContextItem>,
    #[serde(default)]
    pub images: Vec<kxen_app::llm::types::ImagePart>,
}

/// rewind 门禁（纯函数，测试直接覆盖矩阵）：
/// - 同 workspace 有活跃 run：rewind 改写文件会与运行中的 agent 打架
/// - message id 不在本 session：拒绝（不得跨会话定位）
/// - 工作区有未提交改动且无 confirm：rewind 会丢弃，须显式确认
pub(super) fn rewind_gate(active_in_workspace: bool, dirty: bool, confirm: bool, message_found: bool) -> Result<(), String> {
    if active_in_workspace {
        return Err("同 workspace 有会话正在运行，先 abort 再 rewind".into());
    }
    if !message_found {
        return Err("message not found in this session".into());
    }
    if dirty && !confirm {
        return Err("工作区有未提交改动，rewind 将丢弃（传 confirm=true 确认）".into());
    }
    Ok(())
}

/// 代码回滚到该消息的 shadow 检查点 + 会话截断到该消息（含）。
pub(super) fn session_rewind(params: &Value, state: &crate::AppState) -> Result<Value, String> {
    let session_id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
    let message_id = params.get("message_id").and_then(Value::as_str).ok_or("missing message_id")?;
    let confirm = params.get("confirm").and_then(Value::as_bool).unwrap_or(false);
    let dir = kxen_app::core::paths::sessions_dir();
    let meta = kxen_app::core::session::load_meta(&dir, session_id).map_err(|e| e.to_string())?;
    let messages = kxen_app::core::session::load_messages(&dir, session_id);
    let message_found = messages.iter().any(|m| m.id == message_id);
    // 同 workspace（按 session 归属目录判定）任何 session 有 active run 即拒绝
    let active_in_workspace = kxen_app::core::shared::lock(&state.active_runs)
        .keys()
        .any(|sid| kxen_app::core::session::load_meta(&dir, sid).map(|m| m.directory == meta.directory).unwrap_or(false));
    let dirty = kxen_app::tools::checkpoint::is_dirty(std::path::Path::new(&meta.directory));
    rewind_gate(active_in_workspace, dirty, confirm, message_found)?;
    let idx = messages.iter().position(|m| m.id == message_id).expect("rewind_gate 已确认消息存在");
    let hash = kxen_app::tools::checkpoint::reset_to(std::path::Path::new(&meta.directory), message_id)?;
    kxen_app::core::session::rewrite_messages(&dir, session_id, &messages[..=idx]).map_err(|e| e.to_string())?;
    Ok(json!({ "commit": hash, "truncated_to": idx + 1 }))
}

/// ws 内共用的生效模型解析：session 覆盖 > 全局默认（AppState.model）。
pub(super) fn effective_session_model(session_id: Option<&str>, state: &crate::AppState) -> kxen_app::llm::ModelRef {
    let default = state.model.lock().map(|m| m.clone()).unwrap_or_default();
    let session_override =
        session_id.and_then(|id| kxen_app::core::session::load_meta(&kxen_app::core::paths::sessions_dir(), id).ok()).and_then(|m| m.model);
    kxen_app::core::session::effective_model(session_override.as_ref(), &default).clone()
}

/// session.set_model RPC：写会话级模型覆盖（落盘 meta JSON；全局默认仍走 set_model / config.set_role）。
/// provider/model 同缺 = 清除覆盖（跟随全局默认）；只给一个属调用方错误。
pub(super) fn session_set_model(params: &Value) -> Result<Value, String> {
    let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
    let over = parse_model_override(params)?;
    let session = kxen_app::core::session::set_model(&kxen_app::core::paths::sessions_dir(), id, over).map_err(|e| e.to_string())?;
    Ok(json!(session))
}

fn parse_model_override(params: &Value) -> Result<Option<kxen_app::llm::ModelRef>, String> {
    let provider = params.get("provider").and_then(Value::as_str);
    let model = params.get("model").and_then(Value::as_str);
    match (provider, model) {
        (Some(p), Some(m)) => Ok(Some(kxen_app::llm::ModelRef::new(p, m))),
        (None, None) => Ok(None),
        _ => Err("provider 与 model 必须同给或同缺".into()),
    }
}

/// session.update_meta RPC（rpc.rs 迁来，350 门禁）：重命名 / 置顶 / 手动排序。
pub(super) fn session_update_meta(params: &Value) -> Result<Value, String> {
    let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
    let title = params.get("title").and_then(Value::as_str);
    let pinned = params.get("pinned").and_then(Value::as_bool);
    let sort_order = params.get("sort_order").map(|v| v.as_u64());
    let session = kxen_app::core::session::update_meta(&kxen_app::core::paths::sessions_dir(), id, title, pinned, sort_order)
        .map_err(|e| e.to_string())?;
    Ok(json!(session))
}

/// session.delete RPC：统一清理入口（rpc.rs 迁入）。
/// 清理顺序是安全依赖链，不可调换：
/// 1. 先清 pending 队列 + cancel active run token——断粮：run 收尾的续跑逻辑读不到下一条，
///    取消令牌让在跑的 run 尽快落地，防删除后幽灵续跑重建文件；
/// 2. 轮询等 run 落地（3s 超时按失败继续，删除不能被卡死）；
/// 3. 再摘外围引用：approvals / cron / goal / team / extras / picked 授权——它们都可能在 run 结束后回调会话；
/// 4. 最后删文件（queue/compact 随 meta/jsonl 一并清，属同一会话生命周期）。
/// 每步失败只记日志不中断：删除是用户明确意图，半清理好过不清理。
/// goal 标 Canceled 不删文件（审计痕迹）；shadow git 按 workdir 共享，不动。
pub(super) async fn session_delete(params: &Value, state: &crate::AppState) -> Result<Value, String> {
    let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
    let sessions_dir = kxen_app::core::paths::sessions_dir();

    // 删除前兜底蒸馏：持久知识落 notes/，任何失败静默照删（OKF：纯 md 可审计，非 silent auto-write）
    let transcript: Vec<String> = kxen_app::core::session::load_messages(&sessions_dir, id)
        .into_iter()
        .map(|m| {
            m.parts
                .iter()
                .filter_map(|p| match p {
                    kxen_app::core::session::Part::Text { text } | kxen_app::core::session::Part::Context { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|t| !t.is_empty())
        .collect();
    {
        let model = effective_session_model(Some(id), state);
        let store = state.auth_store.lock().map(|s| s.clone()).unwrap_or_default();
        let dir = state.active_workspace.read().expect("workspace").clone();
        let written = kxen_app::knowledge::distill::distill_on_delete(&model, &store, &dir, transcript).await.unwrap_or(0);
        if written > 0 {
            tracing::info!(written, "session distilled before delete");
        }
    }

    // 1. 断粮：清队列 + 取消在跑的 run
    let cleared = state.pending_messages.clear(id);
    if cleared > 0 {
        tracing::info!(cleared, "session delete: pending queue cleared");
    }
    if let Some(token) = kxen_app::core::shared::lock(&state.active_runs).get(id).cloned() {
        token.cancel();
    }

    // 2. 等 run 落地（3s 超时：run 可能卡在不可中断的 IO，删除不能被卡死）
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while kxen_app::core::shared::lock(&state.active_runs).contains_key(id) {
        if std::time::Instant::now() >= deadline {
            tracing::warn!(session = id, "session delete: run still active after 3s, proceeding anyway");
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // 3. 摘外围引用（每步独立容错，只记日志）
    let n = state.approvals.cancel_session(id);
    if n > 0 {
        tracing::info!(n, "session delete: approvals canceled");
    }
    let n = kxen_app::core::schedule::remove_by_session(id);
    if n > 0 {
        tracing::info!(n, "session delete: cron jobs removed");
    }
    let n = kxen_app::core::goal::Goal::cancel_for_session(&kxen_app::core::paths::goals_dir(), id);
    if n > 0 {
        tracing::info!(n, "session delete: goals canceled");
    }
    state.team.drop_session(id);
    state.drop_extras(id);
    state.picked_files.drop_session(id);

    // 4. 删文件（meta/jsonl/compact/queue 一并；trash 可恢复）
    kxen_app::core::session::remove(&sessions_dir, id);
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewind_gate_matrix() {
        // 全绿组合放行
        assert!(rewind_gate(false, false, false, true).is_ok());
        assert!(rewind_gate(false, true, true, true).is_ok());
        // 活跃 run：其余条件再好也拒绝
        assert!(rewind_gate(true, false, false, true).unwrap_err().contains("正在运行"));
        // 消息不在本 session
        assert!(rewind_gate(false, false, false, false).unwrap_err().contains("not found"));
        // 脏且无确认拒绝；带确认放行
        assert!(rewind_gate(false, true, false, true).unwrap_err().contains("confirm"));
        assert!(rewind_gate(false, true, true, true).is_ok());
    }

    #[test]
    fn parse_model_override_contract() {
        // 同给 = 写覆盖
        let over = parse_model_override(&json!({ "provider": "xai", "model": "grok" })).unwrap();
        assert_eq!(over.map(|m| (m.provider, m.model)), Some(("xai".to_string(), "grok".to_string())));
        // 同缺 = 清除覆盖（跟随全局默认）
        assert!(parse_model_override(&json!({})).unwrap().is_none());
        // 只给一个 = 调用方错误
        assert!(parse_model_override(&json!({ "provider": "xai" })).is_err());
        assert!(parse_model_override(&json!({ "model": "grok" })).is_err());
    }
}
