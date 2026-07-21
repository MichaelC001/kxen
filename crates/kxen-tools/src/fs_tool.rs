//! 读写删工具：read（锚点输出）/ edit（锚点+兼容双模式 + 免强制先读 + find_shifted 自愈）/ write（trash 删除）。

use crate::hashline::{generate_anchors, render_anchored, Anchor};
use crate::safety::{guard_path, Verdict};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const READ_MAX_LINES: usize = 2000;
const READ_MAX_LINE_CHARS: usize = 2000;

#[derive(Debug, thiserror::Error)]
pub enum FsToolError {
    #[error("safety: {0}")]
    Safety(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("anchor mismatch at line {line}: expected {expected}, found {found}. file changed externally — fresh anchors:\n{fresh}")]
    AnchorMismatch { line: usize, expected: String, found: String, fresh: String },
    #[error("old_string not found (occurrences: {count})")]
    NoMatch { count: usize },
    #[error("old_string ambiguous: {count} occurrences (expected {expected})")]
    Ambiguous { count: usize, expected: usize },
}

// ---------------- 会话内文件新鲜度跟踪（免强制 read-before-edit） ----------------

#[derive(Default)]
pub struct FileTracker {
    seen: Mutex<HashMap<PathBuf, (u64, u64)>>, // path -> (mtime_secs, size)
}

impl FileTracker {
    pub fn mark(&self, path: &Path) {
        if let Ok(meta) = std::fs::metadata(path) {
            let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            self.seen.lock().expect("tracker").insert(path.to_path_buf(), (mtime, meta.len()));
        }
    }

    /// 会话内读过且未外部变更 -> true（可直接 edit）
    pub fn fresh(&self, path: &Path) -> bool {
        let seen = self.seen.lock().expect("tracker");
        let Some((mtime, size)) = seen.get(path) else { return false };
        let Ok(meta) = std::fs::metadata(path) else { return false };
        let now_mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        now_mtime == *mtime && meta.len() == *size
    }
}

// ---------------- read ----------------

#[derive(Debug, Serialize)]
pub struct ReadResult {
    pub content: String,
    pub total_lines: usize,
    pub truncated: bool,
}

pub fn read(path: &Path, tracker: &FileTracker, cwd: &str) -> Result<ReadResult, FsToolError> {
    safety_check(path, cwd)?;
    let text = std::fs::read_to_string(path)?;
    tracker.mark(path);

    let all: Vec<&str> = text.lines().collect();
    let total = all.len();
    let truncated = total > READ_MAX_LINES;
    let taken: Vec<&str> = all.into_iter().take(READ_MAX_LINES).collect();
    let body = taken.join("\n");
    let body = body
        .lines()
        .map(|l| if l.chars().count() > READ_MAX_LINE_CHARS { l.chars().take(READ_MAX_LINE_CHARS).collect::<String>() + "…" } else { l.to_string() })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(ReadResult { content: render_anchored(&body), total_lines: total, truncated })
}

// ---------------- edit ----------------

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum EditSpec {
    Anchors { edits: Vec<AnchorEdit> },
    Match { old_string: String, new_string: String, expected_replacements: Option<usize> },
}

#[derive(Debug, Deserialize)]
pub struct AnchorEdit {
    pub anchor: String,
    pub new_text: String,
}

#[derive(Debug, Serialize)]
pub struct EditResult {
    pub applied: usize,
    pub diff_summary: String,
    pub diff: String,
}

pub fn edit(path: &Path, spec: &EditSpec, tracker: &FileTracker, cwd: &str) -> Result<EditResult, FsToolError> {
    safety_check(path, cwd)?;
    let text = std::fs::read_to_string(path)?;
    let mut lines: Vec<String> = text.lines().map(String::from).collect();

    let before_lines: Vec<String> = text.lines().map(String::from).collect();
    let applied = match spec {
        EditSpec::Anchors { edits } => apply_anchor_edits(&text, &mut lines, edits, path)?,
        EditSpec::Match { old_string, new_string, expected_replacements } => {
            let count = text.matches(old_string.as_str()).count();
            let expected = expected_replacements.unwrap_or(1);
            if count == 0 {
                return Err(FsToolError::NoMatch { count });
            }
            if count != expected {
                return Err(FsToolError::Ambiguous { count, expected });
            }
            let replaced = text.replacen(old_string, new_string, expected);
            lines = replaced.lines().map(String::from).collect();
            expected
        }
    };
    let diff = simple_diff(&before_lines, &lines);

    let trailing = text.ends_with('\n');
    let mut out = lines.join("\n");
    if trailing {
        out.push('\n');
    }
    std::fs::write(path, &out)?;
    tracker.mark(path);

    Ok(EditResult { applied, diff_summary: format!("{applied} edit(s) applied to {}", path.display()), diff })
}

/// 简单 diff：首个不同行起的 before/after（最多各 5 行）。
fn simple_diff(before: &[String], after: &[String]) -> String {
    let mut out = String::new();
    let common = before.iter().zip(after.iter()).take_while(|(a, b)| a == b).count();
    let before_tail = before.iter().skip(common).take(5);
    let after_tail = after.iter().skip(common).take(5);
    for line in before_tail {
        out.push_str(&format!("- {line}\n"));
    }
    for line in after_tail {
        out.push_str(&format!("+ {line}\n"));
    }
    out
}

