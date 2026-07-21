// ---------------- inbox ----------------

use serde::Deserialize;
use serde_json::json;
use std::path::Path;

use super::member_loop::now_ms;

#[derive(Debug, Deserialize)]
struct InboxEntry {
    from: String,
    text: String,
    #[serde(default)]
    #[allow(dead_code)]
    at: u64,
}

pub(super) fn append_inbox(dir: &Path, to: &str, from: &str, text: &str) -> Result<(), String> {
    use std::io::Write;
    let path = dir.join("inboxes").join(format!("{to}.json"));
    let entry = json!({ "from": from, "text": text, "at": now_ms() });
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|e| e.to_string())?;
    writeln!(file, "{}", entry).map_err(|e| e.to_string())
}

/// 读 + 校验 + 清空（坏行报错剔除，valid 照常送达——对齐 Claude Code v2.1.207+ 行为）。
pub(super) fn drain_inbox(dir: &Path, name: &str) -> Vec<(String, String)> {
    let path = dir.join("inboxes").join(format!("{name}.json"));
    let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new() };
    let mut out = Vec::new();
    for line in text.lines() {
        match serde_json::from_str::<InboxEntry>(line) {
            Ok(entry) => out.push((entry.from, entry.text)),
            Err(e) => tracing::warn!(inbox = name, error = %e, "dropping malformed inbox entry"),
        }
    }
    let _ = std::fs::write(&path, "");
    out
}
