//! 列执行的 outcome 落点与审计落盘（driver.rs 的持久化助手，独立文件守 350 行门禁）。
//! 全部经 Board::apply（Command -> Event 路径）：不存在直写状态的落地。

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::session as ses;
use crate::llm::ModelRef;

use super::command::Board;
use super::events::{KanbanCommand, Outcome};

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

pub(super) fn land_finished(workspace: &Path, board_id: &str, run_id: &str, outcome: Outcome) -> Result<(), String> {
    let mut board = Board::open(workspace, board_id).map_err(|e| e.to_string())?;
    board.apply(KanbanCommand::RunFinished { run_id: run_id.to_string(), outcome }).map_err(|e| e.to_string())?;
    Ok(())
}

pub(super) fn land_timeout(workspace: &Path, board_id: &str, run_id: &str) -> Result<(), String> {
    let mut board = Board::open(workspace, board_id).map_err(|e| e.to_string())?;
    board.apply(KanbanCommand::RunTimeout { run_id: run_id.to_string() }).map_err(|e| e.to_string())?;
    Ok(())
}

/// 审计评论尽力而为：评论丢失不得翻转已定的 outcome 落地（评论是注记，run 事件才是状态）。
pub(super) fn comment(workspace: &Path, board_id: &str, card_id: &str, body: String, author: &str) {
    let result = Board::open(workspace, board_id).and_then(|mut board| {
        board.apply(KanbanCommand::CardComment { card_id: card_id.to_string(), author: author.into(), body }).map(|_| ())
    });
    if let Err(error) = result {
        tracing::warn!(%error, "kanban driver comment failed");
    }
}
