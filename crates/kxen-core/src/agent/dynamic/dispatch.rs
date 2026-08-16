//! dyn__ 调用分发：注册表查找 -> 参数校验 -> QuickJS 沙箱执行（宿主复用 workflow 引擎）。

use serde_json::Value;

use super::{NAME_PREFIX, validate_args};
use crate::agent::agent_loop::AgentContext;

pub async fn execute_defined(name: &str, args: &Value, ctx: &AgentContext) -> Result<String, String> {
    let extras = ctx.extras.as_deref().ok_or("dynamic tools unavailable in this context")?;
    let def = crate::core::shared::lock(&extras.dynamic_tools).get(name).cloned().ok_or_else(|| format!("unknown dynamic tool: {name}"))?;
    validate_args(&def.parameters, args)?;
    crate::agent::workflow::dynamic::run(&def, args, ctx).await
}

/// resume 复验（DCP fail-closed）：历史里的 dyn__ 调用必须能由当前注册表（宏目录加载结果）
/// 解析；宏目录缺失或内容被改（hash 不符已在加载期拦下）时拒绝继续，按 UNKNOWN 处理。
pub fn verify_history_references(
    history: &[crate::core::session::Message],
    extras: &crate::agent::agent_loop::SessionExtras,
) -> Result<(), String> {
    let registry = crate::core::shared::lock(&extras.dynamic_tools);
    for message in history {
        for part in &message.parts {
            if let crate::core::session::Part::ToolCall { name, .. } = part
                && name.starts_with(NAME_PREFIX)
                && !registry.contains_key(name)
            {
                return Err(format!(
                    "dynamic tool {name} referenced by session history is unavailable: macro directory missing or content changed (fail closed)"
                ));
            }
        }
    }
    Ok(())
}
