//! workspace：多项目目录管理（最近列表持久化 + 当前切换）。

use crate::core::shared::now_ms;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub path: String,
    #[serde(default)]
    pub last_used: u64,
}

/// 持久化最近 workspace 列表（data_dir/workspaces.json）。
pub fn list(dir: &Path) -> std::io::Result<Vec<Workspace>> {
    let path = file(dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut list: Vec<Workspace> =
        serde_json::from_str(&text).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    list.sort_by_key(|w| std::cmp::Reverse(w.last_used));
    Ok(list)
}

/// 记录一次使用（置顶 + 更新时间戳）。
pub fn touch(dir: &Path, path: &str) -> std::io::Result<()> {
    use std::io::Write;
    static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = crate::core::shared::lock(&WRITE_LOCK);
    let mut all = list(dir)?;
    all.retain(|w| w.path != path);
    all.insert(0, Workspace { path: path.into(), last_used: now_ms() });
    all.truncate(20);
    std::fs::create_dir_all(dir)?;
    let tmp = file(dir).with_extension("json.tmp");
    let mut output = std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(&tmp)?;
    output.write_all(serde_json::to_string_pretty(&all)?.as_bytes())?;
    output.sync_all()?;
    drop(output);
    std::fs::rename(&tmp, file(dir))?;
    #[cfg(unix)]
    std::fs::File::open(dir)?.sync_all()?;
    Ok(())
}

fn file(dir: &Path) -> PathBuf {
    dir.join("workspaces.json")
}

// ---------------- 工作看板（/workspaces 卡片数据源） ----------------

#[derive(Debug, Clone, Serialize)]
pub struct RunningSession {
    pub id: String,
    pub title: String,
    /// 该会话排队待跑消息数（run 进行中发送的消息在此等续跑）
    pub queued: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeDigest {
    pub name: String,
    pub branch: String,
    pub path: String,
    /// 脏文件数（status 失败为 None，前端不展示计数）
    pub dirty: Option<usize>,
    /// 绑定到该树的会话数（overview 聚合时按 directory 前缀匹配填充，采集层不知会话置 0）
    pub sessions: usize,
    /// 其中运行中会话数
    pub running: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GoalDigest {
    pub id: String,
    pub objective: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceOverview {
    pub path: String,
    pub sessions: usize,
    pub running: usize,
    pub last_activity: u64,
    /// git 脏文件数（非仓库/命令失败为 None，前端不展示该项）
    pub dirty: Option<usize>,
    /// 运行中会话明细（看板「正在跑什么」区）
    pub running_sessions: Vec<RunningSession>,
    /// 该 workspace 的 kxen 隔离树（调用方异步采集注入：git spawn 不进纯函数）
    pub worktrees: Vec<WorktreeDigest>,
    /// 活态 goal 摘要（绑定到本 workspace 会话的最近更新一个）
    pub goal: Option<GoalDigest>,
    /// 该 workspace 的看板摘要（采集层同步注入：读 .kxen/kanban 目录，与 worktrees 同模式）
    pub kanban: Vec<crate::kanban::KanbanDigest>,
    /// 全 workspace 排队消息总数
    pub queued: usize,
    /// 绑定到本 workspace 会话的 cron job 数
    pub cron: usize,
}

/// overview 的注入采集包：昂贵数据全部由调用方采集注入——
/// goals 一次全量读盘、cron/queue 是内存快照、worktree 是异步 git 采集、kanban 是同步目录读。
#[derive(Default)]
pub struct OverviewInjections {
    pub worktrees: HashMap<String, Vec<WorktreeDigest>>,
    pub kanban: HashMap<String, Vec<crate::kanban::KanbanDigest>>,
}

/// 卡片聚合（纯函数，可测）。
pub fn overview(
    workspaces: Vec<Workspace>,
    sessions: &[crate::core::session::Session],
    running: &HashSet<String>,
    queued: &HashMap<String, usize>,
    goals: &[crate::core::goal::Goal],
    cron: &[crate::core::schedule::CronJob],
    inject: &OverviewInjections,
) -> Vec<WorkspaceOverview> {
    workspaces
        .into_iter()
        .map(|w| {
            let mine: Vec<_> = sessions.iter().filter(|s| s.directory == w.path).collect();
            let mine_ids: HashSet<&str> = mine.iter().map(|s| s.id.as_str()).collect();
            let running_sessions: Vec<RunningSession> = mine
                .iter()
                .filter(|s| running.contains(&s.id))
                .map(|s| RunningSession { id: s.id.clone(), title: s.title.clone(), queued: queued.get(&s.id).copied().unwrap_or(0) })
                .collect();
            let mut trees = inject.worktrees.get(&w.path).cloned().unwrap_or_default();
            for t in &mut trees {
                t.sessions = sessions.iter().filter(|s| bound_to(&s.directory, &t.path)).count();
                t.running = sessions.iter().filter(|s| bound_to(&s.directory, &t.path) && running.contains(&s.id)).count();
            }
            WorkspaceOverview {
                sessions: mine.len(),
                running: running_sessions.len(),
                last_activity: mine.iter().map(|s| s.updated_at).max().unwrap_or(w.last_used),
                dirty: dirty_count(&w.path),
                queued: mine_ids.iter().filter_map(|id| queued.get(*id)).sum(),
                cron: cron.iter().filter(|j| mine_ids.contains(j.session_id.as_str())).count(),
                // 全局 goal（session_id=None）不归属任何 workspace：打到每张卡上是噪音
                goal: goals
                    .iter()
                    .filter(|g| live(g) && g.session_id.as_deref().is_some_and(|sid| mine_ids.contains(sid)))
                    .max_by_key(|g| g.updated_at)
                    .map(|g| GoalDigest { id: g.id.clone(), objective: g.contract.objective.clone(), status: g.status.as_str().into() }),
                worktrees: trees,
                kanban: inject.kanban.get(&w.path).cloned().unwrap_or_default(),
                running_sessions,
                path: w.path,
            }
        })
        .collect()
}

/// 活态 = 还在推进或等人介入（与 goal.rs focus 的口径一致）。
fn live(g: &crate::core::goal::Goal) -> bool {
    use crate::core::goal::GoalStatus::*;
    matches!(g.status, Active | Paused | Blocked | BudgetLimited)
}

/// 绑定判定：会话目录落在 worktree 树下（含根部）即算绑定；
/// 用 "path/" 做段边界，防 `exp` 误吞同前缀的 `exp2`。
fn bound_to(dir: &str, tree_path: &str) -> bool {
    dir == tree_path || dir.strip_prefix(tree_path).is_some_and(|suffix| suffix.starts_with('/'))
}

fn dirty_count(path: &str) -> Option<usize> {
    let out = std::process::Command::new("git").args(["-C", path, "status", "--porcelain"]).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).lines().count())
}

#[cfg(test)]
mod tests;
