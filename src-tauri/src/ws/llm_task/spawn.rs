use std::sync::Arc;

use crate::AppState;

pub(in crate::ws) struct RunInput {
    pub(in crate::ws) stream_id: String,
    pub(in crate::ws) session_id: String,
    pub(in crate::ws) text: String,
    pub(in crate::ws) context: Vec<kxen_gui::agent::context::ContextItem>,
    pub(in crate::ws) images: Vec<kxen_gui::llm::types::ImagePart>,
    pub(in crate::ws) queue_delivery_id: Option<String>,
    pub(in crate::ws) queue_created_at: Option<u64>,
    pub(in crate::ws) schedule_job_id: Option<String>,
    pub(in crate::ws) state: Arc<AppState>,
}

/// async run 收尾续跑的普通函数断路器，避免 future 类型递归自嵌套。
pub(in crate::ws) fn spawn_claimed_run(input: RunInput, cancel: kxen_gui::agent::cancel::CancelToken) {
    tokio::spawn(super::run_llm_inner(input, Some(cancel)));
}
