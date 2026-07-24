//! usage 累计落盘（data_dir/usage.json）：per-session (input, output) tokens，重启恢复。
//! statusline 会话 tokens 段与 usage.overview 全局汇总共用这一份数据（per-session 是最细粒度，
//! 全局累计由它求和，不另存一份会漂移的副本）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn store_file() -> PathBuf {
    crate::core::paths::data_dir().join("usage.json")
}

/// 启动恢复：文件缺失/损坏一律空表（用量非关键数据）
pub fn load() -> HashMap<String, (u64, u64)> {
    load_from(&store_file())
}

/// 原子写（tmp + rename），与 notifications 同策略
pub fn persist(map: &HashMap<String, (u64, u64)>) {
    persist_to(&store_file(), map);
}

fn load_from(path: &Path) -> HashMap<String, (u64, u64)> {
    let Ok(text) = std::fs::read_to_string(path) else { return HashMap::new() };
    serde_json::from_str(&text).unwrap_or_default()
}

fn persist_to(path: &Path, map: &HashMap<String, (u64, u64)>) {
    let Ok(json) = serde_json::to_string_pretty(map) else { return };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let path = std::env::temp_dir().join(format!("kxen-usage-rt-{}.json", std::process::id()));
        let mut map = HashMap::new();
        map.insert("s1".to_string(), (100u64, 20u64));
        map.insert("s2".to_string(), (0u64, 5u64));
        persist_to(&path, &map);
        let loaded = load_from(&path);
        assert_eq!(loaded.get("s1"), Some(&(100, 20)));
        assert_eq!(loaded.get("s2"), Some(&(0, 5)));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_file_yields_empty() {
        let path = std::env::temp_dir().join(format!("kxen-usage-bad-{}.json", std::process::id()));
        std::fs::write(&path, "{{").unwrap();
        assert!(load_from(&path).is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
