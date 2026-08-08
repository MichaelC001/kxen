//! Workspace 作用域存储：`<workspace>/.kxen/kanban/<board_id>/events.jsonl`（append-only，唯一真源）
//! 与 `snapshot.json`（物化 BoardState，纯缓存，可删；启动校验不符即从事件流重建）。
//! 追加/读取口径与 session/log.rs 相同：torn 行阻断、幂等去重、fsync 落盘，不发明第二套格式。

use std::path::{Path, PathBuf};

use crate::core::ids;
use crate::core::session::storage;

use super::error::KanbanError;
use super::events::KanbanEvent;
use super::projection::{self, BoardState};

pub fn board_dir(workspace: &Path, board_id: &str) -> Result<PathBuf, KanbanError> {
    // board_id 拼进文件路径，必须先过 id 白名单（杜绝路径穿越）
    ids::validate_id(board_id).map_err(KanbanError::InvalidId)?;
    Ok(workspace.join(".kxen").join("kanban").join(board_id))
}

pub fn events_path(board_dir: &Path) -> PathBuf {
    board_dir.join("events.jsonl")
}

pub fn snapshot_path(board_dir: &Path) -> PathBuf {
    board_dir.join("snapshot.json")
}

fn log_error(error: impl std::fmt::Display) -> KanbanError {
    KanbanError::Log(error.to_string())
}

/// 严格读取：文件不存在 = 空事件流；torn/坏行/seq 不连续一律阻断，不能在残缺历史上继续追加或重建。
pub fn load_events(path: &Path) -> Result<Vec<KanbanEvent>, KanbanError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(log_error(error)),
    };
    if !text.is_empty() && !text.ends_with('\n') {
        return Err(log_error(format!("unterminated JSONL record in {} line {}", path.display(), text.lines().count())));
    }
    let events = text
        .lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str::<KanbanEvent>(line)
                .map_err(|error| log_error(format!("parse {} line {}: {error}", path.display(), index + 1)))
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (index, event) in events.iter().enumerate() {
        if event.seq != index as u64 + 1 {
            return Err(log_error(format!("non-contiguous seq in {} line {}: {}", path.display(), index + 1, event.seq)));
        }
    }
    Ok(events)
}

/// 幂等追加（门禁同 append_line_idempotent）：同 id 同 kind 跳过（created_at 允许漂移），同 id 冲突拒绝。
/// seq 在此指派为 len+1——写入侧单一收口，调用方传进来的 seq 字段无意义。
pub fn append_event(path: &Path, event: &mut KanbanEvent) -> Result<(), KanbanError> {
    let existing = load_events(path)?;
    if let Some(found) = existing.iter().find(|item| item.id == event.id) {
        if found.kind != event.kind {
            return Err(log_error(format!("event id collision: {}", event.id)));
        }
        event.seq = found.seq;
        event.created_at = found.created_at;
        return Ok(());
    }
    event.seq = existing.len() as u64 + 1;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(log_error)?;
    }
    let mut line = serde_json::to_vec(&event).map_err(log_error)?;
    line.push(b'\n');
    storage::append_synced(path, &line).map_err(|failure| log_error(failure.to_string()))
}

/// 启动加载：快照是缓存。锚点校验（board_id 一致且 seq 不越过事件流长度）通过则只补折尾部事件，
/// 快照缺失/损坏/锚点不符一律从事件流全量重建——缓存永远不掩盖真源。
pub fn load_state(workspace: &Path, board_id: &str) -> Result<BoardState, KanbanError> {
    let dir = board_dir(workspace, board_id)?;
    let events = load_events(&events_path(&dir))?;
    let snapshot = std::fs::read(snapshot_path(&dir)).ok().and_then(|bytes| serde_json::from_slice::<BoardState>(&bytes).ok());
    match snapshot {
        Some(mut state) if state.board_id == board_id && state.seq as usize <= events.len() => {
            for event in &events[state.seq as usize..] {
                projection::reduce(&mut state, event)?;
            }
            Ok(state)
        }
        _ => projection::replay(board_id, &events),
    }
}

/// 刷新快照缓存：原子写（tmp + rename），失败不影响已提交事件（下次启动从事件流重建）。
pub fn save_snapshot(board_dir: &Path, state: &BoardState) -> Result<(), KanbanError> {
    let bytes = serde_json::to_vec(state).map_err(log_error)?;
    storage::write_atomic(&snapshot_path(board_dir), &bytes).map_err(|failure| log_error(failure.to_string()))
}

#[cfg(test)]
mod tests;
