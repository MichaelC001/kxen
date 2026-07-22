//! 事件桥：bus 事件 -> JSON-RPC 3.0 stream chunk。
//! - 带 stream_id 的 LlmDelta -> run 流 chunk（done/aborted/error 末块 complete:true 携带 stats），
//!   同时双写到 llm.delta 订阅流（被动监听方语义统一）
//! - 其余 topic -> 命中的订阅流 chunk（result 携带 {topic, payload}）

use serde_json::Value;

use super::protocol::StreamChunk;
use super::{next_seq, SubBinding};

pub(super) fn event_to_chunks(event: kxen_app::core::event::Event, subs: &[SubBinding]) -> Vec<StreamChunk> {
    use kxen_app::core::event::Event;
    match event {
        Event::LlmDelta(payload) => {
            let mut out = Vec::new();
            if let Some(stream_id) = payload.get("stream_id").and_then(Value::as_str).map(String::from) {
                let kind = payload.get("kind").and_then(Value::as_str).unwrap_or("");
                let seq = next_seq(&stream_id);
                let chunk = if kind == "done" || kind == "aborted" || kind == "error" {
                    StreamChunk::complete(&stream_id, seq, payload.clone())
                } else {
                    StreamChunk::new(&stream_id, seq, payload.clone())
                };
                out.push(chunk);
            }
            // 双写 llm.delta 订阅流（teammate/其他会话的被动监听也走这里）
            if let Some(binding) = subs.iter().find(|b| b.topics.contains("llm.delta")) {
                let seq = next_seq(&binding.stream_id);
                out.push(StreamChunk::new(
                    &binding.stream_id,
                    seq,
                    serde_json::json!({ "topic": "llm.delta", "payload": payload }),
                ));
            }
            out
        }
        other => {
            let (topic, payload) = map_event(other);
            let Some(binding) = subs.iter().find(|b| b.topics.contains(topic)) else {
                return Vec::new();
            };
            let seq = next_seq(&binding.stream_id);
            vec![StreamChunk::new(&binding.stream_id, seq, serde_json::json!({ "topic": topic, "payload": payload }))]
        }
    }
}

fn map_event(event: kxen_app::core::event::Event) -> (&'static str, Value) {
    use kxen_app::core::event::Event;
    match event {
        Event::LlmDelta(payload) => ("llm.delta", payload),
        Event::ToolCall { name, summary } => ("llm.delta", serde_json::json!({ "tool": name, "summary": summary })),
        Event::TaskUpdate { id, status } => ("task.update", serde_json::json!({ "id": id, "status": status })),
        Event::GoalUpdate { id, status } => ("goal.update", serde_json::json!({ "id": id, "status": status })),
        Event::Notification(text) => ("notification", serde_json::json!({ "text": text })),
    }
}
