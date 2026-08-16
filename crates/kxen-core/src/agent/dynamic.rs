//! 动态工具（dyn__）：模型在运行时定义、审批后生效的 QuickJS 工具，近似插件。
//!
//! 命名 `dyn__<name>_<hash8>`：实现内容的 sha256 前 8 位 hex 进名字——同一 name 改实现即新名，
//! 重定义无歧义，journal/replay 按名字即可校验模板 hash。名字段复用 MCP 的 ASCII/长度口径。
//!
//! 两条生效路径：
//! - 交互会话（D1）：tool_define 审批通过后注册进 SessionExtras.dynamic_tools（即时生效），
//!   定义快照作为 Part::Context 邻接事件落进会话事件流，fork/resume 经 restore_from_history 重建；
//!   tool_undefine 是对称逆操作（同审批口径），卸载事件同通道落流，restore 按序重放。
//! - DCP（D2）：tool_define 只产出宏提案（macros.rs 提案 -> 审批 -> 宏目录），当前 run 不生效；
//!   新 session 在 runner 加载宏目录进注册表后生效，锁只含族名 `dynamic-tools`（闭集特例同 allow_mcp）。
//!   DCP 提案模式下 tool_undefine 拒绝：宏的跨会话生命周期由宏目录文件管理。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::agent_loop::SessionExtras;
use crate::core::identity::ContentHash;
use crate::llm::tool::ToolDefinition;

mod define;
mod dispatch;
pub mod macros;
mod undefine;

pub use define::define;
pub use dispatch::{execute_defined, verify_history_references};
pub use undefine::undefine;

/// 动态工具限定名前缀（dispatch/白名单/审计路由共用）。
pub const NAME_PREFIX: &str = "dyn__";
/// DCP 族能力名：definition 以 `optional: [dynamic-tools]` 预声明，白名单按族名放行
/// tool_define 与 dyn__*（同构 mcp__* 先例）。
pub const FAMILY: &str = "dynamic-tools";
/// provider 工具名上限与 MCP 同口径（64 ASCII 字节）。
pub(crate) const NAME_MAX: usize = crate::mcp::tools::PROVIDER_TOOL_NAME_MAX;
/// 描述上限：进工具清单（每轮请求都带），不许塞长文。
const MAX_DESCRIPTION_CHARS: usize = 500;
/// 会话事件流里的定义快照行标记（Part::Context 文本前缀）。
const SNAPSHOT_MARKER: &str = "[kxen:dynamic-tool]";
/// 会话事件流里的卸载事件行标记（与快照同通道；restore 按序重放）。
const REMOVE_MARKER: &str = "[kxen:dynamic-tool-remove]";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DynamicToolDef {
    pub schema_version: u32,
    /// 限定名 dyn__<name>_<hash8>（hash8 = 实现 sha256 前 8 位 hex）。
    pub name: String,
    pub description: String,
    /// JSON Schema object（参数校验见 validate_args）。
    pub parameters: Value,
    /// QuickJS 脚本：`args` 为冻结入参，`await tool(name, args)` 组合现有工具，顶层 return 一个字符串。
    pub implementation: String,
    pub implementation_hash: ContentHash,
}

pub const DYNAMIC_TOOL_SCHEMA: u32 = 1;

/// 族/前缀闭集特例判定（runtime policy 与 runner 复验共用）：allow_dynamic_tools=false 时一律拒绝。
pub fn is_dynamic_capability(name: &str) -> bool {
    name == FAMILY || name == "tool_define" || name == "tool_undefine" || name.starts_with(NAME_PREFIX)
}

/// 白名单按族名放行（Some 且含 dynamic-tools）；None（Full 身份）走 catalog 常驻，不经此判定。
pub fn family_permitted(allowed: Option<&[String]>) -> bool {
    allowed.is_some_and(|allowed| allowed.iter().any(|name| name == FAMILY))
}

pub fn implementation_hash(implementation: &str) -> ContentHash {
    ContentHash::from_bytes(implementation.as_bytes())
}

fn hash8(hash: &ContentHash) -> &str {
    // "sha256:" 前缀后的前 8 位 hex
    &hash.as_str()["sha256:".len().."sha256:".len() + 8]
}

/// 组限定名并校验：名字段 ASCII 规则同 MCP，总长不超 provider 上限。
pub fn qualified_name(segment: &str, implementation: &str) -> Result<String, String> {
    crate::mcp::tools::validate_tool_name_segment(segment).map_err(|e| format!("dynamic tool name: {e}"))?;
    let qualified = format!("{NAME_PREFIX}{segment}_{}", hash8(&implementation_hash(implementation)));
    if qualified.len() > NAME_MAX {
        return Err(format!("dynamic tool name exceeds {NAME_MAX} ASCII bytes: {qualified}"));
    }
    Ok(qualified)
}

