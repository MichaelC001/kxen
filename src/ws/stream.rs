//! Stream 通道：订阅-推送（topic 过滤，server 主动推）。

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use super::{StreamCtl, StreamPush};
use crate::AppState;

pub(super) async fn handle_stream(
    ws: tokio_tungstenite::WebSocketStream<TcpStream>,
    app: AppHandle,
) {
    let (mut tx, mut rx) = ws.split();
    let mut topics: HashSet<String> = ["llm.delta", "task.update", "goal.update", "notification"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut bus_rx = app.state::<Arc<AppState>>().bus.subscribe();

    loop {
        tokio::select! {
            // client 控制帧
            msg = rx.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Ok(ctl) = serde_json::from_str::<StreamCtl>(&text) {
                            match ctl {
                                StreamCtl::Subscribe { topics: t } => topics.extend(t),
                                StreamCtl::Unsubscribe { topics: t } => {
                                    for topic in t {
                                        topics.remove(&topic);
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    _ => {}
                }
            }
            // 内部事件桥 -> topic 推送
            event = bus_rx.recv() => {
                let Ok(event) = event else { break };
                let (topic, payload) = map_event(event);
                if !topics.contains(topic) {
                    continue;
                }
                let push = StreamPush { topic: topic.to_string(), payload };
                let Ok(text) = serde_json::to_string(&push) else { continue };
                if tx.send(WsMessage::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

fn map_event(event: kxen_app::core::event::Event) -> (&'static str, Value) {
    use kxen_app::core::event::Event;
    match event {
        Event::LlmDelta(payload) => ("llm.delta", payload),
        Event::ToolCall { name, summary } => ("llm.delta", json!({ "tool": name, "summary": summary })),
        Event::TaskUpdate { id, status } => ("task.update", json!({ "id": id, "status": status })),
        Event::GoalUpdate { id, status } => ("goal.update", json!({ "id": id, "status": status })),
        Event::Notification(text) => ("notification", json!({ "text": text })),
    }
}
