//! workflow 取消装配：父 run abort 级联进 workflow 令牌 + 作用域结束的统一取消守卫。
//! 独立文件守 350 行门禁（workflow.rs 承载引擎主体）。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 父 run abort 级联进 workflow 令牌（与 dispatch 的父子级联同一共识，done_tx drop 回收 watcher）。
pub(crate) fn cascade_parent(
    parent: Option<crate::agent::cancel::CancelToken>,
    child: &crate::agent::cancel::CancelToken,
) -> Option<tokio::sync::oneshot::Sender<()>> {
    parent.map(|parent| {
        let child = child.clone();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            tokio::select! {
                _ = parent.wait() => child.cancel(),
                _ = done_rx => {}
            }
        });
        done_tx
    })
}

/// 作用域结束即触发 JS 中断 + 在飞子代理级联取消（覆盖超时与提前返回两条路径）。
pub(crate) struct CancelGuard(pub Arc<AtomicBool>, pub crate::agent::cancel::CancelToken);

impl Drop for CancelGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
        self.1.cancel();
    }
}