/// 定义自洽校验：限定名必须能由实现内容重算（hash 不符 = 内容被改，fail-closed）。
pub fn validate_def(def: &DynamicToolDef) -> Result<(), String> {
    if def.schema_version != DYNAMIC_TOOL_SCHEMA {
        return Err(format!("unsupported dynamic tool schema {}", def.schema_version));
    }
    let segment = def
        .name
        .strip_prefix(NAME_PREFIX)
        .and_then(|rest| rest.rsplit_once('_').map(|(segment, _)| segment))
        .ok_or_else(|| format!("dynamic tool name must be {NAME_PREFIX}<name>_<hash8>: {}", def.name))?;
    if qualified_name(segment, &def.implementation)? != def.name {
        return Err(format!("dynamic tool name/implementation hash mismatch: {}", def.name));
    }
    if implementation_hash(&def.implementation) != def.implementation_hash {
        return Err(format!("dynamic tool implementation hash mismatch: {}", def.name));
    }
    if def.description.trim().is_empty() || def.description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(format!("dynamic tool description must be 1..={MAX_DESCRIPTION_CHARS} chars"));
    }
    if !def.parameters.is_object() {
        return Err("dynamic tool parameters must be a JSON Schema object".into());
    }
    let impl_len = def.implementation.chars().count();
    let max_impl = crate::core::config::sandbox_config().dynamic_tool_max_implementation_chars();
    if def.implementation.trim().is_empty() || impl_len > max_impl {
        return Err(format!("dynamic tool implementation must be 1..={max_impl} chars"));
    }
    Ok(())
}

/// tool_define 的常驻定义（catalog 内那份是唯一来源，这里只做按名提取）。
pub fn tool_define_definition() -> ToolDefinition {
    crate::agent::tools_spec::core_tool_catalog()
        .iter()
        .find(|tool| tool.function.name == "tool_define")
        .expect("tool_define must be in the core tool catalog")
        .clone()
}

/// 可见动态工具：注册表全量 ∩ 身份口径（None 或族名放行全部；restricted 也可按限定名精确白名单）。
/// 与 deferred_visible 同规：展示侧过滤只决定可见性，执行侧 permits 同口径复验。
pub fn visible_defs(extras: Option<&SessionExtras>, allowed: Option<&[String]>) -> Vec<ToolDefinition> {
    let Some(extras) = extras else { return Vec::new() };
    let registry = crate::core::shared::lock(&extras.dynamic_tools);
    let mut defs: Vec<_> = registry
        .values()
        .filter(|def| match allowed {
            None => true,
            Some(allowed) => family_permitted(Some(allowed)) || allowed.contains(&def.name),
        })
        .map(|def| ToolDefinition::function(def.name.clone(), format!("[dynamic] {}", def.description), def.parameters.clone()))
        .collect();
    defs.sort_by(|left, right| left.function.name.cmp(&right.function.name));
    defs
}

/// 定义快照 -> 会话事件流邻接事件（Part::Context：回放给模型、UI 隐藏，fork/export 随 JSONL 走）。
pub fn snapshot_part(def: &DynamicToolDef) -> crate::core::session::Part {
    let json = serde_json::to_string(def).expect("dynamic tool definition serialization cannot fail");
    crate::core::session::Part::Context { text: format!("{SNAPSHOT_MARKER} {json}").into() }
}

/// 卸载事件 -> 会话事件流邻接事件（与快照同一 Part::Context 通道与 durability 口径）。
pub fn removal_part(name: &str) -> crate::core::session::Part {
    crate::core::session::Part::Context { text: format!("{REMOVE_MARKER} {name}").into() }
}

fn parse_snapshot(text: &str) -> Option<DynamicToolDef> {
    let json = text.strip_prefix(SNAPSHOT_MARKER)?.trim();
    let def: DynamicToolDef = serde_json::from_str(json).ok()?;
    // 快照进注册表前同样过自洽校验：被篡改的史事不会凭空造出工具
    validate_def(&def).ok()?;
    Some(def)
}

fn parse_removal(text: &str) -> Option<&str> {
    let name = text.strip_prefix(REMOVE_MARKER)?.trim();
    // 只认限定名：损坏的卸载事件不产生影响（fail-closed：宁可不卸也不错卸）
    name.starts_with(NAME_PREFIX).then_some(name)
}

/// resume/fork 后重建注册表：按事件流顺序重放——定义快照注册（重复注册幂等：同名同 hash 覆盖同值），
/// 卸载事件摘除。卸载后同限定名再注册（同实现）仍可恢复，与在线执行的注册表终态一致。
pub fn restore_from_history(extras: &SessionExtras, history: &[crate::core::session::Message]) -> usize {
    let mut restored = 0usize;
    {
        let mut registry = crate::core::shared::lock(&extras.dynamic_tools);
        for message in history {
            for part in &message.parts {
                let crate::core::session::Part::Context { text } = part else { continue };
                if let Some(def) = parse_snapshot(text) {
                    if registry.insert(def.name.clone(), def).is_none() {
                        restored += 1;
                    }
                } else if let Some(name) = parse_removal(text) {
                    registry.remove(name);
                }
            }
        }
    }
    restored
}

/// 调用参数按定义 schema 校验：required 必填齐 + 已声明属性的 type 匹配；未声明属性放行
/// （JSON Schema 默认 additionalProperties=true，与 provider 侧口径一致）。
pub fn validate_args(schema: &Value, args: &Value) -> Result<(), String> {
    let object = args.as_object().ok_or("dynamic tool arguments must be a JSON object")?;
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for name in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(name) {
                return Err(format!("missing required argument: {name}"));
            }
        }
    }
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else { return Ok(()) };
    for (name, value) in object {
        let Some(expected) = properties.get(name).and_then(|spec| spec.get("type")).and_then(Value::as_str) else { continue };
        let matches = match expected {
            "string" => value.is_string(),
            "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "boolean" => value.is_boolean(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            _ => true,
        };
        if !matches {
            return Err(format!("argument {name} must be of type {expected}"));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "dynamic/tests.rs"]
mod tests;
#[cfg(test)]
#[path = "dynamic/undefine_tests.rs"]
mod undefine_tests;
