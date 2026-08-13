use crate::AppState;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

mod bot_lifecycle;

const SCHEDULE_INTERVAL: Duration = Duration::from_secs(15);
const CONSOLIDATION_INTERVAL: Duration = Duration::from_secs(30 * 60);
/// kanban 列驱动 tick 5s：卡片流转的响应下限；扫描是纯文件读且板数为个位数，开销可忽略。
/// 周期扫描同时承担崩溃恢复（orphan run 检测），比事件钩子少一条要保活的触发路径。
const KANBAN_INTERVAL: Duration = Duration::from_secs(5);
const BOT_INTERVAL: Duration = Duration::from_secs(1);

pub fn spawn(state: Arc<AppState>) {
    let schedule_state = state.clone();
    tokio::spawn(run_periodic(SCHEDULE_INTERVAL, move || {
        let state = schedule_state.clone();
        async move { dispatch_schedule_tick(state) }
    }));
    let consolidation_state = state.clone();
    tokio::spawn(run_periodic(CONSOLIDATION_INTERVAL, move || {
        let state = consolidation_state.clone();
        async move { consolidate_knowledge(state).await }
    }));
    let kanban_state = state.clone();
    tokio::spawn(run_periodic(KANBAN_INTERVAL, move || {
        let state = kanban_state.clone();
        async move { kxen_core::kanban::tick(&state).await }
    }));
    tokio::spawn(run_periodic(BOT_INTERVAL, move || {
        let state = state.clone();
        async move { bot_tick(state).await }
    }));
}

async fn bot_tick(state: Arc<AppState>) {
    let now = kxen_core::core::shared::now_ms();
    match state.bots.reconcile_inactive_bot_work(now) {
        Ok(changes) if changes > 0 => state.bus.publish(kxen_core::core::event::Event::BotUpdate {
            topic: "bot-lifecycle".into(),
            aggregate_id: "inactive_work".into(),
            seq: now,
        }),
        Ok(_) => {}
        Err(error) => tracing::error!(%error, "inactive Bot work reconciliation failed"),
    }
    bot_lifecycle::reconcile_inactive_runs(&state, now);
    match state.bots.reconcile_group_lifecycle(now) {
        Ok(paused) if paused > 0 => state.bus.publish(kxen_core::core::event::Event::BotUpdate {
            topic: "bot-groups".into(),
            aggregate_id: "group_lifecycle".into(),
            seq: now,
        }),
        Ok(_) => {}
        Err(error) => tracing::error!(%error, "Bot Group lifecycle reconciliation failed"),
    }
    let report = state.bots.tick_routines(now);
    if !report.queued_run_ids.is_empty() || report.skipped_occurrences > 0 {
        state.bus.publish(kxen_core::core::event::Event::BotUpdate {
            topic: "bot-routines".into(),
            aggregate_id: "routine_scheduler".into(),
            seq: now,
        });
    }
    for error in report.errors {
        tracing::error!(%error, "Bot Routine tick failed");
    }
    for error in state.bots.reconcile_runs(now) {
        tracing::error!(%error, "BotRun settlement reconciliation failed");
    }
    for _ in 0..16 {
        match state.bots.dispatch_next_delivery(now) {
            Ok(Some(receipt)) => state.bus.publish(kxen_core::core::event::Event::BotUpdate {
                topic: format!("bot-conversation:{}", receipt.conversation_id),
                aggregate_id: receipt.conversation_id.to_string(),
                seq: state.bots.conversations().get(&receipt.conversation_id).map_or(0, |value| value.event_version),
            }),
            Ok(None) => break,
            Err(error) => {
                tracing::error!(%error, "Bot Delivery dispatch failed");
                break;
            }
        }
    }
    let workspace = match state.active_workspace.read() {
        Ok(workspace) => workspace.clone(),
        Err(_) => return,
    };
    let runs: Vec<_> = match state.bots.runs().recoverable() {
        Ok(runs) => runs
            .into_iter()
            .filter(|run| {
                run.cancellation_requested.is_some()
                    || state
                        .bots
                        .definitions()
                        .get(&run.spec.bot_id)
                        .is_ok_and(|bot| bot.lifecycle == kxen_core::bot::BotLifecycle::Active && bot.current_revision().is_some())
            })
            .collect(),
        Err(error) => {
            tracing::error!(%error, "BotRun recovery scan failed");
            return;
        }
    };
    for run in runs.into_iter().take(8) {
        let state = state.clone();
        let workspace = workspace.clone();
        tokio::spawn(async move {
            match state.bot_executor.execute(&run.spec.run_id, &workspace).await {
                Ok(run) => state.bus.publish(kxen_core::core::event::Event::BotUpdate {
                    topic: format!("bot-run:{}", run.spec.run_id),
                    aggregate_id: run.spec.run_id.to_string(),
                    seq: run.event_version,
                }),
                Err(error) if error.contains("already active") => {}
                Err(error) => tracing::error!(run_id = %run.spec.run_id, %error, "BotRun execution failed"),
            }
        });
    }
}

