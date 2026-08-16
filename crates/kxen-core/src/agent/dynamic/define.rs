//! tool_define 分发：校验 -> 提案/审批 -> 按上下文生效（交互即时注册 / DCP 宏目录下会话生效）。
//!
//! 审批边界（与 knowledge 项目写入同先例，无通道 fail-closed）：
//! - 审批卡展示 name/description/parameters/完整实现源码（reason 为 markdown，前端源码高亮）；
//! - 自主授权通道（DCP DcpAutoApprove / kanban）先落 durable 审计再放行，审计失败回落逐次审批；
//! - 仅 Full 身份可注册：restricted 角色的白名单在执行侧 permits 就挡掉 tool_define（helpers.rs）。

use serde_json::Value;

use super::{DYNAMIC_TOOL_SCHEMA, DynamicToolDef, implementation_hash, qualified_name, snapshot_part, validate_def};
use crate::agent::agent_loop::AgentContext;
use crate::agent::approval::ApprovalOutcome;

pub async fn define(args: &Value, ctx: &AgentContext) -> Result<String, String> {
    let extras = ctx.extras.as_deref().ok_or("tool_define unavailable in this context")?;
    let segment = args.get("name").and_then(Value::as_str).ok_or("missing name")?;
    let description = args.get("description").and_then(Value::as_str).ok_or("missing description")?;
    let parameters = args.get("parameters").cloned().unwrap_or_else(|| serde_json::json!({ "type": "object", "properties": {} }));
    let implementation = args.get("implementation").and_then(Value::as_str).ok_or("missing implementation")?;
    let def = DynamicToolDef {
        schema_version: DYNAMIC_TOOL_SCHEMA,
        name: qualified_name(segment, implementation)?,
        description: description.to_string(),
        parameters,
        implementation: implementation.to_string(),
        implementation_hash: implementation_hash(implementation),
    };
    validate_def(&def)?;
    // 幂等：同名 = 同实现（hash 在名字里），重复注册直接回执，不再打扰审批
    if crate::core::shared::lock(&extras.dynamic_tools).contains_key(&def.name) {
        return Ok(format!("dynamic tool {} is already defined and callable in this session", def.name));
    }
    let macro_dir = crate::core::shared::lock(&extras.dynamic_macro_dir).clone();
    // DCP：提案先落盘（审批不通过也留痕供人工审查），当前 run 一律不生效
    if let Some(dir) = &macro_dir {
        super::macros::propose(dir, &def)?;
    }
    approve(ctx, &format!("tool_define {}", def.name), &render_reason(&def)).await?;
    match macro_dir {
        Some(dir) => {
            let path = super::macros::activate(&dir, &def)?;
            Ok(format!(
                "dynamic tool proposal {} approved and written to {} (audit recorded); it becomes callable as {} in NEW sessions of this policy, not in the current run",
                def.name,
                path.display(),
                def.name
            ))
        }
        None => {
            // 快照先 durable 再注册（与审批规则同口径：写不进事件流的定义不生效，
            // 否则 fork/resume 后注册表与历史不一致）
            let session_id = ctx.session_id.as_deref().ok_or("tool_define requires a durable session")?;
            let broker = ctx.approvals.as_deref().ok_or("tool_define requires an approval channel with session persistence")?;
            broker.append_session_event(session_id, vec![snapshot_part(&def)])?;
            crate::core::shared::lock(&extras.dynamic_tools).insert(def.name.clone(), def.clone());
            Ok(format!(
                "dynamic tool registered: {} — callable for the rest of this session (and after fork/resume). Description: {}",
                def.name, def.description
            ))
        }
    }
}

/// 注册动作审批：自主授权（审计 durable）-> 逐次人工审批 -> 无通道 fail-closed。
pub(super) async fn approve(ctx: &AgentContext, command: &str, reason: &str) -> Result<(), String> {
    if let Some(auto) = ctx.kanban_auto.as_deref()
        && auto.try_auto_allow(command).is_ok()
    {
        return Ok(());
    }
    let Some(appr) = crate::tools::exec::ApprovalCtx::new(
        ctx.approvals.as_deref(),
        ctx.bus.as_ref(),
        ctx.cancel.as_ref(),
        ctx.session_id.as_deref(),
        None,
    ) else {
        return Err(format!("{command} requires user preview and approval; no approval channel is available"));
    };
    match crate::agent::approval::request_approval(&appr, command, reason).await {
        ApprovalOutcome::Allow => Ok(()),
        ApprovalOutcome::Timeout => Err(format!("{command} timed out waiting for approval")),
        ApprovalOutcome::Deny => Err(format!("{command} was denied")),
    }
}

/// 审批卡正文（markdown）：描述 + 参数 schema + 完整实现源码，前端按 code fence 高亮。
fn render_reason(def: &DynamicToolDef) -> String {
    let parameters = serde_json::to_string_pretty(&def.parameters).unwrap_or_else(|_| "{}".into());
    format!(
        "注册动态工具 `{}`（实现内容 hash 进名字，改实现即新名字）。\n\n**描述**：{}\n\n**参数 Schema**：\n```json\n{}\n```\n\n**实现源码**（QuickJS 沙箱，可经 `await tool(name, args)` 组合现有工具）：\n```js\n{}\n```",
        def.name, def.description, parameters, def.implementation
    )
}
