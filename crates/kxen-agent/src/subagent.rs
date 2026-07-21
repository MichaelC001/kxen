//! 角色化 subagent：角色预设（model 经 mrm 路由 + 权限预设 + brief）+ 派发。
//! 角色 brief 全部英文（提示词规则），UI 文案不走这里。

use crate::agent_loop::{run_turn, AgentContext, AgentEvent};
use kxen_llm::mrm::ModelResourceManager;
use kxen_llm::Message;
use serde::Serialize;
use std::path::Path;
use std::sync::Arc;

/// 派发一个 subagent 所需的全部依赖：廉价 Clone，可跨并发派发安全共享。
#[derive(Clone)]
pub struct SubagentDeps {
    pub registry: Arc<kxen_tools::task::TaskRegistry>,
    pub workdir: Arc<Path>,
    pub store: kxen_auth::credential::AuthStore,
    pub mrm: Arc<ModelResourceManager>,
}

impl SubagentDeps {
    pub fn from_context(ctx: &AgentContext) -> Option<Self> {
        Some(Self {
            registry: ctx.registry.clone(),
            workdir: ctx.workdir.clone(),
            store: ctx.store.clone(),
            mrm: ctx.mrm.clone()?,
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
            PermissionProfile::Readonly => &["read"],
            // todo 工具未落地前与 readonly 同集
            PermissionProfile::ReadonlyTodo => &["read"],
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
pub async fn dispatch(role: &str, prompt: String, deps: &SubagentDeps) -> Result<String, String> {
    let agent = role_agent(role);
    let resolved = deps.mrm.resolve(role).await.ok_or_else(|| format!("no available model for role {role}"))?;
    let slot = deps.mrm.acquire(&resolved.provider).await;

    let model = kxen_llm::ModelRef::new(resolved.provider, resolved.model);
    let allowed = agent.permission.allowed_tools();
    let mut child = AgentContext {
        registry: deps.registry.clone(),
        tracker: kxen_tools::fs_tool::FileTracker::default(),
        workdir: deps.workdir.clone(),
        model,
        store: deps.store.clone(),
        max_turns: 6,
        mrm: None,
        allowed_tools: if allowed.is_empty() { None } else { Some(allowed) },
        loop_detector: crate::loop_detect::LoopDetector::new(),
        on_event: {
            let role_owned = agent.name.clone();
            Arc::new(move |event| {
                if let AgentEvent::Error { message } = event {
                    tracing::warn!(role = %role_owned, error = %message, "subagent error");
                }
            })
        },
    };

    let messages = vec![
        Message::system(crate::prompt::subagent_prompt(&agent.name, &agent.prompt)),
        Message::user(prompt),
    ];
    let outcome = run_turn(&mut child, messages).await;
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