fn apply_anchor_edits(original: &str, lines: &mut Vec<String>, edits: &[AnchorEdit], _path: &Path) -> Result<usize, FsToolError> {
    let orig_lines: Vec<&str> = original.lines().collect();
    let anchors = generate_anchors(&orig_lines);
    let mut applied = 0;

    for edit in edits {
        let (line_no, expected_hash) = parse_anchor(&edit.anchor).ok_or(FsToolError::NoMatch { count: 0 })?;
        let idx = line_no.saturating_sub(1);
        let current = anchors.get(idx);
        let valid = current.is_some_and(|a| a.hash == expected_hash);

        if !valid {
            // find_shifted：有界窗口内找回
            if let Some(shifted) = find_shifted(&anchors, &orig_lines, line_no, &expected_hash, 20) {
                lines[shifted - 1] = edit.new_text.clone();
                applied += 1;
                continue;
            }
            let found = current.map(|a| a.hash.clone()).unwrap_or_default();
            let fresh = fresh_around(&orig_lines, line_no, 3);
            return Err(FsToolError::AnchorMismatch { line: line_no, expected: expected_hash, found, fresh });
        }
        lines[idx] = edit.new_text.clone();
        applied += 1;
    }
    Ok(applied)
}

fn parse_anchor(anchor: &str) -> Option<(usize, String)> {
    let (line, hash) = anchor.split_once('#')?;
    Some((line.trim().parse().ok()?, hash.trim().to_lowercase()))
}

/// 有界窗口内找回漂移的锚点（恰好一个匹配才用）。
fn find_shifted(anchors: &[Anchor], lines: &[&str], line_no: usize, expected_hash: &str, radius: usize) -> Option<usize> {
    let start = line_no.saturating_sub(radius).max(1);
    let end = (line_no + radius).min(lines.len());
    let mut found: Option<usize> = None;
    for (i, anchor) in anchors.iter().enumerate().take(end).skip(start.saturating_sub(1)) {
        if anchor.hash == expected_hash {
            if found.is_some() {
                return None; // 多匹配，歧义
            }
            found = Some(i + 1);
        }
    }
    found
}

fn fresh_around(lines: &[&str], line_no: usize, radius: usize) -> String {
    let anchors = generate_anchors(lines);
    let start = line_no.saturating_sub(radius + 1);
    let end = (line_no + radius).min(lines.len());
    (start..end)
        .map(|i| format!("{}#{}  {}", anchors[i].line, anchors[i].hash, lines[i]))
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------- write / delete ----------------

pub fn write(path: &Path, content: &str, tracker: &FileTracker, cwd: &str) -> Result<(), FsToolError> {
    safety_check(path, cwd)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() && !tracker.fresh(path) {
        // 覆盖前自动快照（会话级 undo）
        let backup = path.with_extension("kxen-bak");
        std::fs::copy(path, &backup).ok();
    }
    std::fs::write(path, content)?;
    tracker.mark(path);
    Ok(())
}

/// 删除走回收站（macOS /usr/bin/trash）。
pub fn delete(path: &Path, cwd: &str) -> Result<(), FsToolError> {
    safety_check(path, cwd)?;
    let status = std::process::Command::new("/usr/bin/trash").arg(path).status()?;
    if !status.success() {
        return Err(FsToolError::Io(std::io::Error::other(format!("trash failed: {status}"))));
    }
    Ok(())
}

fn safety_check(path: &Path, cwd: &str) -> Result<(), FsToolError> {
    match guard_path(&path.to_string_lossy(), cwd) {
        Verdict::Deny { rule_id, reason, .. } => Err(FsToolError::Safety(format!("{rule_id}: {reason}"))),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hashline::generate_anchors;

    fn temp_file(content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kxen-fstool-{}-{}", std::process::id(), rand()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        std::fs::write(&path, content).unwrap();
        path
    }

    fn rand() -> u32 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0)
    }

    #[test]
    fn anchor_edit_roundtrip() {
        let path = temp_file("alpha\nbeta\ngamma\n");
        let tracker = FileTracker::default();
        tracker.mark(&path);
        let lines: Vec<&str> = "alpha\nbeta\ngamma\n".lines().collect();
        let anchors = generate_anchors(&lines);
        let spec = EditSpec::Anchors { edits: vec![AnchorEdit { anchor: anchors[1].to_string(), new_text: "BETA".into() }] };
        let result = edit(&path, &spec, &tracker, "/tmp").unwrap();
        assert_eq!(result.applied, 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "alpha\nBETA\ngamma\n");
    }

    #[test]
    fn match_edit_ambiguous() {
        let path = temp_file("x\nx\n");
        let tracker = FileTracker::default();
        tracker.mark(&path);
        let spec = EditSpec::Match { old_string: "x".into(), new_string: "y".into(), expected_replacements: None };
        assert!(matches!(edit(&path, &spec, &tracker, "/tmp"), Err(FsToolError::Ambiguous { .. })));
    }

    #[test]
    fn shifted_anchor_recovers() {
        let lines = vec!["a", "b", "c", "d"];
        let anchors = generate_anchors(&lines);
        let shifted = find_shifted(&anchors, &lines, 3, &anchors[2].hash, 5);
        assert_eq!(shifted, Some(3));
    }
}
