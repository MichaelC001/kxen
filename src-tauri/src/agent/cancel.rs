//! 取消令牌：session 级 abort 的原语（flag + Notify 即时唤醒）。
//! 取消共识：单 run 一个 cancel 通道 + 中断点统一清扫 + 子代理级联。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

#[derive(Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Relaxed);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    /// 代际比较：同一次 new 派生的克隆共享同一 flag（interrupt 重发后旧 run 收尾不得误删新 run 的 token）
    pub fn same_generation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.flag, &other.flag)
    }

    /// 等待取消（已取消立即返回）。
    pub async fn wait(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
}

/// 代际匹配摘除：entry 仍是本 run 的 token 才删。
/// interrupt 策略下新 run 已在 active_runs 占位，旧 run 收尾无条件 remove 会把新 run 的 abort 通道一并删掉。
pub fn remove_if_current(map: &mut std::collections::HashMap<String, CancelToken>, key: &str, token: &CancelToken) {
    if map.get(key).is_some_and(|t| t.same_generation(token)) {
        map.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancel_wakes_waiters() {
        let token = CancelToken::new();
        let waiter = token.clone();
        let handle = tokio::spawn(async move {
            waiter.wait().await;
        });
        tokio::task::yield_now().await;
        token.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(1), handle).await.unwrap().unwrap();
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn cancelled_before_wait_returns_immediately() {
        let token = CancelToken::new();
        token.cancel();
        tokio::time::timeout(std::time::Duration::from_millis(50), token.wait()).await.unwrap();
    }

    #[test]
    fn same_generation_distinguishes_clones_from_new_tokens() {
        let a = CancelToken::new();
        let b = a.clone();
        let c = CancelToken::new();
        assert!(a.same_generation(&b));
        assert!(!a.same_generation(&c));
    }

    #[test]
    fn remove_if_current_only_removes_same_generation() {
        let mut map = std::collections::HashMap::new();
        let old = CancelToken::new();
        let new = CancelToken::new();
        map.insert("s".to_string(), new.clone());
        // interrupt 场景：entry 已是新 run 的 token，旧 run 收尾不得删（否则 abort 够不着新 run）
        remove_if_current(&mut map, "s", &old);
        assert!(map.contains_key("s"));
        // 本 run 收尾：代际匹配，正常摘除
        remove_if_current(&mut map, "s", &new);
        assert!(!map.contains_key("s"));
        // entry 不存在时调用安全（幂等）
        remove_if_current(&mut map, "s", &new);
    }
}
