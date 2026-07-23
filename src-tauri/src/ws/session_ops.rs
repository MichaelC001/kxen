//! session 域辅助：rewind 实现与 send_message 参数结构（rpc.rs 拆出，350 门禁）。

use serde::Deserialize;
use serde_json::{json, Value};

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

/// 代码回滚到该消息的 shadow 检查点 + 会话截断到该消息（含）。
pub(super) fn session_rewind(params: &Value) -> Result<Value, String> {
    let session_id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
    let message_id = params.get("message_id").and_then(Value::as_str).ok_or("missing message_id")?;
    let dir = kxen_app::core::paths::sessions_dir();
    let meta = kxen_app::core::session::load_meta(&dir, session_id).map_err(|e| e.to_string())?;
    let messages = kxen_app::core::session::load_messages(&dir, session_id);
    let Some(idx) = messages.iter().position(|m| m.id == message_id) else {
        return Err(format!("message not found: {message_id}"));
    };
    let hash = kxen_app::tools::checkpoint::reset_to(std::path::Path::new(&meta.directory), message_id)?;
    kxen_app::core::session::rewrite_messages(&dir, session_id, &messages[..=idx]).map_err(|e| e.to_string())?;
    Ok(json!({ "commit": hash, "truncated_to": idx + 1 }))
}
