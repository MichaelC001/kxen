//! 角色化 subagent：角色预设（model 经 mrm 路由 + 权限预设 + prompt）+ task 派发。

use crate::agent_loop::{run_turn, AgentContext, AgentEvent};
use kxen_llm::mrm::ModelResourceManager;
use kxen_llm::Message;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionProfile {
    Readonly,
    ReadonlyTodo,
    Full,
}

impl PermissionProfile {
    pub fn allowed_tools(&self) -> &'static [&'static str] {
        match self {
            PermissionProfile::Readonly => &["read", "glob", "grep"],
            PermissionProfile::ReadonlyTodo => &["read", "glob", "grep", "todowrite"],
            PermissionProfile::Full => &[],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RoleAgent {
    pub name: String,
    pub role: String,
    pub permission: PermissionProfile,
    pub prompt: String,
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

const READONLY_NOTE: &str = "只读工具（read/glob/grep），输出结论与理由，不修改任何文件。";

pub fn role_agent(role: &str) -> RoleAgent {
    let (permission, duty) = match role {
        "thinking" => (PermissionProfile::Readonly, format!("深度分析与方案评估。{READONLY_NOTE}")),
        "planning" => (PermissionProfile::ReadonlyTodo, format!("任务拆解与执行计划。{READONLY_NOTE}输出分步计划。")),
        "execution" => (PermissionProfile::Full, "高速执行既定计划：按任务直接修改文件、运行命令、验证结果。不做额外设计决策，遇到计划外分歧时停下来报告。".to_string()),
        "review" => (PermissionProfile::Readonly, format!("对抗性审查：找出改动中的 bug、回归与遗漏。{READONLY_NOTE}输出按严重度排序的问题清单。")),
        "research" => (PermissionProfile::Readonly, format!("资料调研：搜索、阅读、交叉验证。{READONLY_NOTE}输出带来源的结论。")),
        _ => (PermissionProfile::Full, "完成主代理委派的子任务，遵循其指令边界。".to_string()),
    };
    RoleAgent { name: format!("kxen-{role}"), role: role.to_string(), permission, prompt: duty }
}

/// task 派发：角色 -> mrm 路由 model -> 独立子 loop -> 结果回传。
pub async fn dispatch(role: &str, prompt: String, parent: &mut AgentContext, mrm: &ModelResourceManager) -> Result<String, String> {
    let agent = role_agent(role);
    let resolved = mrm.resolve(role).await.ok_or_else(|| format!("no available model for role {role}"))?;
    let slot = mrm.acquire(&resolved.provider).await;

    let model = kxen_llm::ModelRef::new(resolved.provider, resolved.model);
    let mut child = AgentContext {
        registry: parent.registry.clone(),
        tracker: kxen_tools::fs_tool::FileTracker::default(),
        workdir: parent.workdir.clone(),
        model,
        store: parent.store.clone(),
        max_turns: 6,
        mrm: None,
        on_event: {
            let role_owned = agent.name.clone();
            Box::new(move |event| {
                if let AgentEvent::Error { message } = event {
                    tracing::warn!(role = %role_owned, error = %message, "subagent error");
                }
            })
        },
    };

    let messages = vec![
        Message::system(format!("你是子代理 {}。{}", agent.name, agent.prompt)),
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
    fn role_presets() {
        let thinking = role_agent("thinking");
        assert_eq!(thinking.name, "kxen-thinking");
        assert_eq!(thinking.permission, PermissionProfile::Readonly);
        let execution = role_agent("execution");
        assert_eq!(execution.permission, PermissionProfile::Full);
    }
}
