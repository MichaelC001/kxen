//! session.context_stats：composer 上下文占用指示的组成明细。
//! 口径：三段拆分（系统提示词/工具定义/对话消息）均为 chars/4 粗估
//! （与 agent::compact::estimate_tokens 同口径），前端一律带 ~ 展示，不与精确值对账；
//! last_input_tokens 是最近一次 run 的 provider 实测输入（唯一精确锚点，无实测 = null）。
//! 已知缺口不虚构：系统提示词不含 run 期 involved 文件/task query 激活的动态知识段，
//! 工具定义不含 MCP/动态工具（占用只可能更高，明细是已知下限口径的估算）。

use serde_json::{Value, json};
use std::sync::Arc;

use crate::AppState;

/// chars/4 粗估（composer 预估与 compaction 触发同口径）。
fn estimate_chars(chars: usize) -> u64 {
    (chars / 4) as u64
}

/// 对话消息占用估算：检查点重建后的模型视角历史 -> flatten -> estimate_tokens。
pub(crate) fn message_tokens_estimate(sessions_dir: &std::path::Path, session_id: &str) -> std::io::Result<u64> {
    let view = kxen_core::core::session::load_history_checked(sessions_dir, session_id)?;
    Ok(kxen_core::agent::compact::estimate_tokens(&kxen_core::agent::compact::flatten_stored(&view)))
}

/// 工具定义占用估算：core + deferred 的 wire JSON。MCP/动态工具不在内（见模块注释）。
pub(crate) fn tool_tokens_estimate() -> u64 {
    let tools: Vec<_> =
        kxen_core::agent::tools_spec::core_tools().into_iter().chain(kxen_core::agent::tools_spec::deferred_tools()).collect();
    estimate_chars(serde_json::to_string(&tools).map(|s| s.len()).unwrap_or(0))
}

pub(super) async fn context_stats_report(params: &Value, state: &Arc<AppState>) -> Result<Value, String> {
    let session_id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
    let sessions_dir = kxen_core::core::paths::KxenPaths::user().sessions_dir();
    let meta = kxen_core::core::session::load_meta(&sessions_dir, session_id)
        .map_err(|error| format!("context_stats session {session_id} unavailable: {error}"))?;
    let model = super::session_ops::effective_session_model_from_override(Some(session_id), meta.model, state)?;
    // 基础装配（involved 空、无 task query、mrm None）：run 期动态段是逐轮变量，不进估算
    let system = kxen_core::agent::prompt::system_prompt(
        std::path::Path::new(&meta.directory),
        &[],
        Some(session_id),
        kxen_core::core::config::coding_rules_enabled(),
        None,
    )
    .await;
    let message_tokens = message_tokens_estimate(&sessions_dir, session_id)
        .map_err(|error| format!("context_stats history {session_id} unavailable: {error}"))?;
    let last_input = kxen_core::core::shared::lock(&state.session_last_input).get(session_id).copied().filter(|v| *v > 0);
    Ok(json!({
        "system_tokens": estimate_chars(system.len()),
        "tool_tokens": tool_tokens_estimate(),
        "message_tokens": message_tokens,
        "window_tokens": kxen_core::agent::compact::context_window(&model),
        "last_input_tokens": last_input,
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn message_estimate_counts_text_and_tool_calls_from_disk() {
        let root = std::env::temp_dir().join(format!("kxen-ctx-stats-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let session = kxen_core::core::session::create(&root, root.to_str().unwrap()).unwrap();
        let message = kxen_core::core::session::new_message(
            &session.id,
            kxen_core::core::session::Role::User,
            vec![kxen_core::core::session::Part::Text { text: "x".repeat(400).into() }],
        );
        kxen_core::core::session::append_message(&root, &message).unwrap();

        let tokens = super::message_tokens_estimate(&root, &session.id).unwrap();
        assert_eq!(tokens, 100, "400 字符按 chars/4 估 100 tokens");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn message_estimate_empty_history_is_zero() {
        // 消息文件缺失 = 会话尚无消息，估 0；会话本身是否存在由 handler 的 load_meta 阻断
        let root = std::env::temp_dir().join(format!("kxen-ctx-stats-missing-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(super::message_tokens_estimate(&root, "ses_none").unwrap(), 0);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn tool_estimate_covers_core_and_deferred() {
        assert!(super::tool_tokens_estimate() > 0);
    }
}
