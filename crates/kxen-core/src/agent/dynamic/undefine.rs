//! tool_undefine 分发：交互会话内卸载已注册的动态工具（注册的对称逆操作）。
//!
//! 与注册同一审批口径（自主授权 -> 逐次人工审批 -> 无通道 fail-closed）；
//! 卸载事件与定义快照同通道落会话事件流且先 durable 再摘除注册表，
//! fork/resume 经 restore_from_history 按序重放，卸载后状态一致。
//! 历史里已执行的 dyn__ 调用记录不受影响；卸载后的新调用在注册表查找处 fail-closed。
//! DCP 提案模式（宏目录已挂载）下拒绝：宏的跨会话生命周期由宏目录文件管理。

use serde_json::Value;

use super::{DynamicToolDef, NAME_PREFIX, removal_part};
use crate::agent::agent_loop::AgentContext;

pub async fn undefine(args: &Value, ctx: &AgentContext) -> Result<String, String> {
    let extras = ctx.extras.as_deref().ok_or("tool_undefine unavailable in this context")?;
    let name = args.get("name").and_then(Value::as_str).ok_or("missing name")?;
    if !name.starts_with(NAME_PREFIX) {
        return Err(format!("tool_undefine expects a qualified dynamic tool name ({NAME_PREFIX}<name>_<hash8>): {name}"));
    }
    let def = crate::core::shared::lock(&extras.dynamic_tools).get(name).cloned().ok_or_else(|| format!("unknown dynamic tool: {name}"))?;
    if crate::core::shared::lock(&extras.dynamic_macro_dir).is_some() {
        return Err(format!(
            "tool_undefine is unavailable in proposal mode: remove {name} by deleting it from the dynamic-tools macro directory"
        ));
    }
    super::define::approve(ctx, &format!("tool_undefine {name}"), &render_reason(&def)).await?;
    // 与注册同口径：事件先 durable 再摘除注册表（写不进事件流的卸载不生效，
    // 否则 fork/resume 后注册表与历史不一致）
    let session_id = ctx.session_id.as_deref().ok_or("tool_undefine requires a durable session")?;
    let broker = ctx.approvals.as_deref().ok_or("tool_undefine requires an approval channel with session persistence")?;
    broker.append_session_event(session_id, vec![removal_part(name)])?;
    crate::core::shared::lock(&extras.dynamic_tools).remove(name);
    Ok(format!(
        "dynamic tool unregistered: {name} — no longer callable in this session (and after fork/resume); past calls in history are unchanged"
    ))
}

/// 审批卡正文（markdown）：名字 + 描述 + 实现 hash（卸载不执行代码，无需展示源码全文）。
fn render_reason(def: &DynamicToolDef) -> String {
    format!(
        "卸载动态工具 `{}`（仅影响后续调用，历史中已执行的调用记录不变）。\n\n**描述**：{}\n\n**实现 hash**：{}",
        def.name,
        def.description,
        def.implementation_hash.as_str()
    )
}