async fn run_periodic<Job, JobFuture>(interval: Duration, mut job: Job)
where
    Job: FnMut() -> JobFuture + Send + 'static,
    JobFuture: Future<Output = ()> + Send + 'static,
{
    loop {
        tokio::time::sleep(interval).await;
        job().await;
    }
}

async fn consolidate_knowledge(state: Arc<AppState>) {
    if !kxen_core::core::config::experimental_config().automatic_knowledge_distillation {
        return;
    }
    let store = kxen_core::core::shared::lock(&state.auth_store).clone();
    let result = kxen_core::knowledge::consolidate::run_once_with(&store, &state.session_tokens, |session| {
        consolidation_route(&state.workspace_runtimes, &store, session)
    })
    .await;
    for diagnostic in &result.diagnostics {
        tracing::error!(error = %diagnostic, "memory consolidation failed");
        state.bus.publish(kxen_core::core::event::Event::notify(format!("后台知识整理失败：{diagnostic}"), None));
    }
    if result.written > 0 {
        tracing::info!(written = result.written, "memory consolidation distilled");
    }
}

fn consolidation_route(
    runtimes: &kxen_core::workspace_runtime::WorkspaceRuntimeRegistry,
    store: &kxen_core::auth::credential::AuthStore,
    session: &kxen_core::core::session::Session,
) -> Result<kxen_core::knowledge::consolidate::SessionRoute, String> {
    let runtime = runtimes.runtime(std::path::Path::new(&session.directory))?;
    let mrm = runtime.mrm();
    let default = match mrm.role("chat") {
        Some(binding) => {
            let mut model = kxen_core::llm::ModelRef::new(binding.provider, binding.model);
            model.account = binding.account;
            model
        }
        None => kxen_core::llm::ModelRef::new("xai", "grok-build-0.1"),
    };
    let mut model = kxen_core::core::session::effective_model(session.model.as_ref(), &default).clone();
    model.account = kxen_core::auth::credential::effective_account_name(store, &model.provider, model.account.as_deref());
    Ok(kxen_core::knowledge::consolidate::SessionRoute { mrm, model })
}

fn dispatch_schedule_tick(state: Arc<AppState>) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|duration| duration.as_millis() as u64).unwrap_or(0);
    let candidates = match kxen_core::core::schedule::due_candidates(now) {
        Ok(candidates) => candidates,
        Err(error) => {
            tracing::error!(%error, "schedule tick failed");
            state.bus.publish(kxen_core::core::event::Event::notify(format!("定时任务读取或保存失败：{error}"), None));
            Vec::new()
        }
    };
    for candidate in candidates {
        let lifecycle =
            match kxen_core::core::session_lifecycle::admit_mutation(&kxen_core::core::paths::sessions_dir(), &candidate.session_id) {
                Ok(lifecycle) => lifecycle,
                Err(error) => {
                    tracing::info!(session = candidate.session_id, cron_job_id = candidate.id, %error, "schedule claim rejected");
                    continue;
                }
            };
        let job = match kxen_core::core::schedule::claim_due(&candidate.id, now) {
            Ok(Some(job)) if job.session_id == candidate.session_id => job,
            Ok(Some(job)) => {
                tracing::error!(cron_job_id = job.id, "schedule Session binding changed before claim");
                continue;
            }
            Ok(None) => continue,
            Err(error) => {
                tracing::error!(cron_job_id = candidate.id, %error, "schedule claim failed");
                continue;
            }
        };
        dispatch_schedule_job(&state, job, now, lifecycle);
    }
}

