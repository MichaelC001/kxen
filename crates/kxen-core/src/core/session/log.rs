//! 附属 JSONL 日志（无 meta 的 Message 序列）：team member 历史与 subagent per-run 历史共用。
//! 与会话 messages.jsonl 同一份追加/读取口径（torn 阻断、幂等去重、fsync 落盘），不发明第二套格式。

use std::path::Path;

use super::Message;
use super::storage::{self, CommitFailure};

/// 严格读取：torn/坏行阻断（对齐 load_messages_checked），不能在残缺历史上继续追加或重建。
pub fn load_lines(path: &Path) -> std::io::Result<Vec<Message>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    if !text.is_empty() && !text.ends_with('\n') {
        let line = text.lines().count();
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unterminated JSONL record in {} line {line}", path.display()),
        ));
    }
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|error| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, format!("parse {} line {}: {error}", path.display(), index + 1))
            })
        })
        .collect()
}

/// 幂等追加：同 id 内容等价跳过，同 id 内容冲突拒绝（与 append_message_idempotent 同门禁）。
/// 等价判定排除 created_at：附属日志的消息在崩溃重放时现场重建，时间戳必然漂移；
/// 消息身份是确定性 id，内容等价按 session/role/parts/model 判定。
pub fn append_line_idempotent(path: &Path, message: &Message) -> Result<(), CommitFailure> {
    let existing = load_lines(path).map_err(CommitFailure::before)?;
    if let Some(found) = existing.iter().find(|item| item.id == message.id) {
        let content_of = |m: &Message| serde_json::to_value((&m.session_id, &m.role, &m.parts, &m.model)).unwrap_or_default();
        if content_of(found) != content_of(message) {
            return Err(CommitFailure::before(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("message id collision: {}", message.id),
            )));
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(CommitFailure::before)?;
    }
    let mut line = serde_json::to_vec(message).map_err(|error| CommitFailure::before(std::io::Error::other(error)))?;
    line.push(b'\n');
    storage::append_synced(path, &line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::session::{Part, Role, new_message};

    fn temp(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("kxen-session-log-{tag}-{}-{nanos}", std::process::id()))
    }

    fn text_message(id: &str, text: &str) -> Message {
        let mut message = new_message("ses", Role::User, vec![Part::Text { text: text.into() }]);
        message.id = id.into();
        message
    }

    #[test]
    fn missing_file_loads_empty_and_append_creates_parent_dirs() {
        let dir = temp("create");
        let path = dir.join("nested/history/w.jsonl");
        assert!(load_lines(&path).unwrap().is_empty());
        append_line_idempotent(&path, &text_message("w:w1:u", "brief")).unwrap();
        let stored = load_lines(&path).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, "w:w1:u");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn idempotent_rewrite_with_drifted_timestamp_does_not_duplicate() {
        // 崩溃重放重建的消息 created_at 漂移：同 id 同内容必须跳过而非报碰撞
        let dir = temp("idem");
        let path = dir.join("w.jsonl");
        let mut first = text_message("w:in:msg_1", "hello");
        first.created_at = 1;
        append_line_idempotent(&path, &first).unwrap();
        let mut replay = text_message("w:in:msg_1", "hello");
        replay.created_at = 999;
        append_line_idempotent(&path, &replay).unwrap();
        assert_eq!(load_lines(&path).unwrap().len(), 1, "重放不得写双份");
        let mut different = text_message("w:in:msg_1", "changed");
        different.created_at = 1;
        let error = append_line_idempotent(&path, &different).unwrap_err();
        assert!(error.to_string().contains("collision"), "{error}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn torn_line_blocks_load_and_append() {
        let dir = temp("torn");
        let path = dir.join("w.jsonl");
        append_line_idempotent(&path, &text_message("w:w1:u", "kept")).unwrap();
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"id\":").unwrap();
        file.sync_all().unwrap();
        drop(file);
        let before = std::fs::read(&path).unwrap();

        assert_eq!(load_lines(&path).unwrap_err().kind(), std::io::ErrorKind::InvalidData);
        assert!(append_line_idempotent(&path, &text_message("w:w2:u", "must not append")).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before, "失败路径不得继续追加 torn JSONL");
        std::fs::remove_dir_all(dir).ok();
    }
}
