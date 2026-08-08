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

/// 事件流锁等待上限：带超时而非立即失败，是因为 GUI+web 双进程同开一 workspace 是支持场景，
/// 瞬时冲突应等待而非报错；超时仍持锁说明对方卡住，不能无限等。测试缩小避免拖慢套件。
#[cfg(not(test))]
const LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(test)]
const LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(300);

/// 事件流跨进程互斥锁：进程内 board_lock 管不住另一进程，双进程同时 apply 会写出重复 seq
/// 砖化事件流（load_events 连续性检查 fail-closed）。返回的 File 即锁本体（RAII，drop 释放）。
pub fn lock_events(dir: &Path) -> Result<std::fs::File, KanbanError> {
    std::fs::create_dir_all(dir).map_err(log_error)?;
    let file =
        std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(dir.join("events.lock")).map_err(log_error)?;
    let deadline = std::time::Instant::now() + LOCK_TIMEOUT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(std::fs::TryLockError::WouldBlock) => return Err(KanbanError::Log("board event log is locked by another process".into())),
            Err(std::fs::TryLockError::Error(error)) => return Err(log_error(error)),
        }
    }
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

/// apply 的锁内漂移预检：只读事件流尾部取最后一条的 seq，全量 load_events 是 O(历史)，
/// 每次 apply 不可接受。窗口可能从行中切开，含不了完整尾行时成倍扩窗直到覆盖；
/// 完整尾行解析失败即 Err（torn 行不猜）。文件不存在/空 -> Ok(None)。
pub fn last_event_seq(path: &Path) -> Result<Option<u64>, KanbanError> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(log_error(error)),
    };
    let len = file.metadata().map_err(log_error)?.len();
    if len == 0 {
        return Ok(None);
    }
    let mut window: u64 = 8 * 1024;
    loop {
        let take = window.min(len);
        file.seek(SeekFrom::End(-(take as i64))).map_err(log_error)?;
        let mut buf = vec![0u8; take as usize];
        file.read_exact(&mut buf).map_err(log_error)?;
        let mut end = buf.len();
        while end > 0 && matches!(buf[end - 1], b'\n' | b'\r') {
            end -= 1;
        }
        if end == 0 {
            return Ok(None);
        }
        let start = buf[..end].iter().rposition(|b| *b == b'\n').map(|pos| pos + 1).unwrap_or(0);
        if start == 0 && take < len {
            window = window.saturating_mul(4);
            continue;
        }
        let line = std::str::from_utf8(&buf[start..end]).map_err(|error| log_error(format!("tail of {}: {error}", path.display())))?;
        let event: KanbanEvent =
            serde_json::from_str(line).map_err(|error| log_error(format!("parse last event in {}: {error}", path.display())))?;
        return Ok(Some(event.seq));
    }
}

/// 启动加载：快照是缓存。锚点校验（board_id 一致且 seq 不越过事件流长度）通过则只补折尾部事件，
/// 快照缺失/损坏/锚点不符一律从事件流全量重建——缓存永远不掩盖真源。
pub fn load_state(workspace: &Path, board_id: &str) -> Result<BoardState, KanbanError> {
    let dir = board_dir(workspace, board_id)?;
    load_state_from_dir(&dir, board_id)
}

/// 按目录加载（apply 锁内补折用）：与 load_state 同一实现，不绕开锚点校验。
pub fn load_state_from_dir(dir: &Path, board_id: &str) -> Result<BoardState, KanbanError> {
    let events = load_events(&events_path(dir))?;
    let snapshot = std::fs::read(snapshot_path(dir)).ok().and_then(|bytes| serde_json::from_slice::<BoardState>(&bytes).ok());
    match snapshot {
        // 内容锚：seq>0 时快照折到的尾事件必须就是事件流同位置那条，否则事件流被外部重写
        // （同长度不同内容），锚点通过但状态错误。旧格式快照无 anchor 字段，首载全量 replay
        // 一次后新快照带锚，只付一次成本——缓存永远不掩盖真源
        Some(mut state)
            if state.board_id == board_id
                && state.seq as usize <= events.len()
                && (state.seq == 0 || state.anchor_event_id.as_deref() == Some(events[state.seq as usize - 1].id.as_str())) =>
        {
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
