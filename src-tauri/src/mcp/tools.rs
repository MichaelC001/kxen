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
}
