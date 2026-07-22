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

pub fn summarize_args(arguments: &str) -> String {
    let trimmed = arguments.trim();
    if trimmed.len() <= 120 { trimmed.to_string() } else { format!("{}…", &trimmed[..trimmed.floor_char_boundary(120)]) }
}

pub fn result_summary(name: &str, result: &Result<String, String>) -> String {
    match result {
        Ok(text) => format!("{name}: {}", first_line(text, 100)),
        Err(e) => format!("{name} error: {}", first_line(e, 100)),
    }
}

pub fn result_text(result: &Result<String, String>) -> String {
    match result {
        Ok(text) => text.clone(),
        Err(e) => format!("ERROR: {e}"),
    }
}

pub fn first_line(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or("");
    if line.len() <= max { line.to_string() } else { format!("{}…", &line[..line.floor_char_boundary(max)]) }
}
