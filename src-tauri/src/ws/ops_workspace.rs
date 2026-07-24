//! workspace 域 RPC：工作看板卡片数据（仿 ops_provider 分文件模式）。

use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::AppState;

pub(super) const METHODS: &[&str] = &["workspaces.overview"];

pub(super) async fn handle(method: &str, _params: &Value, app: &AppHandle) -> Result<Value, String> {
    match method {
        "workspaces.overview" => {
            let state = app.state::<Arc<AppState>>();
            let sessions = kxen_app::core::session::list(&kxen_app::core::paths::sessions_dir());
            let running: std::collections::HashSet<String> =
                kxen_app::core::shared::lock(&state.active_runs).keys().cloned().collect();
            let workspaces = kxen_app::core::workspace::list(&kxen_app::core::paths::data_dir());
            // queue/cron 都是内存快照，一次锁取出
            let queued = state.pending_messages.counts();
            let cron = kxen_app::core::schedule::list();
            // goal 一次全量读盘后按会话归属分配：逐 session focus_for 会把磁盘读放大 N 倍
            let goals = kxen_app::core::goal::Goal::list(&kxen_app::core::paths::goals_dir());
            let worktrees = gather_worktrees(&workspaces).await;
            // 聚合内 dirty_count 是同步 git spawn（每 workspace 一次）：移出 async worker，不卡运行时
            let cards = tauri::async_runtime::spawn_blocking(move || {
                kxen_app::core::workspace::overview(workspaces, &sessions, &running, &queued, &goals, &cron, &worktrees)
            })
            .await
            .map_err(|e| e.to_string())?;
            Ok(json!(cards))
        }
        _ => Err(format!("unknown method: {method}")),
    }
}

/// 逐 workspace 采集 kxen 隔离树摘要（name/branch/dirty）。
/// 成本门：先查 `<ws>/.kxen/worktrees` 目录存在再 spawn git——没建过隔离树的 workspace 零进程开销；
/// 多 workspace 并发采集（JoinSet）：最近列表可达 20 项，串行 spawn 会把尾延迟放大到秒级。
async fn gather_worktrees(
    workspaces: &[kxen_app::core::workspace::Workspace],
) -> std::collections::HashMap<String, Vec<kxen_app::core::workspace::WorktreeDigest>> {
    let mut set = tokio::task::JoinSet::new();
    for w in workspaces {
        let root = std::path::PathBuf::from(&w.path);
        if !root.join(".kxen").join("worktrees").is_dir() {
            continue;
        }
        let key = w.path.clone();
        set.spawn(async move {
            let list = kxen_app::tools::worktree::list(&root).await.unwrap_or_default();
            let mut out = Vec::with_capacity(list.len());
            for t in list {
                let dirty = kxen_app::tools::worktree::status(&t.path).await.ok().map(|v| v.len());
                out.push(kxen_app::core::workspace::WorktreeDigest {
                    name: t.name,
                    branch: t.branch,
                    path: t.path.to_string_lossy().into_owned(),
                    dirty,
                });
            }
            (key, out)
        });
    }
    let mut map = std::collections::HashMap::new();
    while let Some(Ok((path, trees))) = set.join_next().await {
        map.insert(path, trees);
    }
    map
}
