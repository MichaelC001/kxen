//! 审批 broker：工具执行挂起等用户决定（允许/拒绝/超时），RPC 应答唤醒。
//! 中断（abort）一律视为拒绝——审批等待绝不卡住取消路径。

use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::oneshot;

/// 审批三态：超时可与主动拒绝区分（文案/遥测），放行语义只认 Allow。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Allow,
    Deny,
    Timeout,
}

/// 默认审批窗口 5 分钟：无限挂起会让 run 永不收尾（session 删除等不到落地、审批卡烂在前端）。
const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

struct PendingEntry {
    tx: oneshot::Sender<bool>,
    session_id: String,
}

pub struct ApprovalBroker {
    pending: Mutex<HashMap<String, PendingEntry>>,
    timeout: std::time::Duration,
    /// 了结事件出口：超时/清场/中断时向 bus 发 approval.resolved，前端审批卡据此置失效
    bus: Option<crate::core::event::EventBus>,
}

// 手动 Default：derive 会把 Duration 置零（0 秒超时 = 所有审批立即超时拒绝）。
impl Default for ApprovalBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalBroker {
    pub fn new() -> Self {
        Self::with_timeout(DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(timeout: std::time::Duration) -> Self {
        Self { pending: Mutex::new(HashMap::new()), timeout, bus: None }
    }

    pub fn with_bus(mut self, bus: crate::core::event::EventBus) -> Self {
        self.bus = Some(bus);
        self
    }

    /// 发 approval.resolved：空归属（workspace 信任门）不带 session_id——
    /// stream ACL 把带 session_id 的帧只发给 session:<id> 订阅者，空串会被当成无人订阅的会话帧丢弃。
    fn publish_resolved(&self, id: &str, session_id: &str, outcome: &str) {
        let Some(bus) = &self.bus else { return };
        let mut payload = serde_json::json!({ "kind": "approval.resolved", "approval_id": id, "outcome": outcome });
        if !session_id.is_empty() {
            payload.as_object_mut().expect("resolved payload").insert("session_id".into(), serde_json::json!(session_id));
        }
        bus.publish(crate::core::event::Event::LlmDelta(payload));
    }

    /// 登记一条审批：返回 (id, 等待句柄)。session_id 记归属，cancel_session 按会话清场。
    pub fn register(&self, session_id: &str) -> (String, oneshot::Receiver<bool>) {
        let id = crate::core::ids::new_id("appr");
        let (tx, rx) = oneshot::channel();
        self.pending.lock().expect("approvals").insert(id.clone(), PendingEntry { tx, session_id: session_id.to_string() });
        (id, rx)
    }

    /// 用户应答（RPC 通道）：id 存在则送达并返回 true。
    pub fn respond(&self, id: &str, allow: bool) -> bool {
        self.pending.lock().expect("approvals").remove(id).map(|e| e.tx.send(allow).is_ok()).unwrap_or(false)
    }

    /// 会话清场：摘走该 session 全部 pending（tx 随 entry drop，等待方收关闭信号按 deny），
    /// 并向 bus 发 approval.resolved(cancelled)——前端等待中的审批卡据此置失效，不再永远等应答。
    pub fn cancel_session(&self, session_id: &str) -> usize {
        let ids: Vec<String> = {
            let mut map = self.pending.lock().expect("approvals");
            let ids: Vec<String> = map.iter().filter(|(_, e)| e.session_id == session_id).map(|(id, _)| id.clone()).collect();
            for id in &ids {
                map.remove(id);
            }
            ids
        };
        for id in &ids {
            self.publish_resolved(id, session_id, "cancelled");
        }
        ids.len()
    }

    /// 等待决定：abort 优先（视为拒绝）；超时自动 Timeout。
    /// 返回前兜底摘除：respond/cancel_session 已摘则无操作，其余路径（超时）防泄漏。
    /// 超时与中断各发一条 approval.resolved；cancel_session 的已由其代发（entry 不在 = 有人代发，不重复）。
    pub async fn wait(&self, id: &str, rx: oneshot::Receiver<bool>, cancel: Option<&crate::agent::cancel::CancelToken>) -> ApprovalOutcome {
        // 唤醒源三态：用户应答 / 发送方 drop（cancel_session 清场）/ abort 令牌
        enum Wake {
            Respond(bool),
            Closed,
            Aborted,
        }
        let decided = async move {
            let wake = |r: Result<bool, oneshot::error::RecvError>| match r {
                Ok(v) => Wake::Respond(v),
                Err(_) => Wake::Closed,
            };
            match cancel {
                Some(token) => tokio::select! {
                    r = rx => wake(r),
                    _ = token.wait() => Wake::Aborted,
                },
                None => wake(rx.await),
            }
        };
        let (outcome, lapsed) = match tokio::time::timeout(self.timeout, decided).await {
            Ok(Wake::Respond(true)) => (ApprovalOutcome::Allow, None),
            Ok(Wake::Respond(false)) => (ApprovalOutcome::Deny, None),
            Ok(Wake::Closed) => (ApprovalOutcome::Deny, None),
            Ok(Wake::Aborted) => (ApprovalOutcome::Deny, Some("cancelled")),
            Err(_) => (ApprovalOutcome::Timeout, Some("timeout")),
        };
        let entry = self.pending.lock().expect("approvals").remove(id);
        if let (Some(outcome_str), Some(entry)) = (lapsed, entry) {
            self.publish_resolved(id, &entry.session_id, outcome_str);
        }
        outcome
    }
}

/// 共享审批请求：登记 + 发事件 + 挂起等用户决定（ApprovalOutcome::Allow = 放行）。
/// payload 双写 reason 与 message：前端审批卡读 message，旧消费方读 reason。
/// 空归属（worktree 删除等 workspace 级审批）不带 session_id，与 publish_resolved 同款：
/// stream ACL 会把空串算成 topic `session:`，无人订阅则全连接丢帧，审批卡永远渲染不出（300s 超时）。
pub async fn request_approval(appr: &crate::tools::exec::ApprovalCtx<'_>, command: &str, reason: &str) -> ApprovalOutcome {
    let (id, rx) = appr.broker.register(appr.session_id);
    let mut payload = serde_json::json!({
        "kind": "approval",
        "approval_id": id,
        "command": command,
        "reason": reason,
        "message": reason,
    });
    if !appr.session_id.is_empty() {
        payload.as_object_mut().expect("approval payload").insert("session_id".into(), serde_json::json!(appr.session_id));
    }
    appr.bus.publish(crate::core::event::Event::LlmDelta(payload));
    appr.broker.wait(&id, rx, appr.cancel).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn timeout_yields_timeout_outcome() {
        let broker = ApprovalBroker::with_timeout(std::time::Duration::from_millis(50));
        let (id, rx) = broker.register("s1");
        let outcome = broker.wait(&id, rx, None).await;
        assert_eq!(outcome, ApprovalOutcome::Timeout);
        assert_eq!(broker.cancel_session("s1"), 0, "wait 兜底已摘除，不得泄漏");
    }

    #[tokio::test]
    async fn abort_wakes_as_deny() {
        let broker = ApprovalBroker::with_timeout(std::time::Duration::from_secs(60));
        let (id, rx) = broker.register("s1");
        let token = crate::agent::cancel::CancelToken::new();
        let t2 = token.clone();
        let waiter = tokio::spawn(async move { broker.wait(&id, rx, Some(&t2)).await });
        tokio::task::yield_now().await;
        token.cancel();
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(1), waiter).await.unwrap().unwrap();
        assert_eq!(outcome, ApprovalOutcome::Deny, "abort 一律按拒绝，绝不卡住取消路径");
    }

    #[tokio::test]
    async fn cancel_session_only_clears_own_session() {
        let broker = ApprovalBroker::with_timeout(std::time::Duration::from_secs(60));
        let (id_a, rx_a) = broker.register("s1");
        let (id_b, _rx_b) = broker.register("s2");
        assert_eq!(broker.cancel_session("s1"), 1);
        assert_eq!(broker.cancel_session("s1"), 0, "重复清场幂等");
        // s1 的等待方收到关闭信号：按 deny
        let outcome = broker.wait(&id_a, rx_a, None).await;
        assert_eq!(outcome, ApprovalOutcome::Deny);
        // s2 不受影响：正常应答放行
        assert!(broker.respond(&id_b, true));
        assert_eq!(broker.cancel_session("s2"), 0, "respond 已消费，map 里不再残留");
    }

    #[tokio::test]
    async fn respond_allow_then_map_is_empty() {
        let broker = ApprovalBroker::new();
        let (id, rx) = broker.register("s1");
        assert!(broker.respond(&id, true));
        let outcome = broker.wait(&id, rx, None).await;
        assert_eq!(outcome, ApprovalOutcome::Allow);
        // 全部消费后 pending map 必须空（二次应答不得命中幽灵 id）
        assert!(!broker.respond(&id, true));
        let total: usize = broker.cancel_session("s1") + broker.cancel_session("s2");
        assert_eq!(total, 0);
    }

    fn resolved_payload(event: &crate::core::event::Event) -> Option<&serde_json::Value> {
        match event {
            crate::core::event::Event::LlmDelta(v) if v.get("kind").and_then(serde_json::Value::as_str) == Some("approval.resolved") => {
                Some(v)
            }
            _ => None,
        }
    }

    #[tokio::test]
    async fn timeout_publishes_resolved_event() {
        let bus = crate::core::event::EventBus::new(16);
        let mut sub = bus.subscribe();
        let broker = ApprovalBroker::with_timeout(std::time::Duration::from_millis(50)).with_bus(bus);
        let (id, rx) = broker.register("s1");
        let outcome = broker.wait(&id, rx, None).await;
        assert_eq!(outcome, ApprovalOutcome::Timeout);
        let event = sub.try_recv().expect("超时必须发 approval.resolved");
        let payload = resolved_payload(&event).expect("必须是 approval.resolved 帧");
        assert_eq!(payload["approval_id"], serde_json::json!(id));
        assert_eq!(payload["outcome"], serde_json::json!("timeout"));
        assert_eq!(payload["session_id"], serde_json::json!("s1"));
        assert!(sub.try_recv().is_err(), "同一条审批只发一次 resolved");
    }

    #[tokio::test]
    async fn cancel_session_publishes_resolved_and_wait_does_not_repeat() {
        let bus = crate::core::event::EventBus::new(16);
        let mut sub = bus.subscribe();
        let broker = ApprovalBroker::with_timeout(std::time::Duration::from_secs(60)).with_bus(bus);
        let (id, rx) = broker.register("s1");
        assert_eq!(broker.cancel_session("s1"), 1);
        let event = sub.try_recv().expect("清场必须发 approval.resolved");
        let payload = resolved_payload(&event).expect("必须是 approval.resolved 帧");
        assert_eq!(payload["approval_id"], serde_json::json!(id));
        assert_eq!(payload["outcome"], serde_json::json!("cancelled"));
        // 等待方收关闭信号按 deny 唤醒，且不重复发事件
        let outcome = broker.wait(&id, rx, None).await;
        assert_eq!(outcome, ApprovalOutcome::Deny);
        assert!(sub.try_recv().is_err(), "cancel_session 代发后 wait 不得重复发");
    }

    #[tokio::test]
    async fn abort_publishes_cancelled() {
        let bus = crate::core::event::EventBus::new(16);
        let mut sub = bus.subscribe();
        let broker = ApprovalBroker::with_timeout(std::time::Duration::from_secs(60)).with_bus(bus);
        let (id, rx) = broker.register("s1");
        let token = crate::agent::cancel::CancelToken::new();
        let t2 = token.clone();
        let waiter = tokio::spawn(async move { broker.wait(&id, rx, Some(&t2)).await });
        tokio::task::yield_now().await;
        token.cancel();
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(1), waiter).await.unwrap().unwrap();
        assert_eq!(outcome, ApprovalOutcome::Deny);
        let event = sub.try_recv().expect("abort 必须发 approval.resolved");
        let payload = resolved_payload(&event).expect("必须是 approval.resolved 帧");
        assert_eq!(payload["outcome"], serde_json::json!("cancelled"));
        assert_eq!(payload["session_id"], serde_json::json!("s1"));
    }

