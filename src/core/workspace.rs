//! workspace：多项目目录管理（最近列表持久化 + 当前切换）。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub path: String,
    #[serde(default)]
    pub last_used: u64,
}

/// 持久化最近 workspace 列表（data_dir/workspaces.json）。
pub fn list(dir: &Path) -> Vec<Workspace> {
    let Ok(text) = std::fs::read_to_string(file(dir)) else {
        return Vec::new();
    };
    let mut list: Vec<Workspace> = serde_json::from_str(&text).unwrap_or_default();
    list.sort_by(|a, b| b.last_used.cmp(&a.last_used));
    list
}

/// 记录一次使用（置顶 + 更新时间戳）。
pub fn touch(dir: &Path, path: &str) -> std::io::Result<()> {
    let mut all = list(dir);
    all.retain(|w| w.path != path);
    all.insert(0, Workspace { path: path.into(), last_used: now_ms() });
    all.truncate(20);
    std::fs::create_dir_all(dir)?;
    let tmp = file(dir).with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&all)?)?;
    std::fs::rename(&tmp, file(dir))
}

fn file(dir: &Path) -> PathBuf {
    dir.join("workspaces.json")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_orders_by_recency() {
        let dir = std::env::temp_dir().join(format!("kxen-ws-{}", std::process::id()));
        touch(&dir, "/a").unwrap();
        touch(&dir, "/b").unwrap();
        touch(&dir, "/a").unwrap();
        let all = list(&dir);
        assert_eq!(all[0].path, "/a");
        assert_eq!(all.len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }
}
