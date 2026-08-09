//! 事件桥：bus 事件 -> JSON-RPC 3.0 stream chunk。
//! 全部走订阅流：命中 topic 的订阅流 chunk（result 携带 {topic, payload}）。
//! run 增量只经 llm.delta 下发：chunk 必须带 topic，前端按 topic 匹配消费，无 topic 的帧会被丢弃。
//!
//! 会话 ACL：带 session_id 的 LlmDelta 只发给订阅了 `session:<id>` topic 的连接。
//! 无 session_id 的审批走独立 `approval.global`，由 App 常驻消费，不混入 Session 时间线。

use serde_json::Value;

use super::protocol::StreamChunk;
use super::{StreamSequences, SubBinding};

pub(super) fn event_to_chunks(
    event: kxen_core::core::event::Event,
    subs: &[SubBinding],
    sequences: &mut StreamSequences,
) -> Option<StreamChunk> {
    use kxen_core::core::event::Event;
    match event {
        Event::LlmDelta(payload) if is_global_approval(&payload) => {
            let binding = subs.iter().find(|binding| binding.topics.contains("approval.global"))?;
            let seq = sequences.next(&binding.stream_id);
            Some(StreamChunk::new(&binding.stream_id, seq, serde_json::json!({ "topic": "approval.global", "payload": payload })))
        }
        Event::LlmDelta(payload) => {
            // 连接级判定：本连接的订阅里没有 session:<id> 就一帧都不发
            if let Some(sid) = payload.get("session_id").and_then(Value::as_str)
                && !subs.iter().any(|binding| binding.topics.iter().any(|topic| topic.strip_prefix("session:") == Some(sid)))
            {
                return None;
            }
            // llm.delta 订阅流是唯一消费路径（teammate/其他会话的被动监听也走这里）
            let binding = subs.iter().find(|b| b.topics.contains("llm.delta"))?;
            let seq = sequences.next(&binding.stream_id);
            Some(StreamChunk::new(&binding.stream_id, seq, serde_json::json!({ "topic": "llm.delta", "payload": payload })))
        }
        Event::KanbanUpdate { board_id, workspace } => {
            // 动态 topic（同 session: 臂）：只发订阅了该板的连接。
            // 无会话 ACL：板 metadata 是 workspace 本地信息，与 goal.update 全局同级。
            let topic = format!("kanban:{board_id}");
            let binding = subs.iter().find(|b| b.topics.contains(&topic))?;
            let seq = sequences.next(&binding.stream_id);
            Some(StreamChunk::new(
                &binding.stream_id,
                seq,
                serde_json::json!({ "topic": topic, "payload": { "board_id": board_id, "workspace": workspace } }),
            ))
        }
        other => {
            let (topic, payload) = map_event(other);
            let binding = subs.iter().find(|b| b.topics.contains(topic))?;
            let seq = sequences.next(&binding.stream_id);
            Some(StreamChunk::new(&binding.stream_id, seq, serde_json::json!({ "topic": topic, "payload": payload })))
        }
    }
}

fn is_global_approval(payload: &Value) -> bool {
    payload.get("session_id").is_none() && matches!(payload.get("kind").and_then(Value::as_str), Some("approval" | "approval.resolved"))
}

