//! workspace 域 RPC：工作看板卡片数据。

use serde_json::{Value, json};
use std::sync::Arc;

use crate::AppState;

pub(super) const METHODS: &[&str] = &["workspaces.overview"];

pub(super) async fn handle(method: &str, _params: &Value, state: &Arc<AppState>) -> Result<Value, String> {
    match method {
        "workspaces.overview" => {
            let sessions = kxen_core::core::session::list_checked(&kxen_core::core::paths::sessions_dir())
                .map_err(|error| format!("session catalog unavailable: {error}"))?;
            let running: std::collections::HashSet<String> = kxen_core::core::shared::lock(&state.active_runs).keys().cloned().collect();
            let workspaces = kxen_core::core::workspace::list(&kxen_core::core::paths::data_dir()).map_err(|error| error.to_string())?;
            // queue/cron 都是内存快照，一次锁取出
            let queued = state.pending_messages.counts();
            let cron = kxen_core::core::schedule::list()?;
            // goal 一次全量读盘后按会话归属分配：逐 session focus_for 会把磁盘读放大 N 倍
            let goals =
                kxen_core::core::goal::Goal::list_checked(&kxen_core::core::paths::goals_dir()).map_err(|error| error.to_string())?;
            let worktrees = gather_worktrees(&workspaces).await?;
            // 聚合内 dirty_count 是同步 git spawn（每 workspace 一次）：移出 async worker，不卡运行时
            let cards = tokio::task::spawn_blocking(move || {
                let inject = kxen_core::core::workspace::OverviewInjections { worktrees, kanban: gather_kanban(&workspaces) };
                kxen_core::core::workspace::overview(workspaces, &sessions, &running, &queued, &goals, &cron, &inject)
            })
            .await
            .map_err(|e| e.to_string())?;
            Ok(json!(cards))
        }
        _ => Err(format!("unknown method: {method}")),
    }
}

/// 逐 workspace 采集看板摘要（同步读 .kxen/kanban 目录；digest 尽力而为，坏板在 collect 内跳过）。
fn gather_kanban(
    workspaces: &[kxen_core::core::workspace::Workspace],
) -> std::collections::HashMap<String, Vec<kxen_core::kanban::KanbanDigest>> {
    workspaces.iter().map(|w| (w.path.clone(), kxen_core::kanban::collect(std::path::Path::new(&w.path)))).collect()
}

/// 逐 workspace 采集 kxen 隔离树摘要（name/branch/dirty）。
/// 成本门：先查 `<ws>/.kxen/worktrees` 目录存在再 spawn git——没建过隔离树的 workspace 零进程开销；
/// 多 workspace 并发采集（JoinSet）：最近列表可达 20 项，串行 spawn 会把尾延迟放大到秒级。
async fn gather_worktrees(
    workspaces: &[kxen_core::core::workspace::Workspace],
) -> Result<std::collections::HashMap<String, Vec<kxen_core::core::workspace::WorktreeDigest>>, String> {
    let mut set = tokio::task::JoinSet::new();
    for w in workspaces {
        let root = std::path::PathBuf::from(&w.path);
        let worktree_dir = root.join(".kxen").join("worktrees");
        match std::fs::metadata(&worktree_dir) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => return Err(format!("worktree store is not a directory: {}", worktree_dir.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("inspect worktree store {}: {error}", worktree_dir.display())),
        }
        let key = w.path.clone();
        set.spawn(async move {
            let list =
                kxen_core::tools::worktree::list(&root).await.map_err(|error| format!("list worktrees for {}: {error}", root.display()))?;
            let mut out = Vec::with_capacity(list.len());
            for t in list {
                // 单棵树 status 失败以 dirty=None 明确表示 UNKNOWN，不把整张 workspace 卡片降成失败。
                let dirty = kxen_core::tools::worktree::status(&t.path).await.ok().map(|v| v.len());
                out.push(kxen_core::core::workspace::WorktreeDigest {
                    name: t.name,
                    branch: t.branch,
                    path: t.path.to_string_lossy().into_owned(),
                    dirty,
                    // 绑定计数由 overview 聚合填充：采集层拿不到会话列表
                    sessions: 0,
                    running: 0,
                });
            }
            Ok::<_, String>((key, out))
        });
    }
    let mut map = std::collections::HashMap::new();
    while let Some(result) = set.join_next().await {
        let (path, trees) = result.map_err(|error| format!("worktree inspection task failed: {error}"))??;
        map.insert(path, trees);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn worktree_list_failure_is_not_reported_as_empty() {
        let root = std::env::temp_dir().join(format!("kxen-workspace-worktree-error-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join(".kxen/worktrees")).unwrap();
        let workspaces = vec![kxen_core::core::workspace::Workspace { path: root.to_string_lossy().into_owned(), last_used: 0 }];

        let error = gather_worktrees(&workspaces).await.expect_err("non-git workspace with a worktree store must return the list error");
        assert!(error.contains("list worktrees"));
        assert!(error.contains(&root.to_string_lossy().to_string()));
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn missing_worktree_store_is_a_valid_empty_result() {
        let root = std::env::temp_dir().join(format!("kxen-workspace-no-worktrees-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.to_string_lossy().into_owned();
        let workspaces = vec![kxen_core::core::workspace::Workspace { path: path.clone(), last_used: 0 }];

        let gathered = gather_worktrees(&workspaces).await.unwrap();
        assert!(!gathered.contains_key(&path));
        std::fs::remove_dir_all(root).ok();
    }
}
