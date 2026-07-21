//! 事件总线（tokio broadcast，零拷贝）。

use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum Event {
    LlmDelta(serde_json::Value),
    ToolCall { name: &'static str, summary: String },
    TaskUpdate { id: String, status: &'static str },
    GoalUpdate { id: String, status: &'static str },
    Notification(String),
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn publish(&self, event: Event) {
        // 无订阅者时静默丢弃，不算错误
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}