fn map_event(event: kxen_core::core::event::Event) -> (&'static str, Value) {
    use kxen_core::core::event::Event;
    match event {
        Event::LlmDelta(payload) => ("llm.delta", payload),
        Event::TaskUpdate { id, status } => ("task.update", serde_json::json!({ "id": id, "status": status })),
        Event::GoalUpdate { id, status } => ("goal.update", serde_json::json!({ "id": id, "status": status })),
        Event::Notification { text, session_id } => ("notification", serde_json::json!({ "text": text, "session_id": session_id })),
        // session.update 不带 ACL：侧栏需要全量会话的 run 存亡信号（LlmDelta 的 session ACL 给不了）
        Event::SessionRun { session_id, running } => {
            ("session.update", serde_json::json!({ "session_id": session_id, "running": running }))
        }
        // 动态 topic 放不进 &'static str：event_to_chunks 的专用臂先行拦截，到不了这里
        Event::KanbanUpdate { .. } => unreachable!("KanbanUpdate 由 event_to_chunks 的动态 topic 臂处理"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn binding(topics: &[&str]) -> SubBinding {
        SubBinding { stream_id: format!("sub-test-{}", topics.len()), topics: topics.iter().map(|t| t.to_string()).collect::<HashSet<_>>() }
    }

    fn delta(session_id: Option<&str>) -> kxen_core::core::event::Event {
        let mut payload = serde_json::json!({ "kind": "delta", "stream_id": "run-t1", "text": "x" });
        if let Some(sid) = session_id {
            payload.as_object_mut().unwrap().insert("session_id".into(), serde_json::json!(sid));
        }
        kxen_core::core::event::Event::LlmDelta(payload)
    }

    /// 未订阅 session:<id> 的连接一帧都收不到
    #[test]
    fn unsubscribed_connection_gets_nothing() {
        let subs = vec![binding(&["llm.delta"])];
        assert!(event_to_chunks(delta(Some("s1")), &subs, &mut StreamSequences::default()).is_none());
    }

    /// 订阅了 session:<id> 的连接正常收到（llm.delta 单写）
    #[test]
    fn subscribed_connection_receives() {
        let subs = vec![binding(&["llm.delta", "session:s1"])];
        assert!(event_to_chunks(delta(Some("s1")), &subs, &mut StreamSequences::default()).is_some());
    }

    /// 连接级判定：同一事件，两个连接各自按自己的订阅判定；
    /// 无 session_id 的全局事件不受 ACL 影响
    #[test]
    fn acl_is_per_connection_and_global_events_unaffected() {
        let with = vec![binding(&["llm.delta", "session:s1"])];
        let without = vec![binding(&["llm.delta"])];
        let event = delta(Some("s1"));
        assert!(event_to_chunks(event.clone(), &with, &mut StreamSequences::default()).is_some());
        assert!(event_to_chunks(event, &without, &mut StreamSequences::default()).is_none());
        // 普通无 session_id 的全局 delta 仍照常吃 llm.delta
        assert!(event_to_chunks(delta(None), &without, &mut StreamSequences::default()).is_some());
    }

    #[test]
    fn global_approval_uses_dedicated_topic_without_session_duplication() {
        let event = kxen_core::core::event::Event::LlmDelta(serde_json::json!({
            "kind": "approval", "approval_id": "appr-1", "command": "cmd", "reason": "r",
        }));
        let both = vec![binding(&["approval.global"]), binding(&["llm.delta", "session:s1"])];
        let chunk = event_to_chunks(event, &both, &mut StreamSequences::default()).unwrap();
        assert_eq!(chunk.result["topic"], "approval.global");

        let session_only = vec![binding(&["llm.delta", "session:s1"])];
        let resolved = kxen_core::core::event::Event::LlmDelta(serde_json::json!({
            "kind": "approval.resolved", "approval_id": "appr-1", "outcome": "timeout",
        }));
        assert!(event_to_chunks(resolved, &session_only, &mut StreamSequences::default()).is_none());
    }

    /// 订了别的会话不等于订了本会话（越权不泄露）
    #[test]
    fn other_session_does_not_leak() {
        let subs = vec![binding(&["llm.delta", "session:s2"])];
        assert!(event_to_chunks(delta(Some("s1")), &subs, &mut StreamSequences::default()).is_none());
    }

    /// done 帧照常走 llm.delta 下发（终态判定看 payload.kind，不靠 complete 标记）
    #[test]
    fn done_frame_flows_through_llm_delta() {
        let subs = vec![binding(&["llm.delta", "session:s1"])];
        let event = kxen_core::core::event::Event::LlmDelta(serde_json::json!({
            "kind": "done", "stream_id": "run-t2", "session_id": "s1",
        }));
        let chunk = event_to_chunks(event, &subs, &mut StreamSequences::default()).unwrap();
        assert_eq!(chunk.result["topic"], "llm.delta");
        assert_eq!(chunk.result["payload"]["kind"], "done");
    }

    /// voice 帧带 session_id 后走同一条 session ACL：未订阅该 session 的连接收不到
    #[test]
    fn voice_frames_follow_session_acl() {
        let voice = || {
            kxen_core::core::event::Event::LlmDelta(serde_json::json!({
                "kind": "voice.partial", "text": "你好", "session_id": "s1",
            }))
        };
        let unsubscribed = vec![binding(&["llm.delta"])];
        assert!(event_to_chunks(voice(), &unsubscribed, &mut StreamSequences::default()).is_none());
        let subscribed = vec![binding(&["llm.delta", "session:s1"])];
        assert!(event_to_chunks(voice(), &subscribed, &mut StreamSequences::default()).is_some());
    }

    /// SessionRun 走 session.update topic 且无会话 ACL：只订 topic 的连接就收到（侧栏不逐会话订阅）
    #[test]
    fn session_run_broadcasts_without_acl() {
        let subs = vec![binding(&["session.update"])];
        let chunk =
            event_to_chunks(kxen_core::core::event::Event::session_run("s1", true), &subs, &mut StreamSequences::default()).unwrap();
        let payload = &chunk.result["payload"];
        assert_eq!(payload["session_id"], "s1");
        assert_eq!(payload["running"], true);
    }

    /// KanbanUpdate 走动态 kanban:<board_id> topic：订阅该板的连接收到，未订阅/订别板的收不到
    #[test]
    fn kanban_update_follows_dynamic_board_topic() {
        let update = || kxen_core::core::event::Event::KanbanUpdate { board_id: "board_1".into(), workspace: "/ws".into() };
        let subscribed = vec![binding(&["kanban:board_1"])];
        let chunk = event_to_chunks(update(), &subscribed, &mut StreamSequences::default()).unwrap();
        assert_eq!(chunk.result["topic"], "kanban:board_1");
        assert_eq!(chunk.result["payload"]["board_id"], "board_1");
        assert_eq!(chunk.result["payload"]["workspace"], "/ws");

        let unsubscribed = vec![binding(&["kanban:board_2"])];
        assert!(event_to_chunks(update(), &unsubscribed, &mut StreamSequences::default()).is_none());
    }
}
