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
    /// 置顶（排在该目录组最前）
    #[serde(default)]
    pub pinned: bool,
    /// 手动排序序号（同组内升序；None = 按 updated_at 倒序）
    #[serde(default)]
    pub sort_order: Option<u64>,
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
        pinned: false,
        sort_order: None,
    };
    save_meta(dir, &session)?;
    Ok(session)
}

/// 就地更新元信息（重命名 / 置顶 / 手动排序）。
pub fn update_meta(dir: &Path, id: &str, title: Option<&str>, pinned: Option<bool>, sort_order: Option<Option<u64>>) -> std::io::Result<Session> {
    let mut session = load_meta(dir, id)?;
    if let Some(t) = title {
        session.title = t.to_string();
    }
    if let Some(p) = pinned {
        session.pinned = p;
    }
    if let Some(so) = sort_order {
        session.sort_order = so;
    }
    session.updated_at = now_ms();
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

/// 删除会话：移入系统废纸篓（Finder 可恢复）。测试走硬删，避免 cargo test 污染用户废纸篓。
#[cfg(not(test))]
pub fn remove(dir: &Path, id: &str) {
    let _ = trash::delete(meta_path(dir, id));
    let _ = trash::delete(messages_path(dir, id));
}

#[cfg(test)]
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

/// 从指定消息分叉：新会话携带 [..=message_id] 前缀历史（parent_id 指向源会话）。
pub fn fork(dir: &Path, id: &str, message_id: &str) -> std::io::Result<Session> {
    let parent = load_meta(dir, id)?;
    let messages = load_messages(dir, id);
    let Some(idx) = messages.iter().position(|m| m.id == message_id) else {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("message not found: {message_id}")));
    };
    let mut session = create(dir, &parent.directory)?;
    session.parent_id = Some(id.to_string());
    session.title = format!("分叉: {}", parent.title.chars().take(24).collect::<String>());
    save_meta(dir, &session)?;
    for m in &messages[..=idx] {
        let mut cloned = m.clone();
        cloned.session_id = session.id.clone();
        append_message(dir, &cloned)?;
    }
    Ok(session)
}

/// 导出 markdown：user/assistant 正文 + 工具调用摘要（reasoning 略）。
pub fn export_markdown(dir: &Path, id: &str) -> std::io::Result<String> {
    let session = load_meta(dir, id)?;
    let messages = load_messages(dir, id);
    let mut out = format!(
        "# {}\n\n- session: {}\n- directory: {}\n\n",
        session.title, session.id, session.directory
    );
    for m in &messages {
        let role = match m.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => continue,
        };
        let mut body = String::new();
        for p in &m.parts {
            match p {
                Part::Text { text } => {
                    body.push_str(text);
                    body.push('\n');
                }
                Part::ToolCall { name, input, output } => {
                    let summary: String = output.chars().take(120).collect();
                    body.push_str(&format!("\n> tool `{name}`: {input} -> {summary}\n"));
                }
                Part::Reasoning { .. } => {}
            }
        }
        if !body.trim().is_empty() {
            out.push_str(&format!("\n## {role}\n\n{body}\n"));
        }
    }
    Ok(out)
}

/// 导出到指定路径（空则 ~/Downloads/kxen-<title>-<ts>.md），返回落盘路径。
pub fn export_to_file(dir: &Path, id: &str, out: Option<&Path>) -> std::io::Result<PathBuf> {
    let md = export_markdown(dir, id)?;
    let path = match out {
        Some(p) => p.to_path_buf(),
        None => {
            let session = load_meta(dir, id)?;
            let slug: String = session
                .title
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .take(40)
                .collect();
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("Downloads")
                .join(format!("kxen-{slug}-{}.md", now_ms()))
        }
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, md)?;
    Ok(path)
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

        let m2 = new_message(&s.id, Role::Assistant, vec![Part::Text { text: "好的".into() }, Part::ToolCall { name: "exec".into(), input: serde_json::json!({"command": "ls"}), output: "a.txt b.txt".into() }]);
        append_message(&dir, &m2).unwrap();

        let messages = load_messages(&dir, &s.id);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::User);

        // fork 到第一条消息：前缀历史只有 user 一条，parent_id 指源
        let forked = fork(&dir, &s.id, &m1.id).unwrap();
        assert_eq!(forked.parent_id.as_deref(), Some(s.id.as_str()));
        let forked_msgs = load_messages(&dir, &forked.id);
        assert_eq!(forked_msgs.len(), 1);
        assert_eq!(forked_msgs[0].role, Role::User);

        // 元信息更新：重命名/置顶/排序
        let s2 = update_meta(&dir, &s.id, Some("改名后"), Some(true), Some(Some(7))).unwrap();
        assert_eq!(s2.title, "改名后");
        assert!(s2.pinned);
        assert_eq!(s2.sort_order, Some(7));

        // 导出 markdown：标题 + user 正文 + tool 摘要
        let md = export_markdown(&dir, &s.id).unwrap();
        assert!(md.contains("帮我改一下 README 的开头"));
        assert!(md.contains("tool `exec`"));
        assert!(md.contains("a.txt b.txt"));
        let out = export_to_file(&dir, &s.id, None).unwrap();
        assert!(out.exists());

        remove(&dir, &s.id);
        remove(&dir, &forked.id);
        assert!(list(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}

