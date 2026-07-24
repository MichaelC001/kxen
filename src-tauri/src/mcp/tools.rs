//! MCP 工具桥：server 的工具清单展开为 agent 可见的 ToolDefinition（mcp__server__tool 前缀隔离）。

use super::client::McpTool;
use crate::llm::tool::ToolDefinition;

pub fn tool_defs(tools: &[McpTool]) -> Vec<ToolDefinition> {
    tools
        .iter()
        .map(|t| {
            ToolDefinition::function(
                &format!("mcp__{}__{}", t.server, t.name),
                &format!("[mcp:{}] {}", t.server, t.description),
                t.schema.clone(),
            )
        })
        .collect()
}

/// 按角色过滤后的 MCP tool defs（P0-08）：restricted（子代理白名单角色）只保留
/// read_only=true 的工具——无 annotation 的一律视为写工具被滤掉（宁严勿宽）。
pub fn tool_defs_for(tools: &[McpTool], restricted: bool) -> Vec<ToolDefinition> {
    tool_defs(
        &tools
            .iter()
            .filter(|t| !restricted || t.read_only)
            .cloned()
            .collect::<Vec<_>>(),
    )
}

/// 前缀解析：mcp__server__tool -> (server, tool)。
pub fn split_prefixed(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    rest.split_once("__")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_roundtrip() {
        assert_eq!(split_prefixed("mcp__fs__read_file"), Some(("fs", "read_file")));
        assert_eq!(split_prefixed("agent"), None);
        assert_eq!(split_prefixed("mcp__only"), None);
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
        let tools = vec![tool("read_file", true), tool("write_file", false)];
        let defs = tool_defs_for(&tools, false);
        assert_eq!(defs.len(), 2, "非 restricted 角色放行全部 MCP 工具");
    }

    #[test]
    fn restricted_keeps_only_read_only() {
        let tools = vec![tool("read_file", true), tool("write_file", false)];
        let defs = tool_defs_for(&tools, true);
        assert_eq!(defs.len(), 1, "无 annotation 一律视为写工具被滤掉");
        assert_eq!(defs[0].function.name, "mcp__s__read_file");
    }
}
