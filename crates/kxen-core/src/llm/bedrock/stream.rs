//! converse-stream eventstream 事件 -> 统一 Delta 投影 + 响应字节流管线。
//! 事件契约（AWS 文档「ConverseStream」）：contentBlockStart 起 toolUse 块、contentBlockDelta 带
//! text / toolUse.input 分片 / reasoningContent.text、contentBlockStop 收块、metadata 带 usage、
//! 异常帧走 :message-type=exception（:exception-type 头 + payload.message）。

use crate::llm::eventstream::{Event, FrameDecoder};
use crate::llm::types::Delta;
use futures::StreamExt;
use serde_json::Value;
use std::collections::VecDeque;
use std::pin::Pin;

/// 进行中的 toolUse 块：input 是 JSON 字符串分片，contentBlockStop 时一次性解析给出。
/// toolUseId 不回传：Delta::ToolCall 不携带 id，按 contentBlockIndex 槽位归并即可。
struct ToolBuf {
    name: String,
    fragments: String,
}

#[derive(Default)]
struct Projection {
    /// contentBlockIndex -> tool 缓冲槽位
    tools: std::collections::HashMap<u64, ToolBuf>,
    saw_message_stop: bool,
}

impl Projection {
    /// 单事件 -> 0..n 个 Delta；返回 false 表示协议终态（异常帧，流就此结束）。
    fn process(&mut self, event: &Event, out: &mut VecDeque<Delta>) -> bool {
        if event.message_type == "exception" || event.message_type == "error" {
            let message = event.payload.as_ref().and_then(|p| p.get("message")).and_then(Value::as_str);
            let detail = match (message, event.error_code.is_empty()) {
                (Some(message), false) => format!("{}: {message}", event.error_code),
                (Some(message), true) => message.to_string(),
                (None, false) => event.error_code.clone(),
                (None, true) => "upstream eventstream exception".to_string(),
            };
            out.push_back(Delta::Error(format!("bedrock {detail}")));
            return false;
        }
        let payload = || event.payload.as_ref();
        match event.event_type.as_str() {
            "contentBlockStart" => {
                let index = payload().and_then(|p| p.get("contentBlockIndex")).and_then(Value::as_u64).unwrap_or(0);
                if let Some(tool_use) = payload().and_then(|p| p.get("start")).and_then(|s| s.get("toolUse")) {
                    let name = tool_use.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
                    self.tools.insert(index, ToolBuf { name, fragments: String::new() });
                }
            }
            "contentBlockDelta" => {
                let Some(delta) = payload().and_then(|p| p.get("delta")) else { return true };
                if let Some(text) = delta.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                    out.push_back(Delta::Text(text.to_string()));
                } else if let Some(text) =
                    delta.get("reasoningContent").and_then(|r| r.get("text")).and_then(Value::as_str).filter(|t| !t.is_empty())
                {
                    out.push_back(Delta::Reasoning(text.to_string()));
                } else if let Some(fragment) = delta.get("toolUse").and_then(|t| t.get("input")).and_then(Value::as_str) {
                    let index = payload().and_then(|p| p.get("contentBlockIndex")).and_then(Value::as_u64).unwrap_or(0);
                    if let Some(tool) = self.tools.get_mut(&index) {
                        tool.fragments.push_str(fragment);
                    }
                }
            }
            "contentBlockStop" => {
                let index = payload().and_then(|p| p.get("contentBlockIndex")).and_then(Value::as_u64).unwrap_or(0);
                if let Some(tool) = self.tools.remove(&index) {
                    match serde_json::from_str::<Value>(&tool.fragments) {
                        Ok(input) => out.push_back(Delta::ToolCall { name: tool.name, input }),
                        // 空分片 = 无参工具，给空对象；其余为明确解析失败
                        Err(_) if tool.fragments.trim().is_empty() => {
                            out.push_back(Delta::ToolCall { name: tool.name, input: Value::Object(serde_json::Map::new()) })
                        }
                        Err(error) => out.push_back(Delta::Error(format!("bedrock tool input is not valid JSON ({}): {error}", tool.name))),
                    }
                }
            }
            "messageStop" => self.saw_message_stop = true,
            "metadata" => {
                if let Some(usage) = payload().and_then(|p| p.get("usage")) {
                    let input = usage.get("inputTokens").and_then(Value::as_u64);
                    if let Some(input) = input {
                        let output = usage.get("outputTokens").and_then(Value::as_u64).unwrap_or(0);
                        out.push_back(Delta::Usage { input, output });
                    }
                }
            }
            // messageStart 等仅带角色信息，无 Delta 投影
            _ => {}
        }
        true
    }

    /// 传输 EOF 收尾：未见 messageStop 即协议截断；未闭合的 toolUse 块视为丢失（input 不完整不可执行）。
    fn finish(&mut self, out: &mut VecDeque<Delta>) {
        if !self.tools.is_empty() {
            out.push_back(Delta::Error("bedrock eventstream ended with unterminated toolUse blocks".into()));
            return;
        }
        if self.saw_message_stop {
            out.push_back(Delta::Done);
        } else {
            out.push_back(Delta::Error("bedrock eventstream ended before messageStop".into()));
        }
    }
}

