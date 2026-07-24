//! 角色化 subagent：角色预设（model 经 mrm 路由 + 权限预设 + brief）+ 派发。
//! 角色 brief 全部英文（提示词规则），UI 文案不走这里。

use crate::agent::agent_loop::{AgentContext, run_turn};
use crate::llm::Message;
use crate::llm::mrm::ModelResourceManager;
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
    /// 父 session 的 extras（None = 无 session 上下文，dispatch 给临时实例）
    pub extras: Option<Arc<crate::agent::agent_loop::SessionExtras>>,
    pub cancel: Option<crate::agent::cancel::CancelToken>,
    pub agents: Arc<crate::agent::activity::AgentRegistry>,
    pub session_id: Option<String>,
    pub bus: crate::core::event::EventBus,
    pub approvals: Option<Arc<crate::agent::approval::ApprovalBroker>>,
    pub mcp: Option<Arc<crate::mcp::McpManager>>,
    pub lsp: Option<Arc<crate::lsp::LspManager>>,
}

impl SubagentDeps {
    pub fn from_context(ctx: &AgentContext) -> Option<Self> {
        Some(Self {
            registry: ctx.registry.clone(),
            workdir: ctx.workdir.clone(),
            store: ctx.store.clone(),
            mrm: ctx.mrm.clone()?,
            hooks: ctx.hooks.clone(),
            extras: ctx.extras.clone(),
            cancel: ctx.cancel.clone(),
            agents: ctx.agents.clone()?,
            session_id: ctx.session_id.clone(),
            bus: ctx.bus.clone()?,
            approvals: ctx.approvals.clone(),
            mcp: ctx.mcp.clone(),
            lsp: ctx.lsp.clone(),
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
    pub max_turns: u32,
}

const READONLY_NOTE: &str = "You have read-only tools; report conclusions with reasoning and never modify files.";

pub fn role_agent(role: &str) -> RoleAgent {
    let (permission, duty, max_turns) = match role {
        "thinking" => (PermissionProfile::Readonly, format!("Deep analysis and option evaluation. {READONLY_NOTE}"), 6),
        "planning" => (PermissionProfile::ReadonlyTodo, format!("Task decomposition and execution planning. {READONLY_NOTE} Output a numbered step plan."), 6),
        "execution" => (PermissionProfile::Full, "Execute the given plan at high speed: edit files, run commands and verify results exactly as instructed. Make no extra design decisions; stop and report when reality diverges from the plan.".to_string(), 8),
        "review" => (PermissionProfile::Readonly, format!("Adversarial review: find bugs, regressions and omissions in the change. {READONLY_NOTE} Output findings ordered by severity."), 6),
        "research" => (PermissionProfile::Readonly, format!("External research: search, read, cross-verify. {READONLY_NOTE} Output conclusions with sources."), 6),
        // 未知 role 兜底只读：可能是模型笔误或信任门拦下的 custom role 回落，此处给 Full 等于静默放大权限
        _ => (PermissionProfile::Readonly, format!("Complete the subtask delegated by the parent agent, staying strictly within its stated boundaries. {READONLY_NOTE}"), 6),
    };
    RoleAgent { name: format!("kxen-{role}"), role: role.to_string(), permission, prompt: duty, max_turns }
}

/// 角色解析：项目 .agents/agents/<role>.md 优先（frontmatter permission/max_turns），缺省回落内建预设。
pub fn role_agent_for(role: &str, workdir: &std::path::Path) -> RoleAgent {
    // role 名来自模型工具参数：先过 id 白名单（拒 ../ 路径穿越），非法名直接回落内建预设
    if crate::core::ids::validate_id(role).is_err() {
        return role_agent(role);
    }
    // 信任门：role 文件即系统提示词注入面，未信任项目的不读取，回落内建预设
    if !crate::core::trust::is_trusted(workdir) {
        return role_agent(role);
    }
    let path = workdir.join(".agents/agents").join(format!("{role}.md"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return role_agent(role);
    };
    let (fm, body) = parse_frontmatter(&text);
    let permission = match fm.get("permission").map(String::as_str) {
        Some("full") => PermissionProfile::Full,
        _ => PermissionProfile::Readonly,
    };
    let max_turns = fm.get("max_turns").and_then(|v| v.parse().ok()).unwrap_or(6);
    RoleAgent {
        name: format!("kxen-{role}"),
        role: role.to_string(),
        permission,
        prompt: if body.is_empty() { fm.get("description").cloned().unwrap_or_default() } else { body },
        max_turns,
    }
}

/// 极简 frontmatter：`---` 包围的 key: value 头 + 剩余正文（与 knowledge 解析同规约，免跨模块）。
fn parse_frontmatter(text: &str) -> (std::collections::HashMap<String, String>, String) {
    let mut map = std::collections::HashMap::new();
    let Some(rest) = text.strip_prefix("---") else { return (map, text.to_string()) };
    let Some(end) = rest.find("\n---") else { return (map, text.to_string()) };
    for line in rest[..end].lines() {
        if let Some((k, v)) = line.split_once(':') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    (map, rest[end + 4..].trim().to_string())
}

/// agent 派发：角色 -> mrm 路由 model -> 独立子 loop -> 结果回传。
/// kind 区分来源（agent 工具 / workflow 的 agent()），统一进活动注册表供 UI 多窗格展示。
pub async fn dispatch(role: &str, prompt: String, deps: &SubagentDeps, kind: crate::agent::activity::AgentKind) -> Result<String, String> {
    let agent = role_agent_for(role, &deps.workdir);
    // 原子 acquire：resolve 与占槽一体，杜绝并发派发时同 provider 超发；grant 持槽整轮
    let grant = deps.mrm.acquire_role(role, &deps.store).await.ok_or_else(|| format!("no available model for role {role}"))?;

    let model = match grant.resolved.account.clone() {
        Some(acc) => crate::llm::ModelRef::with_account(grant.resolved.provider.clone(), grant.resolved.model.clone(), acc),
        None => crate::llm::ModelRef::new(grant.resolved.provider.clone(), grant.resolved.model.clone()),
    };
    let allowed = agent.permission.allowed_tools();
    let session_id = deps.session_id.clone().unwrap_or_else(|| "default".into());
    // 定名 + 注册同一把锁内完成：并发派发同 role 不得同名并条（转录交错根因）
    let name = deps.agents.register_unique(&session_id, role, kind, &model);
    // 子代理独立取消句柄：agents.stop 按名停单个；父 run abort 经 watcher 级联（cancel.rs 的级联共识）。
    // watcher 随 dispatch 结束回收（done_tx drop 即唤醒退出分支），不留驻进程。
    let cancel = crate::agent::cancel::CancelToken::new();
    deps.agents.register_cancel(&session_id, &name, cancel.clone());
    let _cascade = deps.cancel.clone().map(|parent| {
        let child = cancel.clone();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            tokio::select! {
                _ = parent.wait() => child.cancel(),
                _ = done_rx => {}
            }
        });
        done_tx
    });

    let mut child = AgentContext {
        registry: deps.registry.clone(),
        tracker: crate::tools::fs_tool::FileTracker::default(),
        workdir: deps.workdir.clone(),
        model: model.clone(),
        store: deps.store.clone(),
        max_turns: agent.max_turns,
        mrm: None,
        allowed_tools: if allowed.is_empty() { None } else { Some(allowed) },
        // subagent 与父 run 同 session：共享 extras（todo/deferred 工具互通）；
        // deps.extras 为 None 的调用方（test_dispatch 等无 session 上下文）给一次性临时实例
        extras: Some(deps.extras.clone().unwrap_or_default()),
        hooks: deps.hooks.clone(),
        cancel: Some(cancel),
        team: None,
        team_identity: None,
        session_id: Some(session_id.clone()),
        agents: Some(deps.agents.clone()),
        bus: Some(deps.bus.clone()),
        approvals: deps.approvals.clone(),
        mcp: deps.mcp.clone(),
        lsp: deps.lsp.clone(),
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

    let messages = vec![Message::system(crate::agent::prompt::subagent_prompt(&agent.name, &agent.prompt)), Message::user(prompt)];
    let outcome = run_turn(&mut child, messages).await;
    deps.agents.set_status(
        &session_id,
        &name,
        if outcome.aborted { crate::agent::activity::ActivityStatus::Shutdown } else { crate::agent::activity::ActivityStatus::Done },
    );
    drop(grant);
    Ok(outcome.final_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 进程级隔离信任 store：与 render 测试同值（Once 写序防并行 env 竞态）。
    fn setup() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| unsafe {
            std::env::set_var("KXEN_TRUST_FILE", std::env::temp_dir().join(format!("kxen-kn-trust-store-{}.json", std::process::id())));
        });
    }

    fn role_fixture(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kxen-role-{tag}-{}", std::process::id()));
        let agents = dir.join(".agents/agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(
            agents.join("sentry.md"),
            "---\npermission: readonly\nmax_turns: 3\n---\nWatch the perimeter and report anomalies.\n",
        )
        .unwrap();
        dir
    }

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

    #[test]
    fn custom_role_file_overrides_builtin() {
        setup();
        let dir = role_fixture("override");
        crate::core::trust::trust(&dir); // 生产语义：未信任项目 custom role 不读取，夹具显式信任
        let agent = role_agent_for("sentry", &dir);
        assert_eq!(agent.permission, PermissionProfile::Readonly);
        assert_eq!(agent.max_turns, 3);
        assert!(agent.prompt.contains("perimeter"));
        // 未覆盖的内建角色不受影响
        let builtin = role_agent_for("review", &dir);
        assert_eq!(builtin.permission, PermissionProfile::Readonly);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn untrusted_project_custom_role_ignored() {
        setup();
        let dir = role_fixture("untrusted");
        let agent = role_agent_for("sentry", &dir);
        assert!(!agent.prompt.contains("perimeter"), "未信任项目 custom role 文件不得读取");
        assert_eq!(agent.permission, PermissionProfile::Readonly, "未知角色回落必须只读兜底");
        assert_eq!(agent.max_turns, 6);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_role_falls_back_to_readonly() {
        let agent = role_agent("nonexistent-role");
        assert_eq!(agent.permission, PermissionProfile::Readonly, "未知角色不得静默给 Full 权限");
        assert!(agent.prompt.is_ascii(), "role brief must be English");
    }

    #[test]
    fn role_name_traversal_rejected() {
        setup();
        let dir = role_fixture("traversal");
        crate::core::trust::trust(&dir);
        // .agents/agents/../escape.md 落点是 .agents/escape.md：若不做 id 校验会被读出
        std::fs::write(dir.join(".agents/escape.md"), "---\npermission: full\n---\nescaped payload\n").unwrap();
        for bad in ["../escape", "..", "a/b", "a\\b", "a b", "中文字符"] {
            let agent = role_agent_for(bad, &dir);
            assert!(!agent.prompt.contains("escaped payload"), "穿越名 {bad:?} 不得读出文件");
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
