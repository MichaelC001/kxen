//! 删除会话兜底蒸馏：消息流 -> 当前 provider 一次性调用 -> 0..N 条 note 落 personal notes/。
//! 纯函数（build_prompt/parse_output）可单测；流错误经 Result 上抛，是否阻塞由调用方决定。

use super::{Scope, add};
use crate::llm::{LlmClient, Message, ModelRef};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct NewNote {
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default = "default_note_type", rename = "type")]
    pub note_type: String,
    pub description: String,
    pub content: String,
}

fn default_scope() -> String {
    "personal".into()
}
fn default_note_type() -> String {
    "note".into()
}

/// 蒸馏提示词：只要可沉淀的持久知识（纠正/约定/坑/偏好），一次性任务细节直接丢弃。
pub fn build_prompt(transcript: &str) -> String {
    format!(
        "You are distilling a finished coding-agent session before it is deleted. \
Extract 0 to 5 durable learnings worth persisting as plain markdown notes: user corrections, \
project conventions, non-obvious pitfalls, lasting preferences. Skip one-off task details, \
ephemeral state, and anything already obvious from the code itself. \
Reply with ONLY a JSON array (no prose, no code fence): \
[{{\"scope\": \"project\"|\"personal\", \"type\": \"correction\"|\"convention\"|\"pitfall\"|\"preference\"|\"note\", \
\"description\": \"<=60 chars\", \"content\": \"<=500 chars\"}}]. \
scope project = true only about this codebase; personal = useful across projects. \
If nothing is worth keeping, reply [].\n\nSESSION TRANSCRIPT:\n{transcript}"
    )
}

/// 宽容解析：截取首个 `[` 到末个 `]`，坏 JSON 返回空（= 不沉淀）。
pub fn parse_output(text: &str) -> Vec<NewNote> {
    let start = text.find('[');
    let end = text.rfind(']');
    let (Some(s), Some(e)) = (start, end) else { return Vec::new() };
    if e <= s {
        return Vec::new();
    }
    let notes: Vec<NewNote> = serde_json::from_str(&text[s..=e]).unwrap_or_default();
    notes.into_iter().filter(|n| !n.description.trim().is_empty() && !n.content.trim().is_empty()).take(5).collect()
}

/// 删除前兜底蒸馏。返回沉淀条数；LLM 流报错（Delta::Error）以 Err 传播，
/// 由调用方决定静默（删除路径）或留水位重试（consolidation）；单条落盘失败仍跳过不计数。
pub async fn distill_on_delete(
    model: &ModelRef,
    store: &crate::auth::credential::AuthStore,
    workdir: &std::path::Path,
    transcript: Vec<String>,
) -> Result<usize, String> {
    if transcript.is_empty() {
        return Ok(0);
    }
    let joined = transcript.join("\n\n");
    // 蒸馏输入截断：长会话只取尾部 12k 字符（最近的纠正/结论密度最高）
    let tail: String = joined.chars().rev().take(12_000).collect::<Vec<_>>().into_iter().rev().collect();
    let messages = vec![Message::user(build_prompt(&tail))];
    let mut stream = LlmClient::stream(model, &messages, store);
    let mut text = String::new();
    use futures::StreamExt;
    while let Some(delta) = stream.next().await {
        match delta {
            crate::llm::Delta::Text(t) => text.push_str(&t),
            crate::llm::Delta::Error(e) => return Err(e),
            _ => {}
        }
    }
    let notes = parse_output(&text);
    let mut written = 0;
    for note in notes {
        let scope = Scope::parse(&note.scope).unwrap_or(Scope::Personal);
        if add(scope, workdir, None, &note.note_type, &note.description, &note.content).is_ok() {
            written += 1;
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_output_tolerates_prose_and_fence() {
        let text = "Here you go:\n```json\n[{\"scope\":\"project\",\"type\":\"pitfall\",\"description\":\"vite 端口 7823\",\"content\":\"devUrl 必须与 vite.config 一致\"}]\n```";
        let notes = parse_output(text);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].scope, "project");
        assert_eq!(notes[0].note_type, "pitfall");
    }

    #[test]
    fn parse_output_empty_and_broken() {
        assert!(parse_output("[]").is_empty());
        assert!(parse_output("not json at all").is_empty());
        assert!(parse_output("[{\"description\":\"\",\"content\":\"\"}]").is_empty());
    }

    #[test]
    fn prompt_asks_for_json_only() {
        let p = build_prompt("user: x\nassistant: y");
        assert!(p.contains("JSON array"));
        assert!(p.contains("SESSION TRANSCRIPT:"));
    }
}