    #[tokio::test]
    async fn respond_does_not_publish() {
        let bus = crate::core::event::EventBus::new(16);
        let mut sub = bus.subscribe();
        let broker = ApprovalBroker::with_timeout(std::time::Duration::from_secs(60)).with_bus(bus);
        let (id, rx) = broker.register("s1");
        assert!(broker.respond(&id, false));
        let outcome = broker.wait(&id, rx, None).await;
        assert_eq!(outcome, ApprovalOutcome::Deny);
        assert!(sub.try_recv().is_err(), "用户正常应答不发 resolved（前端已乐观上屏）");
    }

    #[tokio::test]
    async fn workspace_approval_resolved_has_no_session_id() {
        // workspace 信任门 register("")：resolved 帧不得带 session_id，否则被 stream ACL 当无人订阅的会话帧丢弃
        let bus = crate::core::event::EventBus::new(16);
        let mut sub = bus.subscribe();
        let broker = ApprovalBroker::with_timeout(std::time::Duration::from_millis(50)).with_bus(bus);
        let (_id, rx) = broker.register("");
        let outcome = broker.wait(&_id, rx, None).await;
        assert_eq!(outcome, ApprovalOutcome::Timeout);
        let event = sub.try_recv().expect("超时必须发 approval.resolved");
        let payload = resolved_payload(&event).expect("必须是 approval.resolved 帧");
        assert!(payload.get("session_id").is_none(), "空归属审批的 resolved 帧不带 session_id");
    }

