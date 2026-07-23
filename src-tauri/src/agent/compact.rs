//! 上下文压缩（compaction）：阈值触发把旧历史蒸馏成一条摘要消息，窗口腾出后重注入。
//! 窗口取 catalog 的模型 limit.context（200k 硬编码的唯一替代源），失败兜底 200k。

use crate::llm::{Delta, LlmClient, Message, ModelRef};

/// 粗估 tokens（chars/4，与 composer 的预估同口径）。
pub fn estimate_tokens(messages: &[Message]) -> u64 {
    messages.iter().map(|m| (m.content.len() / 4) as u64).sum()
}

/// 模型上下文窗：catalog 查不到回落 200k。
pub fn context_window(model: &ModelRef) -> u64 {
    crate::llm::catalog::catalog()
        .iter()
        .find(|p| p.provider == model.provider)
        .and_then(|p| p.models.iter().find(|m| m.id == model.model))
        .map(|m| m.context)
        .filter(|c| *c > 0)
        .unwrap_or(200_000)
}

/// 触发线：窗口 80%。
pub fn needs_compact(messages: &[Message], model: &ModelRef) -> bool {
    estimate_tokens(messages) > context_window(model) * 80 / 100
}

const COMPACT_PROMPT: &str = "\
You are compacting a coding-agent conversation to free context space. \
Summarize the following conversation segment into a durable working memory: \
goal/progress so far, key decisions and their reasons, files touched and why, \
open TODOs, pitfalls encountered. Be terse and factual, no filler. \
Output plain markdown, <= 800 words.\n\nCONVERSATION:\n";

/// 压缩消息序列：保留 system（若有）与最近 keep_recent 条，旧段蒸馏为一条 assistant 摘要。
/// LLM 失败时降级为截断式保留（旧段只留首尾各 2 条），绝不丢最近上下文。
pub async fn compact_messages(
    model: &ModelRef,
    store: &crate::auth::credential::AuthStore,
    messages: &[Message],
    keep_recent: usize,
) -> Vec<Message> {
    let (system, rest) = match messages.first() {
        Some(m) if m.role == crate::llm::types::Role::System => (vec![m.clone()], &messages[1..]),
        _ => (vec![], messages),
    };
    if rest.len() <= keep_recent + 2 {
        return messages.to_vec();
    }
    let (old, recent) = rest.split_at(rest.len() - keep_recent);
    let segment: String = old.iter().map(|m| format!("{:?}: {}", m.role, m.content)).collect::<Vec<_>>().join("\n\n");
    let summary = summarize(model, store, &segment).await.unwrap_or_else(|| {
        // 降级：LLM 不可用时只留关键行（首条 user 意图 + 末条状态），不假装蒸留出内容
        let mut out = String::from("(compaction fallback: LLM unavailable, kept head/tail only)\n");
        for m in old.iter().take(1).chain(old.iter().rev().take(1)) {
            out.push_str(&format!("{:?}: {}\n", m.role, m.content.chars().take(500).collect::<String>()));
        }
        out
    });
    let mut out = system;
    out.push(Message::assistant(format!(
        "[Earlier conversation compacted]\n{summary}"
    )));
    out.extend(recent.iter().cloned());
    out
}

async fn summarize(model: &ModelRef, store: &crate::auth::credential::AuthStore, segment: &str) -> Option<String> {
    let tail: String = segment.chars().rev().take(48_000).collect::<Vec<_>>().into_iter().rev().collect();
    let req = vec![Message::user(format!("{COMPACT_PROMPT}{tail}"))];
    let mut stream = LlmClient::stream(model, &req, store);
    use futures::StreamExt;
    let mut text = String::new();
    while let Some(delta) = stream.next().await {
        match delta {
            Delta::Text(t) => text.push_str(&t),
            Delta::Error(_) => return None,
            _ => {}
        }
    }
    if text.trim().is_empty() { None } else { Some(text) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_counts_chars() {
        let msgs = vec![Message::user("a".repeat(400)), Message::assistant("b".repeat(400))];
        assert_eq!(estimate_tokens(&msgs), 200);
    }

    #[test]
    fn needs_compact_uses_window() {
        let model = ModelRef::new("xai", "grok-build-0.1");
        let big = vec![Message::user("x".repeat(900_000))]; // ~225k tokens > 256k*0.8=204.8k
        assert!(needs_compact(&big, &model));
        let small = vec![Message::user("hello".to_string())];
        assert!(!needs_compact(&small, &model));
    }

    #[test]
    fn compact_preserves_system_and_recent() {
        let model = ModelRef::new("xai", "grok-build-0.1");
        let mut msgs = vec![Message::system("sys")];
        for i in 0..10 {
            msgs.push(Message::user(format!("u{i}")));
            msgs.push(Message::assistant(format!("a{i}")));
        }
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let store = crate::auth::credential::AuthStore::default();
        let out = rt.block_on(compact_messages(&model, &store, &msgs, 4));
        assert_eq!(out[0].content, "sys");
        // 末 4 条原样保留
        assert_eq!(out.last().unwrap().content, "a9");
        assert!(out.len() < msgs.len());
    }
}
