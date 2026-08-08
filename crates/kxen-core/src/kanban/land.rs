//! 列执行的 outcome 落点与审计落盘（driver.rs 的持久化助手，独立文件守 350 行门禁）。
//! 全部经 Board::apply（Command -> Event 路径）：不存在直写状态的落地。

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::event::{Event, EventBus};
use crate::core::session as ses;
use crate::llm::ModelRef;

use super::command::Board;
use super::events::{KanbanCommand, Outcome};

/// 落地不经 kanban_rpc 的 commit_and_publish，必须自己补发板粒度失效通知：
/// UI 靠 KanbanUpdate 即时重拉，缺广播就只能等轮询周期才看到落地状态。
pub(super) fn publish_update(bus: &EventBus, workspace: &Path, board_id: &str) {
    bus.publish(Event::KanbanUpdate { board_id: board_id.into(), workspace: workspace.to_string_lossy().into_owned() });
}

/// turns JSONL 追加一行（与 subagent run_line 同基建）：失败置旗并如实上传（fail-closed：
/// 持久化失败的 run 结果按 Unknown 处置，调用方据此落 run_timeout 而非猜 outcome）。
pub(super) fn run_line(
    path: &Path,
    board_id: &str,
    id: String,
    role: ses::Role,
    parts: Vec<ses::Part>,
    model: Option<ModelRef>,
    failed: &AtomicBool,
) -> Result<(), String> {
    let mut message = ses::new_message(board_id, role, parts);
    message.id = id;
    message.model = model;
    match ses::append_line_idempotent(path, &message) {
        Ok(()) => Ok(()),
        Err(error) => {
            failed.store(true, Ordering::Relaxed);
            Err(error.to_string())
        }
    }
}

pub(super) fn land_finished(workspace: &Path, board_id: &str, run_id: &str, outcome: Outcome, bus: &EventBus) -> Result<(), String> {
    let mut board = Board::open(workspace, board_id).map_err(|e| e.to_string())?;
    board.apply(KanbanCommand::RunFinished { run_id: run_id.to_string(), outcome }).map_err(|e| e.to_string())?;
    publish_update(bus, workspace, board_id);
    Ok(())
}

pub(super) fn land_timeout(workspace: &Path, board_id: &str, run_id: &str, bus: &EventBus) -> Result<(), String> {
    let mut board = Board::open(workspace, board_id).map_err(|e| e.to_string())?;
    board.apply(KanbanCommand::RunTimeout { run_id: run_id.to_string() }).map_err(|e| e.to_string())?;
    publish_update(bus, workspace, board_id);
    Ok(())
}

/// 审计评论尽力而为：评论丢失不得翻转已定的 outcome 落地（评论是注记，run 事件才是状态）。
/// 只有 apply 成功才补发失效通知：评论没落盘就没有需要 UI 重拉的新事实。
pub(super) fn comment(workspace: &Path, board_id: &str, card_id: &str, body: String, author: &str, bus: &EventBus) {
    let result = Board::open(workspace, board_id).and_then(|mut board| {
        board.apply(KanbanCommand::CardComment { card_id: card_id.to_string(), author: author.into(), body }).map(|_| ())
    });
    match result {
        Ok(()) => publish_update(bus, workspace, board_id),
        Err(error) => tracing::warn!(%error, "kanban driver comment failed"),
    }
}
