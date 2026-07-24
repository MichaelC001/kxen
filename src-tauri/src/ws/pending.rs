//! pending queue 的 AppHandle 侧接线（P1-13）：启动恢复续跑。

use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::AppState;

/// 启动恢复：上次退出前排队的消息逐 session 弹首条续跑，run 收尾会依次消化剩余。
/// 立即续跑而非等用户再发消息：「已排队」是后端对用户消息的承诺，重启不该变成无限搁置。
pub(crate) fn restore_queues(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<Arc<AppState>>();
        for sid in state.pending_messages.restore() {
            // 会话已删（队列文件残留）：清盘不续跑
            if kxen_app::core::session::load_meta(&kxen_app::core::paths::sessions_dir(), &sid).is_err() {
                state.pending_messages.clear(&sid);
                continue;
            }
            let Some(q) = state.pending_messages.pop(&sid) else {
                continue;
            };
            let stream_id = super::protocol::stream_id("run");
            kxen_app::core::shared::lock(&state.run_streams).insert(stream_id.clone(), sid.clone());
            tokio::spawn(super::llm_task::run_llm(stream_id, sid, q.text, q.context, q.images, app.clone()));
        }
    });
}
