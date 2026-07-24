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
        Self {
            pending: Mutex::new(HashMap::new()),
            timeout,
        }
    }

    /// 登记一条审批：返回 (id, 等待句柄)。session_id 记归属，cancel_session 按会话清场。
    pub fn register(&self, session_id: &str) -> (String, oneshot::Receiver<bool>) {
        let id = crate::core::ids::new_id("appr");
        let (tx, rx) = oneshot::channel();
        self.pending.lock().expect("approvals").insert(
            id.clone(),
            PendingEntry {
                tx,
                session_id: session_id.to_string(),
            },
        );
        (id, rx)
    }

    /// 用户应答（RPC 通道）：id 存在则送达并返回 true。
    pub fn respond(&self, id: &str, allow: bool) -> bool {
        self.pending
            .lock()
            .expect("approvals")
            .remove(id)
            .map(|e| e.tx.send(allow).is_ok())
            .unwrap_or(false)
    }

    /// 会话清场：摘走该 session 全部 pending（tx 随 entry drop，等待方收关闭信号按 deny）。
    pub fn cancel_session(&self, session_id: &str) -> usize {
        let mut map = self.pending.lock().expect("approvals");
        let ids: Vec<String> = map
            .iter()
            .filter(|(_, e)| e.session_id == session_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ids {
            map.remove(id);
        }
        ids.len()
    }

    /// 等待决定：abort 优先（视为拒绝）；超时自动 Timeout。
    /// 返回前兜底摘除：respond/cancel_session 已摘则无操作，其余路径（超时）防泄漏。
    pub async fn wait(
        &self,
        id: &str,
        rx: oneshot::Receiver<bool>,
        cancel: Option<&crate::agent::cancel::CancelToken>,
    ) -> ApprovalOutcome {
        let decided = async move {
            match cancel {
                Some(token) => tokio::select! {
                    r = rx => r.unwrap_or(false),
                    _ = token.wait() => false,
                },
                None => rx.await.unwrap_or(false),
            }
        };
        let outcome = match tokio::time::timeout(self.timeout, decided).await {
            Ok(true) => ApprovalOutcome::Allow,
            Ok(false) => ApprovalOutcome::Deny,
            Err(_) => ApprovalOutcome::Timeout,
        };
        self.pending.lock().expect("approvals").remove(id);
        outcome
    }
}

/// 共享审批请求：登记 + 发事件 + 挂起等用户决定（ApprovalOutcome::Allow = 放行）。
/// payload 双写 reason 与 message：前端审批卡读 message，旧消费方读 reason。
pub async fn request_approval(
    appr: &crate::tools::exec::ApprovalCtx<'_>,
    command: &str,
    reason: &str,
) -> ApprovalOutcome {
    let (id, rx) = appr.broker.register(appr.session_id);
    appr.bus.publish(crate::core::event::Event::LlmDelta(serde_json::json!({
        "kind": "approval",
        "approval_id": id,
        "command": command,
        "reason": reason,
        "message": reason,
        "session_id": appr.session_id,
    })));
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
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
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
}
