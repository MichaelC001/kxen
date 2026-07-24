//! 参数/路径/结果摘要的小工具函数。

use crate::tools::shell::ShellKind;
use std::path::{Path, PathBuf};

pub fn parse_shell(s: &str) -> Result<ShellKind, String> {
    match s {
        "zsh" => Ok(ShellKind::Zsh),
        "bash" => Ok(ShellKind::Bash),
        "fish" => Ok(ShellKind::Fish),
        other => Err(format!("invalid shell type: {other} (must be zsh/bash/fish)")),
    }
}

pub fn resolve_path(input: &str, workdir: &Path) -> PathBuf {
    let p = PathBuf::from(input);
    if p.is_absolute() { p } else { workdir.join(p) }
}

/// 工具调用一行摘要：按工具提取关键参数（exec=command、fs=path、glob/grep=pattern），
/// 不落原始 JSON——UI 执行行只展示这一条（Claude Code `⏺ Bash(ls -la)` 同款形态）。
pub fn summarize_args(name: &str, arguments: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let get = |key: &str| parsed.get(key)?.as_str().map(String::from);
    let salient = match name {
        "exec" => get("command"),
        "read" | "edit" | "write" | "delete" => get("path"),
        "glob" | "grep" => get("pattern"),
        "agent" => get("role"),
        "skill" => get("name"),
        "knowledge" => get("description").or_else(|| get("action")),
        _ => None,
    };
    first_line(&salient.unwrap_or_else(|| arguments.trim().to_string()), 80)
}

pub fn result_text(result: &Result<String, String>) -> String {
    match result {
        Ok(text) => text.clone(),
        Err(e) => format!("ERROR: {e}"),
    }
}

/// UI 展开体用的结果全文（截 2000 字符防爆）。
/// 收起行只放参数摘要；输出本体进同一张卡的折叠区（Cursor/Cline 单卡形态）。
pub fn result_display(result: &Result<String, String>) -> String {
    let text = result_text(result);
    if text.len() <= 2000 { text } else { format!("{}…", &text[..text.floor_char_boundary(2000)]) }
}

pub fn first_line(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or("");
    if line.len() <= max { line.to_string() } else { format!("{}…", &line[..line.floor_char_boundary(max)]) }
}

/// 内置只读工具集（P2-04 并行判定）：read/glob/grep/search 类，无文件与状态写。
pub fn is_read_only_builtin(name: &str) -> bool {
    const READ_ONLY: &[&str] = &["read", "glob", "grep", "lsp", "webfetch", "websearch"];
    READ_ONLY.contains(&name)
}

/// 只读判定 = 内置只读集 ∪ MCP 显式 read_only 标注；未标注一律视为写（宁严勿宽，同 mcp restricted 口径）。
pub fn is_read_only_tool(name: &str, ctx: &super::context::AgentContext) -> bool {
    if is_read_only_builtin(name) {
        return true;
    }
    if let Some((server, tool)) = crate::mcp::tools::split_prefixed(name) {
        return ctx.mcp.as_ref().is_some_and(|m| m.all_tools().iter().any(|t| t.server == server && t.name == tool && t.read_only));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_extracts_salient_arg() {
        assert_eq!(
            summarize_args("exec", r#"{"command":"ls -la","path":"/x","type":"zsh"}"#),
            "ls -la"
        );
        assert_eq!(summarize_args("read", r#"{"path":"/x/README.md"}"#), "/x/README.md");
        assert_eq!(summarize_args("glob", r#"{"pattern":"**/*.rs"}"#), "**/*.rs");
        assert_eq!(summarize_args("knowledge", r#"{"action":"add","description":"用 trash"}"#), "用 trash");
        // 未知工具/坏 JSON 退化为原文截断
        assert_eq!(summarize_args("mystery", "raw args"), "raw args");
    }
}
