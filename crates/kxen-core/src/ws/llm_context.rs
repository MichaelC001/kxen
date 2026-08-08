use std::sync::Arc;

use kxen_core::agent::agent_loop::AgentEvent;
use kxen_core::core::event::{Event, EventBus};
use kxen_core::core::session as ses;
use kxen_core::llm::ModelRef;
use serde_json::json;

pub(super) fn event_handler(
    transcript: Arc<std::sync::Mutex<Vec<ses::Part>>>,
    session_id: String,
    stream_id: String,
    model: ModelRef,
    bus: EventBus,
) -> Arc<dyn Fn(AgentEvent) + Send + Sync> {
    Arc::new(move |event| {
        if matches!(&event, AgentEvent::Done { .. } | AgentEvent::Aborted | AgentEvent::Error { .. }) {
            return;
        }
        // transcript 只归约 Reasoning（finalize 拼最终消息的唯一来源）。tool 交互由 run loop
        // 逐迭代持久化（persist_turn），这里再归约会与迭代消息双写。
        if let AgentEvent::Reasoning { text } = &event {
            let mut parts = kxen_core::core::shared::lock(&transcript);
            match parts.last_mut() {
                Some(ses::Part::Reasoning { text: existing }) => existing.push_str(text),
                _ => parts.push(ses::Part::Reasoning { text: text.clone() }),
            }
        }
        let Ok(mut payload) = serde_json::to_value(&event) else { return };
        if let Some(object) = payload.as_object_mut() {
            object.insert("session_id".into(), json!(session_id));
            object.insert("stream_id".into(), json!(stream_id));
            object.insert("model".into(), json!({ "provider": model.provider, "model": model.model }));
        }
        bus.publish(Event::LlmDelta(payload));
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_keeps_only_reasoning_and_publishes_only_non_terminal_deltas() {
        let transcript = Arc::new(std::sync::Mutex::new(Vec::new()));
        let bus = EventBus::new(16);
        let mut events = bus.subscribe();
        let handler = event_handler(transcript.clone(), "ses_one".into(), "stream_one".into(), ModelRef::new("xai", "grok"), bus);

        handler(AgentEvent::Reasoning { text: "first ".into() });
        handler(AgentEvent::Reasoning { text: "second".into() });
        handler(AgentEvent::ToolCall { name: "read".into(), summary: "read file".into(), arguments: r#"{"path":"README.md"}"#.into() });
        handler(AgentEvent::ToolResult { name: "read".into(), summary: "done".into(), output: "contents".into() });
        handler(AgentEvent::Text { text: "answer".into() });

        for terminal in [AgentEvent::Done { turns: 1, stats: None }, AgentEvent::Aborted, AgentEvent::Error { message: "failed".into() }] {
            handler(terminal);
        }

        // tool 交互已逐迭代落盘（persist_turn），transcript 只剩 Reasoning，finalize 不会双写 ToolCall
        let parts = kxen_core::core::shared::lock(&transcript);
        assert_eq!(parts.len(), 1);
        assert!(matches!(&parts[0], ses::Part::Reasoning { text } if text == "first second"));
        drop(parts);

        let mut published = Vec::new();
        while let Ok(Event::LlmDelta(payload)) = events.try_recv() {
            published.push(payload);
        }
        assert_eq!(published.len(), 5, "terminal events must not be published as deltas; tool deltas still broadcast");
        assert!(published.iter().all(|payload| {
            payload["session_id"] == "ses_one"
                && payload["stream_id"] == "stream_one"
                && payload["model"] == json!({ "provider": "xai", "model": "grok" })
        }));
    }
}
