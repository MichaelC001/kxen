//! 角色化 subagent：角色预设（model 经 mrm 路由 + 权限预设 + brief）+ 派发。
//! 角色 brief 全部英文（提示词规则），UI 文案不走这里。

use crate::agent::agent_loop::{run_turn, AgentContext};
use crate::llm::mrm::ModelResourceManager;
use crate::llm::Message;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;

/// 派发一个 subagent 所需的全部依赖：廉价 Clone，可跨并发派发安全共享。
#[derive(Clone)]
pub struct SubagentDeps {
    pub registry: Arc<crate::tools::task::TaskRegistry>,
    pub workdir: Arc<Path>,
    pub store: crate::auth::credential::AuthStore,
    pub mrm: Arc<ModelResourceManager>,
    pub hooks: Option<Arc<crate::tools::hooks::HookRunner>>,
    pub cancel: Option<crate::agent::cancel::CancelToken>,
    pub agents: Arc<crate::agent::activity::AgentRegistry>,
    pub session_id: Option<String>,
    pub bus: crate::core::event::EventBus,
}

impl SubagentDeps {
    pub fn from_context(ctx: &AgentContext) -> Option<Self> {
        Some(Self {
            registry: ctx.registry.clone(),
            workdir: ctx.workdir.clone(),
            store: ctx.store.clone(),
            mrm: ctx.mrm.clone()?,
            hooks: ctx.hooks.clone(),
            cancel: ctx.cancel.clone(),
            agents: ctx.agents.clone()?,
            session_id: ctx.session_id.clone(),
            bus: ctx.bus.clone()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionProfile {
    Readonly,
    ReadonlyTodo,
    Full,
}

impl PermissionProfile {
    /// 允许的工具名（空 = 全部）。注意与 tools_spec 的实际工具名对齐。
    pub fn allowed_tools(&self) -> &'static [&'static str] {
        match self {
            PermissionProfile::Readonly => &["read", "glob", "grep"],
            // todo 经 tool_search 挂载且会话态不继承，与 readonly 同集
            PermissionProfile::ReadonlyTodo => &["read", "glob", "grep"],
            PermissionProfile::Full => &[],
        }
    }
}

impl serde::Serialize for PermissionProfile {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match self {
            PermissionProfile::Readonly => "readonly",
            PermissionProfile::ReadonlyTodo => "readonly-todo",
            PermissionProfile::Full => "full",
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RoleAgent {
    pub name: String,
    pub role: String,
    pub permission: PermissionProfile,
    pub prompt: String,
}

const READONLY_NOTE: &str = "You have read-only tools; report conclusions with reasoning and never modify files.";

pub fn role_agent(role: &str) -> RoleAgent {
    let (permission, duty) = match role {
        "thinking" => (PermissionProfile::Readonly, format!("Deep analysis and option evaluation. {READONLY_NOTE}")),
        "planning" => (PermissionProfile::ReadonlyTodo, format!("Task decomposition and execution planning. {READONLY_NOTE} Output a numbered step plan.")),
        "execution" => (PermissionProfile::Full, "Execute the given plan at high speed: edit files, run commands and verify results exactly as instructed. Make no extra design decisions; stop and report when reality diverges from the plan.".to_string()),
        "review" => (PermissionProfile::Readonly, format!("Adversarial review: find bugs, regressions and omissions in the change. {READONLY_NOTE} Output findings ordered by severity.")),
        "research" => (PermissionProfile::Readonly, format!("External research: search, read, cross-verify. {READONLY_NOTE} Output conclusions with sources.")),
        _ => (PermissionProfile::Full, "Complete the subtask delegated by the parent agent, staying strictly within its stated boundaries.".to_string()),
    };
    RoleAgent { name: format!("kxen-{role}"), role: role.to_string(), permission, prompt: duty }
}

/// agent 派发：角色 -> mrm 路由 model -> 独立子 loop -> 结果回传。
/// kind 区分来源（agent 工具 / workflow 的 agent()），统一进活动注册表供 UI 多窗格展示。
pub async fn dispatch(role: &str, prompt: String, deps: &SubagentDeps, kind: crate::agent::activity::AgentKind) -> Result<String, String> {
    let agent = role_agent(role);
    let resolved = deps.mrm.resolve(role).await.ok_or_else(|| format!("no available model for role {role}"))?;
    let slot = deps.mrm.acquire(&resolved.provider).await;

    let model = crate::llm::ModelRef::new(resolved.provider, resolved.model);
    let allowed = agent.permission.allowed_tools();
    let session_id = deps.session_id.clone().unwrap_or_else(|| "default".into());
    let name = deps.agents.unique_name(&session_id, role);
    deps.agents.register(&session_id, &name, kind, &model);

    let mut child = AgentContext {
        registry: deps.registry.clone(),
        tracker: crate::tools::fs_tool::FileTracker::default(),
        workdir: deps.workdir.clone(),
        model: model.clone(),
        store: deps.store.clone(),
        max_turns: 6,
        mrm: None,
        allowed_tools: if allowed.is_empty() { None } else { Some(allowed) },
        extras: None,
        hooks: deps.hooks.clone(),
        cancel: deps.cancel.clone(),
        team: None,
        team_identity: None,
        session_id: Some(session_id.clone()),
        agents: Some(deps.agents.clone()),
        bus: Some(deps.bus.clone()),
        loop_detector: crate::agent::loop_detect::LoopDetector::new(),
        on_event: {
            let bus = deps.bus.clone();
            let agents = deps.agents.clone();
            let name_event = name.clone();
            let sid = session_id.clone();
            Arc::new(move |event| {
                use serde_json::json;
                let mut payload = match serde_json::to_value(&event) {
                    Ok(v) => v,
                    Err(_) => return,
                };
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("agent".into(), json!(name_event));
                    obj.insert("session_id".into(), json!(sid));
                }
                agents.push_transcript(&sid, &name_event, payload.clone());
                bus.publish(crate::core::event::Event::LlmDelta(payload));
            })
        },
    };

    let messages = vec![
        Message::system(crate::agent::prompt::subagent_prompt(&agent.name, &agent.prompt)),
        Message::user(prompt),
    ];
    let outcome = run_turn(&mut child, messages).await;
    deps.agents.set_status(
        &session_id,
        &name,
        if outcome.aborted { crate::agent::activity::ActivityStatus::Shutdown } else { crate::agent::activity::ActivityStatus::Done },
    );
    drop(slot);
    Ok(outcome.final_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_have_english_briefs() {
        for role in ["thinking", "planning", "execution", "review", "research"] {
            let agent = role_agent(role);
            assert!(agent.name.starts_with("kxen-"));
            assert!(agent.prompt.is_ascii(), "role brief must be English: {role}");
        }
    }

    #[test]
    fn readonly_roles_cannot_write() {
        for role in ["thinking", "review", "research"] {
            let agent = role_agent(role);
            let allowed = agent.permission.allowed_tools();
            assert!(!allowed.is_empty());
            for tool in ["edit", "write", "delete", "exec"] {
                assert!(!allowed.contains(&tool), "{role} must not have {tool}");
            }
        }
    }
}
