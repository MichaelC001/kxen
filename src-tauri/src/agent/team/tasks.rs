// ---------------- tasks（依赖自动解锁 + 串行 claim） ----------------

use crate::core::shared::lock;
use serde_json::json;
use std::sync::Arc;

use super::inbox::append_inbox;
use super::types::{TeamTask, TeamTaskStatus};
use super::TeamState;

pub(super) fn create_task(state: &Arc<TeamState>, title: &str, depends_on: Vec<u64>) -> TeamTask {
    let id = state.next_task_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let task = TeamTask { id, title: title.into(), status: TeamTaskStatus::Pending, assignee: None, depends_on };
    lock(&state.tasks).push(task.clone());
    persist_tasks(state);
    task
}

pub(super) fn claim_task(state: &Arc<TeamState>, who: &str) -> Result<String, String> {
    let mut tasks = lock(&state.tasks);
    let done: Vec<u64> = tasks.iter().filter(|t| t.status == TeamTaskStatus::Completed).map(|t| t.id).collect();
    let Some(task) = tasks.iter_mut().find(|t| {
        t.status == TeamTaskStatus::Pending && t.assignee.is_none() && t.depends_on.iter().all(|d| done.contains(d))
    }) else {
        return Err("no claimable task (all claimed or blocked by dependencies)".into());
    };
    task.status = TeamTaskStatus::InProgress;
    task.assignee = Some(who.into());
    let title = task.title.clone();
    let id = task.id;
    drop(tasks);
    persist_tasks(state);
    Ok(format!("claimed task #{id}: {title}"))
}

pub(super) async fn complete_task(state: &Arc<TeamState>, who: &str, id: u64) -> Result<String, String> {
    let title = {
        let mut tasks = lock(&state.tasks);
        let Some(task) = tasks.iter_mut().find(|t| t.id == id) else {
            return Err(format!("task not found: #{id}"));
        };
        if task.assignee.as_deref() != Some(who) {
            return Err(format!("task #{id} is not assigned to {who}"));
        }
        task.status = TeamTaskStatus::Completed;
        task.title.clone()
    };
    persist_tasks(state);
    // task_completed hook：exit 非零 = 打回（回滚 in_progress + 反馈给完成者 inbox）
    if let Some(hooks) = &state.deps.hooks {
        if let Err(feedback) = hooks.run_named("task_completed", &title, &json!({ "task_id": id, "title": title, "assignee": who })).await {
            if let Some(task) = lock(&state.tasks).iter_mut().find(|t| t.id == id) {
                task.status = TeamTaskStatus::InProgress;
            }
            persist_tasks(state);
            let _ = append_inbox(&state.dir, who, "hooks", &format!("task #{id} completion rejected: {feedback}"));
            return Err(format!("task_completed hook rejected: {feedback}"));
        }
    }
    Ok(format!("task #{id} completed"))
}

fn persist_tasks(state: &Arc<TeamState>) {
    let tasks = lock(&state.tasks).clone();
    let _ = std::fs::write(state.dir.join("tasks.json"), serde_json::to_string_pretty(&tasks).unwrap_or_default());
}
