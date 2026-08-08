//! 通知落盘 bus 订阅：Event::Notification -> notification hook 分发 + 环形缓冲落盘（通知中心数据源）。
//! GUI 与 headless CLI 共用同一条订阅；桌面专属的事件分支（如 run 完成的 OS 通知）经 on_event 挂在同一事件流上。

use std::sync::Arc;

use crate::AppState;
use crate::core::event::{Event, RecvVerdict, recv_verdict};

pub fn spawn(state: Arc<AppState>) {
    spawn_with(state, |_| {});
}

/// on_event：调用方挂在同一条订阅上的额外事件分支，先于 Notification 分支触发。
pub fn spawn_with(state: Arc<AppState>, on_event: impl Fn(&Event) + Send + 'static) {
    tokio::spawn(async move {
        let mut rx = state.bus.subscribe();
        // Lagged 跳过继续收（静默退出 = 通知中心永久停更），Closed（进程退出）才停
        loop {
            let event = match recv_verdict(rx.recv().await) {
                RecvVerdict::Event(event) => event,
                RecvVerdict::Skip => continue,
                RecvVerdict::Stop => break,
            };
            on_event(&event);
            if let Event::Notification { text, session_id } = event {
                dispatch_hook(&state, text.clone(), session_id.clone());
                persist(&state, text, session_id);
            }
        }
    });
}

/// notification hook（全部 Notification 事件的单一收口点；Ask 档走审批）。
fn dispatch_hook(state: &Arc<AppState>, text: String, session_id: Option<String>) {
    let active = crate::core::shared::read(&state.active_workspace).clone();
    let runtime = notification_workdir(&crate::core::paths::sessions_dir(), &active, session_id.as_deref())
        .and_then(|workdir| state.workspace_runtimes.runtime(&workdir));
    // broker/bus 克隆进任务（借用无法跨 spawn 的 'static 边界）
    let broker = state.approvals.clone();
    let bus = state.bus.clone();
    tokio::spawn(async move {
        let runtime = match runtime {
            Ok(runtime) => runtime,
            Err(error) => {
                tracing::warn!(%error, "notification workspace runtime unavailable");
                return;
            }
        };
        let approval = crate::tools::exec::ApprovalCtx::new(Some(broker.as_ref()), Some(&bus), None, None, None);
        let payload = &serde_json::json!({ "text": text, "session_id": session_id });
        if let Err(error) = runtime.hooks().run_named_with_approval("notification", &text, payload, approval.as_ref()).await {
            tracing::warn!(%error, "notification hook failed");
        }
    });
}

/// 通知进环形缓冲并落盘；落盘失败回滚缓冲（内存变更不能误报为已提交）。
fn persist(state: &Arc<AppState>, text: String, session_id: Option<String>) {
    let now = crate::core::shared::now_ms();
    let mut buf = crate::core::shared::lock(&state.notifications);
    let previous = buf.clone();
    crate::core::notifications::push(&mut buf, now, text, session_id);
    if let Err(error) = crate::core::notifications::persist_checked(&buf) {
        *buf = previous;
        tracing::error!(%error, "notification persistence failed");
    }
}

fn notification_workdir(
    sessions_dir: &std::path::Path,
    active_workspace: &std::path::Path,
    session_id: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    match session_id {
        Some(id) => crate::core::session::load_meta(sessions_dir, id)
            .map(|meta| std::path::PathBuf::from(meta.directory))
            .map_err(|error| format!("notification session {id}: {error}")),
        None => Ok(active_workspace.to_path_buf()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_session_never_falls_back_to_active_workspace() {
        let base = std::env::temp_dir().join(format!("kxen-notification-workdir-{}", std::process::id()));
        let sessions = base.join("sessions");
        let active = base.join("active");
        let owned = base.join("owned");
        std::fs::create_dir_all(&active).unwrap();
        std::fs::create_dir_all(&owned).unwrap();
        let session = crate::core::session::create(&sessions, owned.to_str().unwrap()).unwrap();

        assert_eq!(notification_workdir(&sessions, &active, None).unwrap(), active);
        assert_eq!(notification_workdir(&sessions, &active, Some(&session.id)).unwrap(), owned);
        assert!(notification_workdir(&sessions, &active, Some("ses_missing")).is_err());
        std::fs::remove_dir_all(base).ok();
    }
}
