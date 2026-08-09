use std::path::{Path, PathBuf};

use crate::kanban::error::KanbanError;
use crate::kanban::events::Outcome;

pub fn turns_path(workspace: &Path, board_id: &str, run_id: &str) -> Result<PathBuf, KanbanError> {
    Ok(crate::kanban::store::board_dir(workspace, board_id)?.join("runs").join(format!("{run_id}.turns.jsonl")))
}

/// 从末轮文本解析显式 verdict：自尾向前取第一条声明（模型可能在前文引用 verdict 字样，以最后声明为准）。
pub fn parse_verdict(final_text: &str) -> Option<Outcome> {
    for line in final_text.lines().rev() {
        let line = line.trim();
        if line.eq_ignore_ascii_case("verdict: success") {
            return Some(Outcome::Success);
        }
        if line.eq_ignore_ascii_case("verdict: failure") {
            return Some(Outcome::Failure);
        }
    }
    None
}
