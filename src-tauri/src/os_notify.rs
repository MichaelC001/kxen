//! OS 桌面通知（会话完成）+ 点击跳回来源会话。
//! tauri-plugin-notification 桌面端只有发接口：action handler（register_action_types / on_action）是
//! mobile-only，桌面 show() 拿不到点击回调。改走其底层 notify-rust 的 wait_for_action：
//! 投递路径同一实现（行为不变），多拿点击语义 -> 聚焦主窗口 + emit 事件由前端切会话。

use tauri::{AppHandle, Emitter, Manager};

/// 前端切会话事件（payload = session_id；App.tsx 经 lib/os-notify.ts 挂 listen）。
pub const CLICK_EVENT: &str = "os-notification-click";

/// 发「kxen 会话完成」桌面通知；点击通知体 -> 聚焦主窗口 + emit CLICK_EVENT。
pub fn notify_session_done(app: &AppHandle, session_id: &str, title: &str) {
    let Ok(handle) = notify_rust::Notification::new().summary("kxen 会话完成").body(title).show() else {
        tracing::warn!("desktop notification failed");
        return;
    };
    let app = app.clone();
    let sid = session_id.to_string();
    // wait_for_action 阻塞到用户点击/关闭：独占后台线程，不占 async runtime worker；
    // 通知挂通知中心无人理时线程随之驻留（进程退出即收），完成通知频率低可接受。
    std::thread::spawn(move || {
        handle.wait_for_action(|action| {
            // "default" = 点击通知体；"__closed"/自定义 action 不跳
            if action != "default" {
                return;
            }
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
            let _ = app.emit(CLICK_EVENT, sid);
        });
    });
}
