//! 会话（持久化：meta JSON + messages JSONL，branch/fork/resume 的数据模型）。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub directory: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text { text: String },
    ToolCall { name: String, input: serde_json::Value, output: String },
    Reasoning { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: Role,
    pub parts: Vec<Part>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
}

// ---------------- 持久化（<sessions_dir>/<id>.json meta + <id>.jsonl 消息行） ----------------

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}_{:04x}", now_ms(), std::process::id() & 0xffff)
}

fn meta_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

fn messages_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.jsonl"))
}

pub fn create(dir: &Path, directory: &str) -> std::io::Result<Session> {
    std::fs::create_dir_all(dir)?;
    let now = now_ms();
    let session = Session {
        id: new_id("ses"),
        title: "新会话".into(),
        directory: directory.into(),
        parent_id: None,
        created_at: now,
        updated_at: now,
    };
    save_meta(dir, &session)?;
    Ok(session)
}

pub fn save_meta(dir: &Path, session: &Session) -> std::io::Result<()> {
    let tmp = meta_path(dir, &session.id).with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(session)?)?;
    std::fs::rename(&tmp, meta_path(dir, &session.id))
}

pub fn load_meta(dir: &Path, id: &str) -> std::io::Result<Session> {
    let text = std::fs::read_to_string(meta_path(dir, id))?;
    serde_json::from_str(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// 全部会话元信息（updated_at 倒序）。
pub fn list(dir: &Path) -> Vec<Session> {
    let mut out: Vec<Session> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok())
        .collect();
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

pub fn remove(dir: &Path, id: &str) {
    let _ = std::fs::remove_file(meta_path(dir, id));
    let _ = std::fs::remove_file(messages_path(dir, id));
}

/// 追加一条消息（JSONL 行）并维护 meta（updated_at + 首条用户消息生成标题）。
pub fn append_message(dir: &Path, message: &Message) -> std::io::Result<Session> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(messages_path(dir, &message.session_id))?;
    writeln!(file, "{}", serde_json::to_string(message)?)?;

    let mut session = load_meta(dir, &message.session_id)?;
    session.updated_at = now_ms();
    if message.role == Role::User && session.title == "新会话" {
        if let Some(Part::Text { text }) = message.parts.first() {
            session.title = text.chars().take(30).collect();
        }
    }
    save_meta(dir, &session)?;
    Ok(session)
}

pub fn load_messages(dir: &Path, id: &str) -> Vec<Message> {
    let Ok(text) = std::fs::read_to_string(messages_path(dir, id)) else {
        return Vec::new();
    };
    text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
}

pub fn new_message(session_id: &str, role: Role, parts: Vec<Part>) -> Message {
    Message { id: new_id("msg"), session_id: session_id.into(), role, parts, created_at: now_ms() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_lifecycle() {
        let dir = std::env::temp_dir().join(format!("kxen-ses-{}", std::process::id()));
        let s = create(&dir, "/tmp/work").unwrap();
        assert_eq!(list(&dir).len(), 1);

        let m1 = new_message(&s.id, Role::User, vec![Part::Text { text: "帮我改一下 README 的开头".into() }]);
        let meta = append_message(&dir, &m1).unwrap();
        assert_eq!(meta.title, "帮我改一下 README 的开头");

        let m2 = new_message(&s.id, Role::Assistant, vec![Part::Text { text: "好的".into() }]);
        append_message(&dir, &m2).unwrap();

        let messages = load_messages(&dir, &s.id);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::User);

        remove(&dir, &s.id);
        assert!(list(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}

