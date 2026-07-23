//! 审批 broker：工具执行挂起等用户决定（允许/拒绝），RPC 应答唤醒。
//! 中断（abort）一律视为拒绝——审批等待绝不卡住取消路径。

use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::oneshot;

#[derive(Default)]
pub struct ApprovalBroker {
    pending: Mutex<HashMap<String, oneshot::Sender<bool>>>,
}

impl ApprovalBroker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一条审批：返回 (id, 等待句柄)。
    pub fn register(&self) -> (String, oneshot::Receiver<bool>) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let id = format!("appr_{now}_{:04x}", std::process::id() & 0xffff);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().expect("approvals").insert(id.clone(), tx);
        (id, rx)
    }

    /// 用户应答（RPC 通道）：id 存在则送达并返回 true。
    pub fn respond(&self, id: &str, allow: bool) -> bool {
        self.pending
            .lock()
            .expect("approvals")
            .remove(id)
            .map(|tx| tx.send(allow).is_ok())
            .unwrap_or(false)
    }

    /// 等待决定：abort 优先（视为拒绝）。
    pub async fn wait(&self, rx: oneshot::Receiver<bool>, cancel: Option<&crate::agent::cancel::CancelToken>) -> bool {
        match cancel {
            Some(token) => tokio::select! {
                r = rx => r.unwrap_or(false),
                _ = token.wait() => false,
            },
            None => rx.await.unwrap_or(false),
        }
    }
}
