//! DCP Agent 定义（design.md「AI 编写的 DCP Agent」）：Markdown + 行式 frontmatter 数据文件，
//! 存 `<workspace>/.kxen/kanban/agents/<name>.md`。主线程经 kanban_agent_create（P2b 工具面）写入，
//! 列触发器（driver.rs）按 on_enter.agent 引用加载。
//!
//! frontmatter 与 subagent/knowledge 同规约（`---` 包围的 key: value 头，不引入 YAML 依赖），
//! 但解析是收紧版 fail-closed：缺字段、未知字段、重名字段、空值、坏名一律拒绝——
//! 定义文件即被调用 Agent 的 system prompt 与权限来源，半截定义放行等于静默注入。
//! permission_profile 决定被调用实例的 allowed_tools（映射 AgentContext.allowed_tools，执行侧
//! 由 agent_loop::tool_permitted 复验）；未知 profile 拒绝而非兜底：静默只读会把「跑不了」
//! 藏成「跑完没改」，静默 Full 是权限放大。

use std::path::{Path, PathBuf};

use crate::core::ids;
use crate::core::session::storage;

use super::error::KanbanError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDefinition {
    pub name: String,
    /// mrm 路由角色（model = "auto" 时经 mrm.resolve(role) 选模型）；仅作路由键，不直接赋权。
    pub role: String,
    /// "auto" = 经 mrm 按 role 路由；"provider:model" = 显式钉选（仍走 mrm admission 与计量）。
    pub model: String,
    pub permission_profile: String,
    /// frontmatter 之后的正文：agent_run 列是 prompt；workflow 列复用为 QuickJS 脚本（单一引用面，
    /// 不发明第二套 workflow 存储）。
    pub prompt: String,
}

pub fn agents_dir(workspace: &Path) -> PathBuf {
    workspace.join(".kxen").join("kanban").join("agents")
}

const KEYS: [&str; 4] = ["name", "role", "model", "permission_profile"];

/// profile -> allowed_tools（外层 None = 未知 profile；内层 None = 全部常驻工具，
/// 与 subagent PermissionProfile::Full 同语义）。
pub fn profile_tools(profile: &str) -> Option<Option<&'static [&'static str]>> {
    match profile {
        "readonly" => Some(Some(&["read", "glob", "grep"])),
        // test 追加 exec 跑验证命令：exec 仍逐次过 safety gate / Approval（P3 前无看板自主授权），
        // 白名单只决定「能叫什么工具」，不放大 Safety 判定
        "readonly+test" => Some(Some(&["read", "glob", "grep", "exec"])),
        "full" => Some(None),
        _ => None,
    }
}

fn invalid(reason: impl Into<String>) -> KanbanError {
    KanbanError::InvalidAgentDef(reason.into())
}

pub fn parse(text: &str) -> Result<AgentDefinition, KanbanError> {
    let Some(rest) = text.strip_prefix("---\n") else { return Err(invalid("missing --- frontmatter header")) };
    let Some(end) = rest.find("\n---") else { return Err(invalid("unterminated frontmatter")) };
    let mut fields = std::collections::BTreeMap::new();
    for (index, line) in rest[..end].lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(invalid(format!("frontmatter line {} is not key: value", index + 1)));
        };
        let (key, value) = (key.trim(), value.trim());
        if !KEYS.contains(&key) {
            return Err(invalid(format!("unknown frontmatter field {key:?}")));
        }
        if value.is_empty() {
            return Err(invalid(format!("frontmatter field {key} is empty")));
        }
        if fields.insert(key.to_string(), value.to_string()).is_some() {
            return Err(invalid(format!("duplicate frontmatter field {key}")));
        }
    }
    let mut take = |key: &str| fields.remove(key).ok_or_else(|| invalid(format!("missing frontmatter field {key}")));
    let name = take("name")?;
    let role = take("role")?;
    let model = take("model")?;
    let permission_profile = take("permission_profile")?;
    // name 拼进文件路径，先过 id 白名单（杜绝 ../ 穿越）
    ids::validate_id(&name).map_err(KanbanError::InvalidId)?;
    if profile_tools(&permission_profile).is_none() {
        return Err(invalid(format!("unknown permission_profile {permission_profile:?}")));
    }
    let prompt = rest[end + 4..].trim();
    if prompt.is_empty() {
        return Err(invalid("prompt body is empty"));
    }
    Ok(AgentDefinition { name, role, model, permission_profile, prompt: prompt.to_string() })
}

pub fn to_markdown(definition: &AgentDefinition) -> String {
    format!(
        "---\nname: {}\nrole: {}\nmodel: {}\npermission_profile: {}\n---\n{}\n",
        definition.name, definition.role, definition.model, definition.permission_profile, definition.prompt
    )
}

/// 写定义文件（原子写）：未来 P2b 的 kanban_agent_create 与测试共用此收口，不经第二路径。
pub fn save(workspace: &Path, definition: &AgentDefinition) -> Result<(), KanbanError> {
    // 与 parse 同一套校验：写路径不得比读路径宽，否则垃圾定义能落盘但加载即拒
    parse(&to_markdown(definition))?;
    let path = agents_dir(workspace).join(format!("{}.md", definition.name));
    storage::write_atomic(&path, to_markdown(definition).as_bytes()).map_err(|failure| KanbanError::Log(failure.to_string()))
}

pub fn load(workspace: &Path, name: &str) -> Result<AgentDefinition, KanbanError> {
    // name 来自列配置（BoardCreate/ColumnAdd 已校验列 id，但 agent 名只查非空），此处收口路径安全
    ids::validate_id(name).map_err(KanbanError::InvalidId)?;
    let path = agents_dir(workspace).join(format!("{name}.md"));
    let text = std::fs::read_to_string(&path).map_err(|error| KanbanError::Log(format!("read {}: {error}", path.display())))?;
    let definition = parse(&text)?;
    // 文件名与 frontmatter name 不一致 = 引用面自相矛盾，fail-closed 不猜哪个是真的
    if definition.name != name {
        return Err(invalid(format!("frontmatter name {:?} does not match file name {name:?}", definition.name)));
    }
    Ok(definition)
}

#[cfg(test)]
#[path = "agents/tests.rs"]
mod tests;
