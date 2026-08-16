//! 列执行的 AgentContext 与模型装配（driver.rs 的执行上下文助手，独立文件守 350 行门禁）。

use std::sync::Arc;

use crate::agent::agent_loop::{AgentContext, PersistTurn};
use crate::agent::cancel::CancelToken;
use crate::llm::ModelRef;

use super::agents;
use super::driver::DriverDeps;

/// 列执行最大工具迭代 16：高于 subagent 单角色（6-8），列任务含多轮编辑-验证循环；
/// 触顶 run 以 Error 终态结束，无 VERDICT 落 Failure，可显式重试。
const KANBAN_MAX_TURNS: u32 = 16;

pub(super) async fn resolve_model(definition: &agents::AgentDefinition, deps: &DriverDeps) -> Result<ModelRef, String> {
    if definition.model == "auto" {
        // 派发只选择模型；每次实际请求由 ctx 重新做 admission、RPM 与并发占槽（同 subagent）
        let resolved = deps
            .mrm
            .resolve(&definition.role, &deps.store)
            .await
            .ok_or_else(|| format!("no available model for role {}", definition.role))?;
        return Ok(match resolved.account {
            Some(account) => ModelRef::with_account(resolved.provider, resolved.model, account),
            None => ModelRef::new(resolved.provider, resolved.model),
        });
    }
    let Some((provider, model)) = definition.model.split_once(':') else {
        return Err(format!("model must be \"auto\" or \"provider:model\", got {:?}", definition.model));
    };
    Ok(ModelRef::new(provider, model))
}

pub(super) fn base_context(
    deps: &DriverDeps,
    model: ModelRef,
    allowed: Option<Vec<String>>,
    persist_turn: Option<PersistTurn>,
    cancel: CancelToken,
    auto: Option<Arc<dyn crate::tools::auto_approve::AutoApprove>>,
) -> AgentContext {
    AgentContext {
        registry: deps.registry.clone(),
        tracker: crate::tools::fs_tool::FileTracker::default(),
        workdir: deps.workdir.clone(),
        child_env: None,
        path_grants: Arc::new(Default::default()),
        path_scope: None,
        model,
        store: deps.store.clone(),
        max_turns: KANBAN_MAX_TURNS,
        max_pure_retries: None,
        mrm: Some(deps.mrm.clone()),
        allowed_tools: allowed,
        extras: None,
        hooks: deps.hooks.clone(),
        loop_detector: crate::agent::loop_detect::LoopDetector::new(),
        cancel: Some(cancel),
        team: None,
        team_identity: None,
        session_id: None,
        exec_scope: None,
        bound_goal_id: None,
        // kanban run 不绑定用户 goal：focus 查找会把列执行挂到无关 goal 的 deadline/记账下
        goal_binding_frozen: true,
        agents: None,
        bus: Some(deps.bus.clone()),
        approvals: deps.approvals.clone(),
        kanban_auto: auto,
        mcp: deps.mcp.clone(),
        mcp_approval_prechecked: false,
        lsp: deps.lsp.clone(),
        notify: None,
        persist_compaction: None,
        persist_turn,
        tool_journal: None,
        domain_tools: None,
        code_orchestration: true,
        auxiliary_usage: Arc::default(),
        usage_reporter: deps.usage_reporter.clone(),
        on_event: Arc::new(|_| ()),
        stream_override: deps.stream_override.clone(),
    }
}

#[cfg(test)]
#[path = "context/tests.rs"]
mod tests;
