//! 角色预设与解析（独立文件守 350 行门禁）：内建权限预设 + 项目 .agents/agents/<role>.md 覆盖。

use serde::Serialize;

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
            // todo 虽常驻但不在白名单（展示侧按名单过滤），与 readonly 同集
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

const BUILTIN_ROLES: &[&str] = &["thinking", "planning", "execution", "review", "research"];

/// role 是否存在：内建集合，或已信任项目的 .agents/agents/<role>.md。
pub(super) fn role_exists(role: &str, workdir: &std::path::Path) -> bool {
    BUILTIN_ROLES.contains(&role)
        || (crate::core::ids::validate_id(role).is_ok()
            && crate::core::trust::is_trusted(workdir)
            && workdir.join(".agents/agents").join(format!("{role}.md")).is_file())
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
