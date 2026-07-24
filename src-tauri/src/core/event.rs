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
    /// Sender 不暴露容量查询，自建时留底（doctor 健康快照用）
    capacity: usize,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx, capacity }
    }

    pub fn publish(&self, event: Event) {
        // 无订阅者时静默丢弃，不算错误
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// 健康快照（doctor）：(容量, 活跃订阅数)。0 订阅 = 事件全在丢，属异常态
    pub fn stats(&self) -> (usize, usize) {
        (self.capacity, self.tx.receiver_count())
    }
}

/// recv 三态：Lagged 溢出跳过继续收（静默退出 = 通知落盘循环永久停更），Closed 才停。
pub enum RecvVerdict {
    Event(Event),
    Skip,
    Stop,
}

pub fn recv_verdict(result: Result<Event, broadcast::error::RecvError>) -> RecvVerdict {
    match result {
        Ok(event) => RecvVerdict::Event(event),
        Err(broadcast::error::RecvError::Lagged(_)) => RecvVerdict::Skip,
        Err(broadcast::error::RecvError::Closed) => RecvVerdict::Stop,
    }
}