fn dispatch_schedule_job(
    state: &Arc<AppState>,
    job: kxen_core::core::schedule::CronJob,
    now: u64,
    _lifecycle: kxen_core::core::session_lifecycle::MutationGuard,
) {
    let Some(dispatch_id) = job.dispatch_id.clone() else {
        tracing::error!(cron_job_id = job.id, "claimed schedule is missing dispatch id");
        return;
    };
    let queued = state.pending_messages.enqueue_existing_committed(
        &job.session_id,
        kxen_core::core::pending_queue::QueuedMessage {
            id: dispatch_id.clone(),
            created_at: kxen_core::core::shared::now_ms(),
            text: format!("[cron {}] {}", job.id, job.prompt),
            context: vec![],
            images: vec![],
            schedule_job_id: Some(job.id.clone()),
        },
        || match kxen_core::core::schedule::ack_dispatch(&job.id, &dispatch_id, now) {
            Ok(true) => Ok(()),
            Ok(false) => Err(format!("schedule disappeared before dispatch acknowledgement: {}", job.id)),
            Err(error) => Err(error),
        },
    );
    match queued {
        Ok(position) => {
            state.bus.publish(kxen_core::core::event::Event::notify(
                format!("cron 已进入持久队列（第 {position} 条）"),
                Some(job.session_id.clone()),
            ));
            crate::ws::pending::kick_session(state.clone(), job.session_id);
        }
        Err(error) => {
            tracing::error!(cron_job_id = job.id, %error, "cron durable enqueue or acknowledgement failed");
            state
                .bus
                .publish(kxen_core::core::event::Event::notify(format!("cron 消息入队失败，将保留并重试：{error}"), Some(job.session_id)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn blocked_consolidation_clock_does_not_delay_schedule_clock() {
        let schedule_ticks = Arc::new(AtomicUsize::new(0));
        let schedule_count = schedule_ticks.clone();
        let schedule = tokio::spawn(run_periodic(Duration::from_millis(10), move || {
            let count = schedule_count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
            }
        }));
        let consolidation_started = Arc::new(AtomicUsize::new(0));
        let consolidation_count = consolidation_started.clone();
        let never = Arc::new(tokio::sync::Notify::new());
        let consolidation = tokio::spawn(run_periodic(Duration::from_millis(10), move || {
            let count = consolidation_count.clone();
            let never = never.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                never.notified().await;
            }
        }));

        tokio::time::sleep(Duration::from_millis(75)).await;

        assert_eq!(consolidation_started.load(Ordering::SeqCst), 1);
        assert!(schedule_ticks.load(Ordering::SeqCst) >= 4, "schedule clock must keep advancing while consolidation is blocked");
        schedule.abort();
        consolidation.abort();
    }

    #[test]
    fn consolidation_routes_each_session_through_its_workspace_runtime() {
        let root = std::env::temp_dir().join(format!("kxen-consolidation-routes-{}", uuid::Uuid::new_v4()));
        let workspace_a = root.join("a");
        let workspace_b = root.join("b");
        std::fs::create_dir_all(&workspace_a).unwrap();
        std::fs::create_dir_all(&workspace_b).unwrap();
        let runtimes = kxen_core::workspace_runtime::WorkspaceRuntimeRegistry::default();
        let session = |id: &str, directory: &std::path::Path, model: &str| kxen_core::core::session::Session {
            id: id.into(),
            title: id.into(),
            directory: directory.to_string_lossy().into_owned(),
            parent_id: None,
            branch_root_id: None,
            fork_point: None,
            fork_kind: None,
            created_at: 1,
            updated_at: 1,
            message_revision: 0,
            pinned: false,
            sort_order: None,
            model: Some(kxen_core::llm::ModelRef::new("xai", model)),
        };
        let auth = kxen_core::auth::credential::AuthStore::new();
        let route_a = consolidation_route(&runtimes, &auth, &session("ses_a", &workspace_a, "model-a")).unwrap();
        let route_b = consolidation_route(&runtimes, &auth, &session("ses_b", &workspace_b, "model-b")).unwrap();

        assert_eq!(route_a.model.model, "model-a");
        assert_eq!(route_b.model.model, "model-b");
        assert!(!Arc::ptr_eq(&route_a.mrm, &route_b.mrm), "different workspaces must not share one scoped MRM instance");
        std::fs::remove_dir_all(root).ok();
    }
}
