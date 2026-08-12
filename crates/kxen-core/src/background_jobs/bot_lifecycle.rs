use crate::AppState;
use std::sync::Arc;

pub(super) fn reconcile_inactive_runs(state: &Arc<AppState>, now: u64) {
    let runs = match state.bots.runs().list() {
        Ok(runs) => runs,
        Err(error) => {
            tracing::error!(%error, "inactive BotRun scan failed");
            return;
        }
    };
    for run in runs.into_iter().filter(|run| {
        !run.status.is_terminal()
            && !state
                .bots
                .definitions()
                .get(&run.spec.bot_id)
                .is_ok_and(|bot| bot.lifecycle == kxen_core::bot::BotLifecycle::Active && bot.current_revision().is_some())
    }) {
        let key = match kxen_core::bot::system::stable_idempotency("inactive_bot_cancel", &[run.spec.run_id.as_str()]) {
            Ok(key) => key,
            Err(error) => {
                tracing::error!(run_id = %run.spec.run_id, %error, "inactive BotRun cancellation id failed");
                continue;
            }
        };
        let requested = state.bots.runs().execute(kxen_core::bot::run::RunWrite {
            run_id: run.spec.run_id.clone(),
            expected_version: run.event_version,
            idempotency_key: key,
            actor: kxen_core::core::identity::ActorRef::System { actor: kxen_core::core::identity::SystemActor::Runtime },
            trace: kxen_core::core::identity::TraceContext::default(),
            command: kxen_core::bot::run::RunCommand::RequestCancel { reason: "owning Bot is not active".into(), at_ms: now },
        });
        match requested {
            Ok(_) => {
                state.bot_executor.cancel(&run.spec.run_id);
            }
            Err(error) if run.cancellation_requested.is_some() => {
                state.bot_executor.cancel(&run.spec.run_id);
                tracing::debug!(run_id = %run.spec.run_id, %error, "inactive BotRun already has cancellation request");
            }
            Err(error) => tracing::error!(run_id = %run.spec.run_id, %error, "inactive BotRun cancellation failed"),
        }
    }
}
