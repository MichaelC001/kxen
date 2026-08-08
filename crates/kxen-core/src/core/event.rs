//! 事件总线（tokio broadcast，零拷贝）。

use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum Event {
    // DCP 语义声明：llm.delta 是瞬态渲染流，只广播不落盘。主会话实质内容已按 loop 迭代
    // 持久化（persist_turn），断连期间的增量不需要也无法从 EventBus 恢复；kanban 列执行的
    // 增量将走自己的 event log 持久化，不依赖本总线。
    LlmDelta(serde_json::Value),
    TaskUpdate { id: String, status: &'static str },
    GoalUpdate { id: String, status: &'static str },
    // session_id 记录来源会话：通知中心条目点击可跳转回来源，系统级通知为 None（不可点）
    Notification { text: String, session_id: Option<String> },
    // run 开始/结束（session.update topic；侧栏 running 圆点事件源）。
    // 不走 LlmDelta：那一路带 session_id ACL 只发订阅方，侧栏需要全量会话的存亡信号。
    SessionRun { session_id: String, running: bool },
    // 看板粗粒度变更信号（kanban:<board_id> topic）：板变了，订阅方重拉 kanban.snapshot。
    // 不带全量状态：snapshot 才是重连恢复口径，事件只承担「失效通知」。
    KanbanUpdate { board_id: String, workspace: String },
}

impl Event {
    /// 通知发布统一入口：裸构造容易漏填 session_id，跳转能力就此丢失
    pub fn notify(text: impl Into<String>, session_id: Option<String>) -> Self {
        Self::Notification { text: text.into(), session_id }
    }

    /// run 存亡广播统一入口（唯一构造点：rewind_lock::run_guard）
    pub fn session_run(session_id: impl Into<String>, running: bool) -> Self {
        Self::SessionRun { session_id: session_id.into(), running }
    }
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
