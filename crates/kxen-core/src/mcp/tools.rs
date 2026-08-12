//! MCP 工具桥：server 的工具清单展开为 agent 可见的 ToolDefinition（mcp__server__tool 前缀隔离）。

use super::client::McpTool;
use crate::llm::tool::ToolDefinition;
use std::sync::Arc;

pub(crate) const PROVIDER_TOOL_NAME_MAX: usize = 64;

pub(crate) fn provider_tool_name(server: &str, tool: &str) -> Result<String, String> {
    super::config::validate_server_key(server)?;
    if tool.is_empty() || !tool.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')) {
        return Err("MCP tool name must be non-empty ASCII [A-Za-z0-9_-]".into());
    }
    let exposed = format!("mcp__{server}__{tool}");
    if exposed.len() > PROVIDER_TOOL_NAME_MAX {
        return Err(format!("provider tool name exceeds {PROVIDER_TOOL_NAME_MAX} ASCII bytes"));
    }
    Ok(exposed)
}

pub fn tool_defs(tools: &[McpTool]) -> Vec<ToolDefinition> {
    tools
        .iter()
        .filter_map(|t| match provider_tool_name(&t.server, &t.name) {
            Ok(name) => Some(ToolDefinition::function(name, format!("[mcp:{}] {}", t.server, t.description), t.schema.clone())),
            Err(error) => {
                tracing::warn!(server = t.server, tool = t.name, %error, "invalid MCP tool omitted from provider definitions");
                None
            }
        })
        .collect()
}

/// Server annotations are advisory only. Restricted identities receive only
/// exact locally-derived tool names present in their allowlist.
pub fn tool_defs_for(tools: &[Arc<McpTool>], allowed: Option<&[String]>) -> Vec<ToolDefinition> {
    tools
        .iter()
        .filter_map(|tool| match provider_tool_name(&tool.server, &tool.name) {
            Ok(name) if allowed.is_none_or(|allowed| allowed.contains(&name)) => {
                Some(ToolDefinition::function(name, format!("[mcp:{}] {}", tool.server, tool.description), tool.schema.clone()))
            }
            Ok(_) => None,
            Err(error) => {
                tracing::warn!(server = tool.server, tool = tool.name, %error, "invalid MCP tool omitted from provider definitions");
                None
            }
        })
        .collect()
}

/// 前缀解析：mcp__server__tool -> (server, tool)。
pub fn split_prefixed(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once("__")?;
    super::config::validate_server_key(server).ok()?;
    if tool.is_empty() || !tool.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')) {
        return None;
    }
    (name.len() <= PROVIDER_TOOL_NAME_MAX).then_some((server, tool))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_roundtrip() {
        assert_eq!(split_prefixed("mcp__fs__read_file"), Some(("fs", "read_file")));
        assert_eq!(split_prefixed("agent"), None);
        assert_eq!(split_prefixed("mcp__only"), None);
        assert_eq!(
            split_prefixed("mcp__bad__server__tool"),
            Some(("bad", "server__tool")),
            "the first delimiter is deterministic because server keys cannot contain '__'"
        );
        assert_eq!(split_prefixed("mcp__fs__bad.tool"), None);
    }

    #[test]
    fn provider_name_contract_rejects_illegal_and_oversized_names() {
        assert_eq!(provider_tool_name("safe-server", "read_file").unwrap(), "mcp__safe-server__read_file");
        assert!(provider_tool_name("bad.server", "tool").is_err());
        assert!(provider_tool_name("server", "space tool").is_err());
        let budget = PROVIDER_TOOL_NAME_MAX - "mcp__server__".len();
        assert!(provider_tool_name("server", &"a".repeat(budget)).is_ok());
        assert!(provider_tool_name("server", &"a".repeat(budget + 1)).is_err());
    }

    fn tool(name: &str, read_only: bool) -> McpTool {
        McpTool {
            server: "s".into(),
            name: name.into(),
            description: String::new(),
            schema: serde_json::json!({ "type": "object" }),
            read_only,
        }
    }

    #[test]
    fn unrestricted_keeps_all() {
        let tools = vec![Arc::new(tool("read_file", true)), Arc::new(tool("write_file", false))];
        let defs = tool_defs_for(&tools, None);
        assert_eq!(defs.len(), 2, "非 restricted 角色放行全部 MCP 工具");
    }

    #[test]
    fn restricted_requires_exact_locally_derived_names() {
        let tools = vec![Arc::new(tool("read_file", true)), Arc::new(tool("write_file", false))];
        let allowed = vec!["mcp__s__read_file".to_string()];
        let defs = tool_defs_for(&tools, Some(&allowed));
        assert_eq!(defs.iter().map(|tool| tool.function.name.as_str()).collect::<Vec<_>>(), ["mcp__s__read_file"]);
    }
}
