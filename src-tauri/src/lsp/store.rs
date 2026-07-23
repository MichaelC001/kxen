//! 诊断缓存：publishDiagnostics 驱动的 path -> 诊断列表，agent 查询的快照源。

use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub line: u32,   // 1-based（LSP 是 0-based，展示层 +1）
    pub col: u32,    // 1-based
    pub severity: char, // 'E' | 'W' | 'I'
    pub message: String,
}

#[derive(Default)]
pub struct Store {
    by_path: std::sync::Mutex<HashMap<PathBuf, Vec<Diagnostic>>>,
}

impl Store {
    /// publishDiagnostics params -> 更新缓存（空数组 = 该文件诊断清零）。
    pub fn update_from_publish(&self, params: &Value) {
        let Some(uri) = params.get("uri").and_then(Value::as_str) else { return };
        let Some(path) = uri.strip_prefix("file://").map(PathBuf::from) else { return };
        let diags = params
            .get("diagnostics")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(parse_diagnostic).collect())
            .unwrap_or_default();
        self.by_path.lock().expect("lsp store").insert(path, diags);
    }

    /// 快照：path 过滤或全量；空 -> "no diagnostics"。格式 `[E] path:line:col message (rust-analyzer)`。
    pub fn snapshot(&self, filter: Option<&std::path::Path>) -> String {
        let map = self.by_path.lock().expect("lsp store");
        let mut entries: Vec<_> = map.iter().filter(|(p, _)| filter.is_none_or(|f| *p == f)).collect();
        entries.sort_by_key(|(p, _)| p.clone());
        let mut out = String::new();
        for (path, diags) in entries {
            for d in diags {
                out.push_str(&format!("[{}] {}:{}:{} {} (rust-analyzer)\n", d.severity, path.display(), d.line, d.col, d.message));
            }
        }
        if out.is_empty() { "no diagnostics".into() } else { out.trim_end().to_string() }
    }

    pub fn has_entry(&self, path: &std::path::Path) -> bool {
        self.by_path.lock().expect("lsp store").contains_key(path)
    }
}

fn parse_diagnostic(v: &Value) -> Option<Diagnostic> {
    let start = v.get("range")?.get("start")?;
    let severity = match v.get("severity").and_then(Value::as_u64).unwrap_or(3) {
        1 => 'E',
        2 => 'W',
        _ => 'I',
    };
    Some(Diagnostic {
        line: start.get("line").and_then(Value::as_u64)? as u32 + 1,
        col: start.get("character").and_then(Value::as_u64)? as u32 + 1,
        severity,
        message: v.get("message").and_then(Value::as_str).unwrap_or_default().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn publish(uri: &str, diags: Value) -> Value {
        json!({ "uri": uri, "diagnostics": diags })
    }

    #[test]
    fn update_and_snapshot() {
        let store = Store::default();
        store.update_from_publish(&publish(
            "file:///w/src/main.rs",
            json!([{ "range": { "start": { "line": 2, "character": 4 } }, "severity": 1, "message": "expected token" }]),
        ));
        assert_eq!(store.snapshot(None), "[E] /w/src/main.rs:3:5 expected token (rust-analyzer)");
    }

    #[test]
    fn empty_array_clears() {
        let store = Store::default();
        let uri = "file:///w/a.rs";
        store.update_from_publish(&publish(uri, json!([{ "range": { "start": { "line": 0, "character": 0 } }, "severity": 2, "message": "warn" }])));
        store.update_from_publish(&publish(uri, json!([])));
        assert_eq!(store.snapshot(None), "no diagnostics");
    }

    #[test]
    fn path_filter() {
        let store = Store::default();
        store.update_from_publish(&publish("file:///w/a.rs", json!([{ "range": { "start": { "line": 0, "character": 0 } }, "message": "in a" }])));
        store.update_from_publish(&publish("file:///w/b.rs", json!([{ "range": { "start": { "line": 1, "character": 2 } }, "message": "in b" }])));
        let snap = store.snapshot(Some(std::path::Path::new("/w/b.rs")));
        assert_eq!(snap, "[I] /w/b.rs:2:3 in b (rust-analyzer)");
    }
}
