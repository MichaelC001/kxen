//! 通知落盘 bus 订阅（与桌面 main.rs setup 等价，去掉 GUI 专属的 OS 通知分支）：
//! Event::Notification -> notification hook 分发 + 环形缓冲落盘（顶栏通知中心数据源）。

use std::sync::Arc;

use kxen_core::AppState;
use kxen_core::core::event::{Event, RecvVerdict, recv_verdict};

pub fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut rx = state.bus.subscribe();
        // Lagged 跳过继续收（静默退出 = 通知中心永久停更），Closed（进程退出）才停
        loop {
            let event = match recv_verdict(rx.recv().await) {
                RecvVerdict::Event(event) => event,
                RecvVerdict::Skip => continue,
                RecvVerdict::Stop => break,
            };
            if let Event::Notification { text, session_id } = event {
                dispatch_hook(&state, text.clone(), session_id.clone());
                persist(&state, text, session_id);
            }
        }
    });
}

/// notification hook（全部 Notification 事件的单一收口点；Ask 档走审批）。
fn dispatch_hook(state: &Arc<AppState>, text: String, session_id: Option<String>) {
    let active = kxen_core::core::shared::read(&state.active_workspace).clone();
    let runtime = notification_workdir(&kxen_core::core::paths::sessions_dir(), &active, session_id.as_deref())
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
        let approval = kxen_core::tools::exec::ApprovalCtx::new(Some(broker.as_ref()), Some(&bus), None, None);
        let payload = &serde_json::json!({ "text": text, "session_id": session_id });
        if let Err(error) = runtime.hooks().run_named_with_approval("notification", &text, payload, approval.as_ref()).await {
            tracing::warn!(%error, "notification hook failed");
        }
    });
}

/// 通知进环形缓冲并落盘；落盘失败回滚缓冲（与桌面一致）。
fn persist(state: &Arc<AppState>, text: String, session_id: Option<String>) {
    let now = kxen_core::core::shared::now_ms();
    let mut buf = kxen_core::core::shared::lock(&state.notifications);
    let previous = buf.clone();
    kxen_core::core::notifications::push(&mut buf, now, text, session_id);
    if let Err(error) = kxen_core::core::notifications::persist_checked(&buf) {
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
        Some(id) => kxen_core::core::session::load_meta(sessions_dir, id)
            .map(|meta| std::path::PathBuf::from(meta.directory))
            .map_err(|error| format!("notification session {id}: {error}")),
        None => Ok(active_workspace.to_path_buf()),
    }
}
