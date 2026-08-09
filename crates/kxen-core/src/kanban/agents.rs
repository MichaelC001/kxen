//! DCP Agent 定义（design.md「AI 编写的 DCP Agent」）：Markdown + 行式 frontmatter 数据文件，
//! 存 `<workspace>/.kxen/kanban/agents/<name>.md`。主线程经 kanban_agent_create（P2b 工具面）写入，
//! 列触发器（driver.rs）按 on_enter.agent 引用加载。
//!
//! frontmatter 与 subagent/knowledge 同规约（`---` 包围的 key: value 头，不引入 YAML 依赖），
//! 但解析是收紧版 fail-closed：缺字段、未知字段、重名字段、空值、坏名一律拒绝——
//! 定义文件即被调用 Agent 的 system prompt 与权限来源，半截定义放行等于静默注入。
//! permission_profile 决定被调用实例的 allowed_tools（映射 AgentContext.allowed_tools，执行侧
//! 由 agent_loop 白名单复验）；未知 profile 拒绝而非兜底：静默只读会把「跑不了」
//! 藏成「跑完没改」，静默 Full 是权限放大。
//! 四档语义：readonly / readonly+test / full 为固定映射（禁止自带 tools 字段，权限语义单一来源）；
//! custom 必须显式给出 tools（逗号分隔的闭集白名单，见 CUSTOM_TOOL_ALLOWLIST）——
//! 闭集之外的名一律拒绝：mcp__* 远端自报能力不可信，agent/workflow/team/kanban_* 是跨 run
//! 提权派发面，schedule/browser 门控在 kanban 无持久 session 的上下文里本就不成立。

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
    /// custom profile 的显式工具白名单（parse 已过 CUSTOM_TOOL_ALLOWLIST 校验）；固定三档恒为 None。
    pub tools: Option<Vec<String>>,
    /// frontmatter 之后的正文：agent_run 列是 prompt；workflow 列复用为 QuickJS 脚本（单一引用面，
    /// 不发明第二套 workflow 存储）。
    pub prompt: String,
}

pub fn agents_dir(workspace: &Path) -> PathBuf {
    workspace.join(".kxen").join("kanban").join("agents")
}

const KEYS: [&str; 5] = ["name", "role", "model", "permission_profile", "tools"];

/// custom profile 的工具闭集（allowlist 而非 denylist）：只放本地内置、无跨 run 派发能力、
/// 且在 kanban 列上下文真实可用的工具——列执行上下文 extras 为 None，todo/skill 在该上下文
/// 恒报 unavailable，挂进 spec 只会诱导模型反复调用反复拿错。
/// 不在此列的名（mcp__*、agent、workflow、team 系、kanban_*、schedule、tool_search、browser、
/// 未知名）一律拒绝——fail-closed 不猜拼写，也不信远端自报能力。
pub const CUSTOM_TOOL_ALLOWLIST: [&str; 13] =
    ["read", "glob", "grep", "edit", "write", "delete", "exec", "lsp", "webfetch", "websearch", "task", "goal", "knowledge"];

/// custom 工具集校验：非空、无重复、逐项精确命中闭集（工具名全小写，大写即未知名）。
/// command.rs 的 AgentDefined 守卫复用此收口，保证文件口径与事件口径一致。
pub fn validate_custom_tools(tools: &[String]) -> Result<(), KanbanError> {
    if tools.is_empty() {
        return Err(invalid("custom permission_profile requires a non-empty tools list"));
    }
    let mut seen = std::collections::HashSet::new();
    for tool in tools {
        if !CUSTOM_TOOL_ALLOWLIST.contains(&tool.as_str()) {
            return Err(invalid(format!("tool {tool:?} is not in the custom tool allowlist")));
        }
        if !seen.insert(tool) {
            return Err(invalid(format!("duplicate tool {tool:?}")));
        }
    }
    Ok(())
}

/// definition -> allowed_tools：custom 取已校验的显式白名单；固定三档按既有映射
/// （full = None 全部常驻工具，与 subagent PermissionProfile::Full 同语义）；未知 profile 拒绝。
pub fn resolve_allowed_tools(definition: &AgentDefinition) -> Result<Option<Vec<String>>, KanbanError> {
    let owned = |names: &[&str]| names.iter().map(|name| name.to_string()).collect();
    match definition.permission_profile.as_str() {
        "readonly" => Ok(Some(owned(&["read", "glob", "grep"]))),
        // test 追加 exec 跑验证命令：exec 仍逐次过 safety gate / Approval（P3 前无看板自主授权），
        // 白名单只决定「能叫什么工具」，不放大 Safety 判定
        "readonly+test" => Ok(Some(owned(&["read", "glob", "grep", "exec"]))),
        "full" => Ok(None),
        "custom" => Ok(Some(definition.tools.clone().ok_or_else(|| invalid("custom permission_profile requires tools"))?)),
        other => Err(invalid(format!("unknown permission_profile {other:?}"))),
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
    // tools 与 profile 强绑定：custom 必须显式工具集，固定档禁止自带（权限语义单一来源，不允两种口径并存）
    let tools = match permission_profile.as_str() {
        "custom" => {
            let raw = fields.remove("tools").ok_or_else(|| invalid("custom permission_profile requires a tools field"))?;
            let tools: Vec<String> = raw.split(',').map(|tool| tool.trim().to_string()).collect();
            validate_custom_tools(&tools)?;
            Some(tools)
        }
        "readonly" | "readonly+test" | "full" => {
            if fields.remove("tools").is_some() {
                return Err(invalid(format!("permission_profile {permission_profile:?} must not declare tools")));
            }
            None
        }
        other => return Err(invalid(format!("unknown permission_profile {other:?}"))),
    };
    let prompt = rest[end + 4..].trim();
    if prompt.is_empty() {
        return Err(invalid("prompt body is empty"));
    }
    Ok(AgentDefinition { name, role, model, permission_profile, tools, prompt: prompt.to_string() })
}

pub fn to_markdown(definition: &AgentDefinition) -> String {
    let tools = definition.tools.as_ref().map(|tools| format!("tools: {}\n", tools.join(","))).unwrap_or_default();
    format!(
        "---\nname: {}\nrole: {}\nmodel: {}\npermission_profile: {}\n{}---\n{}\n",
        definition.name, definition.role, definition.model, definition.permission_profile, tools, definition.prompt
    )
}

/// 写定义文件（原子写）：未来 P2b 的 kanban_agent_create 与测试共用此收口，不经第二路径。
pub fn save(workspace: &Path, definition: &AgentDefinition) -> Result<(), KanbanError> {
    // 与 parse 同一套校验：写路径不得比读路径宽，否则垃圾定义能落盘但加载即拒
    let markdown = to_markdown(definition);
    parse(&markdown)?;
    let path = agents_dir(workspace).join(format!("{}.md", definition.name));
    storage::write_atomic(&path, markdown.as_bytes()).map_err(|failure| KanbanError::Log(failure.to_string()))
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