/// 响应字节流 -> Delta 流：帧可跨分片，传输 EOF 后收尾（截断帧/未见 messageStop 都报错）。
pub(super) fn stream_events(resp: reqwest::Response) -> Pin<Box<dyn futures::Stream<Item = Delta> + Send>> {
    let bytes = Box::pin(resp.bytes_stream());
    let initial = (bytes, FrameDecoder::new("bedrock"), Projection::default(), VecDeque::new(), false);
    Box::pin(futures::stream::unfold(initial, |(mut bytes, mut decoder, mut projection, mut queued, mut finished)| async move {
        loop {
            if let Some(delta) = queued.pop_front() {
                return Some((delta, (bytes, decoder, projection, queued, finished)));
            }
            if finished {
                return None;
            }
            match bytes.next().await {
                Some(Ok(chunk)) => match decoder.feed(&chunk) {
                    Ok(events) => {
                        for event in events {
                            if !projection.process(&event, &mut queued) {
                                finished = true;
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        queued.push_back(Delta::Error(error));
                        finished = true;
                    }
                },
                Some(Err(error)) => {
                    queued.push_back(Delta::Error(format!("bedrock eventstream read: {error}")));
                    finished = true;
                }
                None => {
                    match decoder.finish() {
                        Ok(()) => projection.finish(&mut queued),
                        Err(error) => queued.push_back(Delta::Error(error)),
                    }
                    finished = true;
                }
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::eventstream::Event;

    fn event(event_type: &str, payload: &str) -> Event {
        Event {
            message_type: "event".into(),
            event_type: event_type.into(),
            error_code: String::new(),
            payload: Some(serde_json::from_str(payload).unwrap()),
        }
    }

    #[test]
    fn text_reasoning_and_usage_project_in_order() {
        let mut projection = Projection::default();
        let mut out = VecDeque::new();
        assert!(projection.process(&event("contentBlockDelta", r#"{"delta":{"reasoningContent":{"text":"想"}}}"#), &mut out));
        assert!(projection.process(&event("contentBlockDelta", r#"{"delta":{"text":"答"}}"#), &mut out));
        assert!(projection.process(&event("messageStop", r#"{"stopReason":"end_turn"}"#), &mut out));
        assert!(projection.process(&event("metadata", r#"{"usage":{"inputTokens":7,"outputTokens":3}}"#), &mut out));
        let deltas: Vec<Delta> = out.into_iter().collect();
        assert!(matches!(&deltas[0], Delta::Reasoning(t) if t == "想"), "{deltas:?}");
        assert!(matches!(&deltas[1], Delta::Text(t) if t == "答"), "{deltas:?}");
        assert!(matches!(&deltas[2], Delta::Usage { input: 7, output: 3 }), "{deltas:?}");
    }

    #[test]
    fn tool_use_fragments_emit_call_at_block_stop() {
        let mut projection = Projection::default();
        let mut out = VecDeque::new();
        projection.process(
            &event("contentBlockStart", r#"{"contentBlockIndex":1,"start":{"toolUse":{"toolUseId":"t1","name":"exec"}}}"#),
            &mut out,
        );
        projection
            .process(&event("contentBlockDelta", r#"{"contentBlockIndex":1,"delta":{"toolUse":{"input":"{\"command\":"}}}"#), &mut out);
        projection.process(&event("contentBlockDelta", r#"{"contentBlockIndex":1,"delta":{"toolUse":{"input":"\"ls\"}"}}}"#), &mut out);
        projection.process(&event("contentBlockStop", r#"{"contentBlockIndex":1}"#), &mut out);
        let deltas: Vec<Delta> = out.into_iter().collect();
        assert_eq!(deltas.len(), 1, "{deltas:?}");
        assert!(matches!(&deltas[0], Delta::ToolCall { name, input } if name == "exec" && input["command"] == "ls"), "{deltas:?}");
    }

    #[test]
    fn empty_tool_input_becomes_empty_object() {
        let mut projection = Projection::default();
        let mut out = VecDeque::new();
        projection.process(
            &event("contentBlockStart", r#"{"contentBlockIndex":0,"start":{"toolUse":{"toolUseId":"t1","name":"noop"}}}"#),
            &mut out,
        );
        projection.process(&event("contentBlockStop", r#"{"contentBlockIndex":0}"#), &mut out);
        assert!(matches!(out.pop_front(), Some(Delta::ToolCall { name, input }) if name == "noop" && input == serde_json::json!({})));
    }

    #[test]
    fn exception_frame_is_terminal_error_with_code() {
        let mut projection = Projection::default();
        let mut out = VecDeque::new();
        let frame = Event {
            message_type: "exception".into(),
            event_type: String::new(),
            error_code: "throttlingException".into(),
            payload: Some(serde_json::json!({"message": "slow down"})),
        };
        assert!(!projection.process(&frame, &mut out));
        assert!(matches!(out.pop_front(), Some(Delta::Error(e)) if e == "bedrock throttlingException: slow down"));
    }

    #[test]
    fn finish_requires_message_stop_and_closed_blocks() {
        let mut projection = Projection::default();
        let mut out = VecDeque::new();
        projection.finish(&mut out);
        assert!(matches!(out.pop_front(), Some(Delta::Error(e)) if e.contains("before messageStop")));

        let mut projection = Projection::default();
        projection.process(
            &event("contentBlockStart", r#"{"contentBlockIndex":0,"start":{"toolUse":{"toolUseId":"t","name":"exec"}}}"#),
            &mut VecDeque::new(),
        );
        projection.process(&event("messageStop", "{}"), &mut VecDeque::new());
        let mut out = VecDeque::new();
        projection.finish(&mut out);
        assert!(matches!(out.pop_front(), Some(Delta::Error(e)) if e.contains("unterminated toolUse")));
    }
}