    #[tokio::test]
    async fn request_approval_omits_session_id_when_empty() {
        // worktree 删除走 ApprovalCtx::new(..., None)：请求帧空串 session_id 会被 ACL 算成 `session:` 全连接丢帧
        let bus = crate::core::event::EventBus::new(16);
        let mut sub = bus.subscribe();
        let broker = ApprovalBroker::with_timeout(std::time::Duration::from_millis(50));
        let ctx = crate::tools::exec::ApprovalCtx { broker: &broker, bus: &bus, cancel: None, session_id: "" };
        let outcome = request_approval(&ctx, "git worktree remove wt1", "r").await;
        assert_eq!(outcome, ApprovalOutcome::Timeout);
        let event = sub.try_recv().expect("必须发 approval 请求帧");
        let crate::core::event::Event::LlmDelta(payload) = event else {
            panic!("必须是 LlmDelta 帧");
        };
        assert_eq!(payload["kind"], serde_json::json!("approval"));
        assert!(payload.get("session_id").is_none(), "空归属审批请求帧不带 session_id");
    }

    #[tokio::test]
    async fn request_approval_keeps_session_id_when_present() {
        let bus = crate::core::event::EventBus::new(16);
        let mut sub = bus.subscribe();
        let broker = ApprovalBroker::with_timeout(std::time::Duration::from_millis(50));
        let ctx = crate::tools::exec::ApprovalCtx { broker: &broker, bus: &bus, cancel: None, session_id: "s1" };
        let outcome = request_approval(&ctx, "cmd", "r").await;
        assert_eq!(outcome, ApprovalOutcome::Timeout);
        let event = sub.try_recv().expect("必须发 approval 请求帧");
        let crate::core::event::Event::LlmDelta(payload) = event else {
            panic!("必须是 LlmDelta 帧");
        };
        assert_eq!(payload["session_id"], serde_json::json!("s1"), "会话归属审批照常带 session_id");
    }
}
