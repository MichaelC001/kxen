//! run loop 的工具白名单与系统提示初始化。

use super::context::AgentContext;
use crate::llm::Message;

pub(super) fn base_tools(ctx: &AgentContext) -> Vec<crate::llm::tool::ToolDefinition> {
    resolve_base_tools(ctx.allowed_tools.as_deref())
}

/// 白名单（Some）按名挂载：先查常驻 core_tools，查不到再查 deferred_tools，两者都命中才并入。
/// WHY 查 deferred：custom DCP kanban agent 的工具集可含 lsp/delete/knowledge/skill 等 deferred 名，
/// 而列执行上下文没有 tool_search 挂载通道（extras=None），定义即挂载是这些工具唯一的 spec 来源；
/// 执行侧本就通（dispatch 按 ctx.lsp 等依赖分发，driver 已注入）。既有三档 profile 只含 core 名，行为不变。
fn resolve_base_tools(allowed: Option<&[String]>) -> Vec<crate::llm::tool::ToolDefinition> {
    let Some(allowed) = allowed else { return crate::agent::tools_spec::core_tools() };
    let core = crate::agent::tools_spec::core_tools();
    let deferred = crate::agent::tools_spec::deferred_tools();
    allowed.iter().filter_map(|name| core.iter().chain(deferred.iter()).find(|tool| tool.function.name == *name).cloned()).collect()
}

pub(super) async fn initialize_system_prompt(ctx: &AgentContext, messages: &mut Vec<Message>) -> (bool, Vec<std::path::PathBuf>) {
    let system_owned = !matches!(messages.first(), Some(message) if message.role == crate::llm::types::Role::System);
    if !system_owned {
        return (false, Vec::new());
    }
    let involved = ctx.tracker.files();
    let embedding_runtime = crate::agent::prompt::embedding_runtime(ctx);
    messages.insert(
        0,
        Message::system(
            crate::agent::prompt::system_prompt_with_embedding(crate::agent::prompt::SystemPromptContext {
                workdir: &ctx.workdir,
                involved: &involved,
                session_id: ctx.session_id.as_deref(),
                coding_rules: crate::core::config::coding_rules_enabled(),
                mrm: ctx.mrm.as_deref(),
                bound_goal_id: ctx.bound_goal_id.as_deref(),
                goal_binding_frozen: ctx.goal_binding_frozen,
                embedding_runtime: embedding_runtime.as_ref(),
            })
            .await,
        ),
    );
    (true, involved)
}

pub(super) fn record_unknown_usage(ctx: &AgentContext, acc: &mut super::usage::UsageAcc, usage_reported: bool) -> Option<String> {
    if usage_reported || ctx.stream_override.is_some() {
        return None;
    }
    if let Some(warning) = crate::core::usage_trend::record_unknown(&ctx.model.provider) {
        tracing::warn!(provider = ctx.model.provider, %warning, "usage metering degraded");
        if let Some(bus) = &ctx.bus {
            bus.publish(crate::core::event::Event::notify(warning, ctx.session_id.clone()));
        }
    }
    acc.record_unknown();
    // Transactional reporters settle UNKNOWN into session + Goal with the
    // same durable operation id. The legacy direct path remains only for
    // non-session/test contexts that have no reporter.
    let result = match (ctx.usage_reporter.is_none(), ctx.bound_goal_id.as_deref()) {
        (true, Some(goal_id)) => super::usage::charge_goal_usage_for(goal_id, None, ctx.bus.as_ref()),
        _ => Ok(None),
    };
    match result {
        Ok(message) => message,
        Err(error) => {
            tracing::error!(%error, "goal UNKNOWN usage persistence failed");
            Some(format!("goal UNKNOWN usage save failed: {error}"))
        }
    }
}

pub(super) fn dispatch_failure(ctx: &AgentContext) -> Option<(super::events::AgentEvent, String)> {
    let message =
        crate::llm::LlmClient::validate_dispatch_in(&ctx.model, &ctx.store, ctx.stream_override.as_ref(), ctx.mrm.as_deref()).err()?;
    let event = super::events::AgentEvent::Error { message: message.clone() };
    (ctx.on_event)(event.clone());
    Some((event, format!("(错误: {message})")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(tools: Vec<crate::llm::tool::ToolDefinition>) -> Vec<String> {
        tools.into_iter().map(|tool| tool.function.name).collect()
    }

    #[test]
    fn whitelist_mounts_core_and_deferred_by_name() {
        let allowed: Vec<String> = ["read", "lsp"].iter().map(|name| name.to_string()).collect();
        let mounted = names(resolve_base_tools(Some(&allowed)));
        assert!(mounted.contains(&"read".to_string()), "core 名照常挂载");
        assert!(mounted.contains(&"lsp".to_string()), "deferred 名定义即挂载: {mounted:?}");
        // 白名单收窄：lsp 不在单内即不出现
        let read_only = vec!["read".to_string()];
        assert_eq!(names(resolve_base_tools(Some(&read_only))), ["read"]);
        // None = 全部常驻（deferred 仍走 tool_search 挂载，不提前进上下文）
        let all = names(resolve_base_tools(None));
        assert!(all.contains(&"exec".to_string()));
        assert!(!all.contains(&"lsp".to_string()), "None 不得混入 deferred: {all:?}");
    }
}
