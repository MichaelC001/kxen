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

// ---------------- 中心看板（/workspaces 卡片数据源） ----------------

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceOverview {
    pub path: String,
    pub sessions: usize,
    pub running: usize,
    pub last_activity: u64,
    /// git 脏文件数（非仓库/命令失败为 None，前端不展示该项）
    pub dirty: Option<usize>,
}

/// 卡片聚合：会话数 + 运行中数 + 最近活动（会话 updated_at 优先，回落 workspace last_used）。
pub fn overview(
    workspaces: Vec<Workspace>,
    sessions: &[crate::core::session::Session],
    running: &std::collections::HashSet<String>,
) -> Vec<WorkspaceOverview> {
    workspaces
        .into_iter()
        .map(|w| {
            let mine: Vec<_> = sessions.iter().filter(|s| s.directory == w.path).collect();
            WorkspaceOverview {
                sessions: mine.len(),
                running: mine.iter().filter(|s| running.contains(&s.id)).count(),
                last_activity: mine.iter().map(|s| s.updated_at).max().unwrap_or(w.last_used),
                dirty: dirty_count(&w.path),
                path: w.path,
            }
        })
        .collect()
}

fn dirty_count(path: &str) -> Option<usize> {
    let out = std::process::Command::new("git").args(["-C", path, "status", "--porcelain"]).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).lines().count())
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

    #[test]
    fn overview_aggregates_sessions() {
        let ws = vec![
            Workspace { path: "/a".into(), last_used: 100 },
            Workspace { path: "/b".into(), last_used: 200 },
        ];
        let session = |id: &str, dir: &str, updated: u64| crate::core::session::Session {
            id: id.into(),
            title: id.into(),
            directory: dir.into(),
            parent_id: None,
            created_at: 0,
            updated_at: updated,
            pinned: false,
            sort_order: None,
        };
        let sessions = vec![session("s1", "/a", 500), session("s2", "/a", 900), session("s3", "/b", 300)];
        let running: std::collections::HashSet<String> = ["s2".to_string()].into_iter().collect();
        let cards = overview(ws, &sessions, &running);
        assert_eq!(cards[0].sessions, 2);
        assert_eq!(cards[0].running, 1);
        assert_eq!(cards[0].last_activity, 900, "会话 updated_at 优先于 workspace last_used");
        assert_eq!(cards[1].sessions, 1);
        assert_eq!(cards[1].running, 0);
        assert_eq!(cards[1].last_activity, 300);
        assert!(cards[0].dirty.is_none(), "/a 非 git 仓库");
    }
}
