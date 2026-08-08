//! workspace 级 kanban runner（触发器编排）：周期扫描板状态并驱动 driver（driver.rs）。
//!
//! 调度规则：
//! - Ready 卡 + 可执行列（agent_run/workflow on_enter）-> claim 并执行；
//! - 在飞去重：同一卡片同时只有一个执行任务（P1 守卫保证事件单一写入者，本集合挡调度侧重复拉起）；
//! - 崩溃恢复：open run 的 started_at 早于本进程启动 = 进程死亡遗留（Unknown），提交 run_timeout
//!   停车进 blocked，绝不自动重发（对齐 goal completion「中断置 Unknown」）；
//! - 显式重试：started_at 晚于本进程启动、不在在飞集、未被本进程执行过的 open run = 外部显式
//!   claim（人/工具的 RunStarted 命令），runner 收养执行——显式重试由此生效，且不与遗留恢复混淆；
//! - handled 集：本进程已执行过的 run 不再自动重跑（outcome 落不了时防止重复付费）。
//!
//! human_gate 停车/approve/comment/reject 的 Command 入口 P1 已齐（CardMove Success=approve、
//! Failure=reject 按 transitions 流转、CardComment 入事件流），本模块不另设路径。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::core::shared::now_ms;

use super::driver::{DriverDeps, execute};
use super::model::{CardStatus, OnEnterKind};
use super::{Board, KanbanCommand};

#[derive(Default)]
struct RunnerInner {
    /// card_id -> 有执行任务在飞（去重）。
    in_flight: HashSet<String>,
    /// run_id -> 本进程已执行（成/败都算），不再自动重跑。
    handled: HashSet<String>,
}

pub struct Runner {
    /// 进程启动时刻：orphan 与显式 claim 的分界线（早于它的 open run 不可能是本进程拉起的）。
    boot_ms: u64,
    inner: Arc<Mutex<RunnerInner>>,
}

impl Default for Runner {
    fn default() -> Self {
        Self::new()
    }
}

impl Runner {
    pub fn new() -> Self {
        Self { boot_ms: now_ms(), inner: Arc::new(Mutex::new(RunnerInner::default())) }
    }

    /// 单轮扫描：orphan 恢复 + 收养显式 claim + 拉起 Ready 卡。返回新拉起的执行数。
    /// 单块板损坏不拖垮整轮：warn 后跳过（fail-closed 在板内，不在扫描器）。
    /// 板有实际变动（派发/落地/停车）时发 KanbanUpdate：UI 靠它失效重拉，不发空信号。
    pub async fn scan_once(&self, workspace: &Path, deps: &DriverDeps) -> Result<usize, String> {
        let mut launched = 0;
        for board_id in super::digest::list_boards(workspace)? {
            let board = match Board::open(workspace, &board_id) {
                Ok(board) => board,
                Err(error) => {
                    tracing::warn!(%board_id, %error, "kanban board open failed during scan");
                    continue;
                }
            };
            let mut changed = self.recover_orphans(workspace, &board);
            for run in board.state().runs.values().filter(|run| run.outcome.is_none()) {
                if run.started_at < self.boot_ms {
                    continue; // 遗留 run 已由 recover_orphans 处置
                }
                if !self.claim_card(&run.card_id) {
                    continue;
                }
                // 收养即负责：立刻进 handled，本进程对同一 claim 只执行一次
                crate::core::shared::lock(&self.inner).handled.insert(run.id.clone());
                self.spawn_run(workspace.to_path_buf(), board_id.clone(), run.card_id.clone(), Some(run.id.clone()), deps);
                launched += 1;
                changed += 1;
            }
            for card in board.state().cards.values() {
                if card.status != CardStatus::Ready {
                    continue;
                }
                let Some(column) = board.state().column(&card.column_id) else { continue };
                if !matches!(column.on_enter.kind, OnEnterKind::AgentRun | OnEnterKind::Workflow) {
                    continue;
                }
                if !self.claim_card(&card.id) {
                    continue;
                }
                self.spawn_run(workspace.to_path_buf(), board_id.clone(), card.id.clone(), None, deps);
                launched += 1;
                changed += 1;
            }
            if changed > 0 {
                deps.bus.publish(crate::core::event::Event::KanbanUpdate {
                    board_id: board_id.clone(),
                    workspace: workspace.to_string_lossy().into_owned(),
                });
            }
        }
        Ok(launched)
    }

