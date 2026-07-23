//! @ 引用的内容注入：chip -> <file_content>/<url_content> 上下文块。
//! 数值依据 docs/research/2026-07-21-agent-ux.md §1：16KB 大纲降级、64KB 单文件 cap、200KB 总量 cap。

use serde::Deserialize;
use std::path::{Path, PathBuf};

const OUTLINE_THRESHOLD: usize = 16 * 1024;
const FILE_CAP: usize = 64 * 1024;
const TOTAL_CAP: usize = 200 * 1024;
const DIR_LIST_CAP: usize = 200;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextItem {
    File { path: String },
    Dir { path: String },
    Web { url: String },
    Docs { url: String },
    /// 纯文本注记（知识沉淀指令等）：注入模型但不属于任何文件。
    Note { text: String },
}

/// 全部 context item -> 拼接的注入文本 + 失败清单（调用方负责把失败告知用户）。
/// 单项失败不致命：降级为错误说明块，让模型知情而不是静默丢失；但用户也必须可见。
pub async fn build_context(items: &[ContextItem], workdir: &Path) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut failures = Vec::new();
    for item in items {
        if out.len() >= TOTAL_CAP {
            out.push_str("\n<context_truncated>total cap 200KB reached, remaining items dropped</context_truncated>\n");
            break;
        }
        let (block, failure) = match item {
            ContextItem::File { path } => file_block(path, workdir),
            ContextItem::Dir { path } => dir_block(path, workdir),
            ContextItem::Web { url } | ContextItem::Docs { url } => web_block(url).await,
            ContextItem::Note { text } => (format!("\n{text}\n"), None),
        };
        if let Some(f) = failure {
            failures.push(f);
        }
        out.push_str(&block);
    }
    (out, failures)
}

fn resolve(input: &str, workdir: &Path) -> PathBuf {
    let p = PathBuf::from(input);
    if p.is_absolute() { p } else { workdir.join(p) }
}

fn file_block(path: &str, workdir: &Path) -> (String, Option<String>) {
    let full = resolve(path, workdir);
    let rel = full.strip_prefix(workdir).unwrap_or(&full).to_string_lossy().into_owned();
    match std::fs::read(&full) {
        Err(e) => (
            format!("\n<file_content path=\"{rel}\">(read failed: {e})</file_content>\n"),
            Some(format!("{rel}（{e}）")),
        ),
        Ok(bytes) if bytes.len() > FILE_CAP => (
            format!("\n<file_content path=\"{rel}\">(file too large: {} bytes > 64KB cap; use the read tool with anchors for specific sections)</file_content>\n", bytes.len()),
            None,
        ),
        Ok(bytes) if bytes.len() > OUTLINE_THRESHOLD => {
            let head = String::from_utf8_lossy(&bytes[..1024.min(bytes.len())]).into_owned();
            (
                format!(
                    "\n<file_content path=\"{rel}\"># First 1KB of {rel} ({} bytes total; use the read tool with anchors for the rest)\n{head}</file_content>\n",
                    bytes.len()
                ),
                None,
            )
        }
        Ok(bytes) => {
            if bytes.contains(&0) {
                return (format!("\n<file_content path=\"{rel}\">(binary file, not shown)</file_content>\n"), None);
            }
            (format!("\n<file_content path=\"{rel}\">\n{}\n</file_content>\n", String::from_utf8_lossy(&bytes)), None)
        }
    }
}

fn dir_block(path: &str, workdir: &Path) -> (String, Option<String>) {
    let full = resolve(path, workdir);
    let rel = full.strip_prefix(workdir).unwrap_or(&full).to_string_lossy().into_owned();
    let Ok(entries) = std::fs::read_dir(&full) else {
        return (
            format!("\n<dir_listing path=\"{rel}\">(not a directory)</dir_listing>\n"),
            Some(format!("{rel}（不是目录或不存在）")),
        );
    };
    let mut lines: Vec<String> = entries
        .flatten()
        .take(DIR_LIST_CAP)
        .map(|e| {
            let suffix = if e.file_type().map(|t| t.is_dir()).unwrap_or(false) { "/" } else { "" };
            format!("{}{}", e.file_name().to_string_lossy(), suffix)
        })
        .collect();
    lines.sort();
    (format!("\n<dir_listing path=\"{rel}\">\n{}\n</dir_listing>\n", lines.join("\n")), None)
}

async fn web_block(url: &str) -> (String, Option<String>) {
    match crate::tools::webfetch::fetch_text(url).await {
        Ok(text) => (format!("\n<url_content url=\"{url}\">\n{text}\n</url_content>\n"), None),
        Err(e) => (
            format!("\n<url_content url=\"{url}\">(fetch failed: {e})</url_content>\n"),
            Some(format!("{url}（{e}）")),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_block_caps_and_outlines() {
        let dir = std::env::temp_dir().join(format!("kxen-ctx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("small.txt"), "hello").unwrap();
        std::fs::write(dir.join("big.txt"), "x".repeat(20 * 1024)).unwrap();
        std::fs::write(dir.join("huge.txt"), "y".repeat(80 * 1024)).unwrap();

        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let items = vec![
            ContextItem::File { path: "small.txt".into() },
            ContextItem::File { path: "big.txt".into() },
            ContextItem::File { path: "huge.txt".into() },
        ];
        let (out, failures) = rt.block_on(build_context(&items, &dir));
        assert!(out.contains("hello"));
        assert!(out.contains("First 1KB of big.txt"), "16KB+ 应走大纲降级");
        assert!(out.contains("64KB cap"), "64KB+ 应被拒绝");
        assert!(failures.is_empty());
        // 硬失败进清单（用户可见）：不存在的文件
        let missing = vec![ContextItem::File { path: "nope.txt".into() }];
        let (_, failures2) = rt.block_on(build_context(&missing, &dir));
        assert_eq!(failures2.len(), 1);
        assert!(failures2[0].contains("nope.txt"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
