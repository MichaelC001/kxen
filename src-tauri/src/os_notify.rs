//! OS 桌面通知：会话完成点击回跳。
//! tauri-plugin-notification 桌面端无 action handler（mobile-only），
//! 故用底层 notify-rust::wait_for_action 拿点击语义。

use kxen_core::app_state::NotifyTarget;
use tauri::{AppHandle, Emitter, Manager};

/// payload = session_id。
pub const CLICK_EVENT: &str = "os-notification-click";

/// wait_for_action 无超时口；超时丢弃放行队列。被丢弃线程杀不掉，用户日后点击仍回跳。
const JOB_WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// 串行派发 wait_for_action；job 在 detached 线程跑，进程退出即收。
struct Dispatcher {
    tx: std::sync::mpsc::Sender<WaitJob>,
}

type WaitJob = Box<dyn FnOnce() + Send + 'static>;

impl Dispatcher {
    fn new() -> Self {
        Self::with_timeout(JOB_WAIT_TIMEOUT)
    }

    fn with_timeout(timeout: std::time::Duration) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<WaitJob>();
        std::thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
                std::thread::spawn(move || {
                    job();
                    let _ = done_tx.send(());
                });
                if done_rx.recv_timeout(timeout).is_err() {
                    tracing::warn!("os notification wait timed out, job dropped");
                }
            }
        });
        Self { tx }
    }

    fn enqueue(&self, job: WaitJob) {
        // best-effort：worker 随进程退出，send 失败静默丢弃
        let _ = self.tx.send(job);
    }
}

/// 首发通知才拉起 worker，无通知进程零线程开销。
static DISPATCHER: std::sync::LazyLock<Dispatcher> = std::sync::LazyLock::new(Dispatcher::new);

/// 只发主会话非前台 done；subagent/teammate 带 agent 标记，不过滤会刷成用户会话通知。
pub fn should_notify_done(payload: &serde_json::Value, foreground_session: &str) -> bool {
    if payload.get("kind").and_then(|k| k.as_str()) != Some("done") {
        return false;
    }
    if payload.get("agent").is_some() {
        return false;
    }
    let sid = payload.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
    !sid.is_empty() && sid != foreground_session
}

/// 聚焦主窗口并 emit 点击事件；窗口 handle 仅 bin 可得，lib 默认 no-op。
struct DesktopNotify {
    app: AppHandle,
}

impl NotifyTarget for DesktopNotify {
    fn focus_and_emit(&self, session_id: &str) {
        if let Some(w) = self.app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
        let _ = self.app.emit(CLICK_EVENT, session_id);
    }
}

pub fn desktop_target(app: &AppHandle) -> std::sync::Arc<dyn NotifyTarget> {
    std::sync::Arc::new(DesktopNotify { app: app.clone() })
}

pub fn notify_session_done(target: std::sync::Arc<dyn NotifyTarget>, session_id: &str, title: &str) {
    let Ok(handle) = notify_rust::Notification::new().summary("kxen 会话完成").body(title).show() else {
        tracing::warn!("desktop notification failed");
        return;
    };
    let sid = session_id.to_string();
    // wait_for_action 阻塞至点击/关闭：丢给串行 dispatcher，不占 async runtime worker
    DISPATCHER.enqueue(Box::new(move || {
        handle.wait_for_action(|action| {
            // "default" = 点击通知体；"__closed"/自定义 action 不跳
            if action != "default" {
                return;
            }
            target.focus_and_emit(&sid);
        });
    }));
}

#[cfg(test)]
mod tests {
    use super::should_notify_done;
    use serde_json::json;

    #[test]
    fn main_session_done_notifies_unless_foreground() {
        let payload = json!({ "kind": "done", "session_id": "s1" });
        assert!(should_notify_done(&payload, "s2"), "非前台主会话 done 必须通知");
        assert!(!should_notify_done(&payload, "s1"), "前台会话不打扰");
    }

    #[test]
    fn agent_tagged_done_frames_never_notify() {
        let sub = json!({ "kind": "done", "session_id": "s1", "agent": "thinking-1" });
        assert!(!should_notify_done(&sub, "s2"));
        let team = json!({ "kind": "done", "session_id": "s1", "agent": "teammate-foo" });
        assert!(!should_notify_done(&team, "s2"));
    }

    #[test]
    fn non_done_or_missing_session_never_notify() {
        assert!(!should_notify_done(&json!({ "kind": "text", "session_id": "s1" }), "s2"));
        assert!(!should_notify_done(&json!({ "kind": "done" }), "s2"));
    }

    #[test]
    fn dispatcher_runs_jobs_serially_in_fifo_order() {
        use super::Dispatcher;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};

        let d = Dispatcher::new();
        let active = Arc::new(AtomicBool::new(false));
        let log = Arc::new(Mutex::new(Vec::new()));
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        for i in 0..3usize {
            let active = active.clone();
            let log = log.clone();
            let done = done_tx.clone();
            d.enqueue(Box::new(move || {
                // 并发会撞 active：串行时 swap 恒见 false
                assert!(!active.swap(true, Ordering::SeqCst), "job {i} 与前一个 job 并发重叠");
                std::thread::sleep(std::time::Duration::from_millis(10));
                log.lock().unwrap().push(i);
                active.store(false, Ordering::SeqCst);
                let _ = done.send(());
            }));
        }
        for _ in 0..3 {
            done_rx.recv_timeout(std::time::Duration::from_secs(5)).expect("job 应在超时前完成");
        }
        assert_eq!(*log.lock().unwrap(), vec![0, 1, 2], "FIFO 串行执行顺序");
    }

    #[test]
    fn blocked_job_is_dropped_after_timeout_and_queue_moves_on() {
        use super::Dispatcher;
        use std::time::Duration;

        let d = Dispatcher::with_timeout(Duration::from_millis(50));
        let (block_tx, block_rx) = std::sync::mpsc::channel::<()>();
        d.enqueue(Box::new(move || {
            // 模拟永不结束的 wait_for_action；末尾放行，避免挂死线程污染后续用例
            let _ = block_rx.recv();
        }));
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        d.enqueue(Box::new(move || {
            let _ = done_tx.send(());
        }));
        done_rx.recv_timeout(Duration::from_secs(5)).expect("阻塞 job 超时丢弃后，后续 job 必须执行");
        let _ = block_tx.send(());
    }
}