    /// 进程死亡遗留的 open run：started_at 早于本进程启动（本进程拉起的 run 必然晚于 boot）。
    /// 结果不可知 -> run_timeout 停车 + 审计评论；重试必须显式（新 claim 晚于 boot，走收养路径）。
    /// 返回实际改动数（落盘的评论/停车事件），供 scan_once 决定是否广播。
    fn recover_orphans(&self, workspace: &Path, board: &Board) -> usize {
        let orphans: Vec<(String, String)> = board
            .state()
            .runs
            .values()
            .filter(|run| run.outcome.is_none() && run.started_at < self.boot_ms)
            .map(|run| (run.id.clone(), run.card_id.clone()))
            .collect();
        if orphans.is_empty() {
            return 0;
        }
        let mut board = match Board::open(workspace, &board.state().board_id) {
            Ok(board) => board,
            Err(error) => {
                tracing::warn!(%error, "kanban orphan recovery failed to reopen board");
                return 0;
            }
        };
        let mut changed = 0;
        for (run_id, card_id) in orphans {
            tracing::warn!(%run_id, "kanban run recovered as UNKNOWN after process restart");
            if board
                .apply(KanbanCommand::CardComment {
                    card_id,
                    author: "kanban-runner".into(),
                    body: format!("run {run_id} recovered as UNKNOWN after process restart; blocked pending explicit retry"),
                })
                .is_ok()
            {
                changed += 1;
            }
            match board.apply(KanbanCommand::RunTimeout { run_id: run_id.clone() }) {
                Ok(_) => changed += 1,
                Err(error) => tracing::warn!(%run_id, %error, "kanban orphan run_timeout failed"),
            }
        }
        changed
    }

    fn claim_card(&self, card_id: &str) -> bool {
        crate::core::shared::lock(&self.inner).in_flight.insert(card_id.to_string())
    }

    fn spawn_run(&self, workspace: PathBuf, board_id: String, card_id: String, adopt: Option<String>, deps: &DriverDeps) {
        let inner = self.inner.clone();
        let deps = deps.clone();
        tokio::spawn(async move {
            let result = execute(&workspace, &board_id, &card_id, &deps, adopt).await;
            let run_id = match &result {
                Ok(landing) => Some(landing.run_id.clone()),
                Err(failure) => {
                    tracing::warn!(%card_id, error = %failure.message, "kanban column run failed");
                    failure.run_id.clone()
                }
            };
            let mut inner = crate::core::shared::lock(&inner);
            inner.in_flight.remove(&card_id);
            // claim 过的 run 记入 handled：即使 outcome 没落上，本进程也不自动重跑（防重复付费）
            if let Some(run_id) = run_id {
                inner.handled.insert(run_id);
            }
        });
    }
}

/// background_jobs 挂载点：扫当前活跃 workspace 的看板并驱动列执行。无看板目录直接返回（零开销）。
pub async fn tick(state: &crate::AppState) {
    let root = match state.active_workspace.read() {
        Ok(root) => root.clone(),
        Err(_) => return,
    };
    if !root.join(".kxen").join("kanban").is_dir() {
        return;
    }
    let store = crate::core::shared::lock(&state.auth_store).clone();
    let runtime = match state.workspace_runtimes.runtime(&root) {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::warn!(%error, "kanban tick workspace runtime unavailable");
            return;
        }
    };
    let deps = DriverDeps {
        registry: state.registry.clone(),
        workdir: Arc::from(root.as_path()),
        store,
        mrm: runtime.mrm(),
        hooks: Some(runtime.hooks()),
        bus: state.bus.clone(),
        approvals: Some(state.approvals.clone()),
        agents: state.agents.clone(),
        mcp: Some(runtime.mcp()),
        lsp: Some(runtime.lsp()),
        stream_override: None,
        usage_reporter: None,
    };
    match state.kanban.scan_once(&root, &deps).await {
        Ok(0) => {}
        Ok(launched) => tracing::info!(launched, "kanban column runs launched"),
        Err(error) => tracing::warn!(%error, "kanban scan failed"),
    }
}

#[cfg(test)]
#[path = "runner/tests.rs"]
mod tests;
