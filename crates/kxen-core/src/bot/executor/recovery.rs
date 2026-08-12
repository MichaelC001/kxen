use super::BotExecutor;
use crate::bot::run::{BotRunState, RunCommand, RunStatus};
use crate::core::identity::ResourceId;

impl BotExecutor {
    pub(super) fn block_unknown_recovery(&self, run: &BotRunState) -> Result<(), String> {
        if run.status != RunStatus::Running {
            return Ok(());
        }
        if let Some((operation_id, attempt)) = run.tool_operations.iter().find_map(|(id, operation)| {
            operation
                .attempt
                .as_ref()
                .filter(|attempt| attempt.phase == crate::core::operation::AttemptPhase::Started)
                .map(|attempt| (id, attempt))
        }) {
            let reason = "process restarted after tool start without durable outcome";
            self.write(
                &run.spec.run_id,
                &format!("{}_startup_unknown", operation_id),
                RunCommand::MarkToolUnknown {
                    operation_id: operation_id.clone(),
                    generation: attempt.generation.clone(),
                    reason: reason.into(),
                    evidence: Vec::new(),
                    at_ms: crate::core::shared::now_ms(),
                },
            )?;
            self.system
                .recovery()
                .open(
                    crate::core::identity::AggregateRef { kind: crate::core::identity::AggregateKind::BotRun, id: run.spec.run_id.clone() },
                    reason,
                    vec![operation_id.to_string()],
                    crate::core::shared::now_ms(),
                )
                .map_err(|error| error.to_string())?;
            return Err("BotRun blocked because a tool outcome is UNKNOWN".into());
        }
        Ok(())
    }

    pub(super) fn persist_execution_error(&self, run_id: &ResourceId, error: &str) {
        let Ok(mut run) = self.system.runs().get(run_id) else { return };
        if run.status.is_terminal() || matches!(run.status, RunStatus::ApprovalRequired | RunStatus::InputRequired) {
            return;
        }
        if let Some((operation_id, attempt)) = run.tool_operations.iter().find_map(|(id, operation)| {
            operation
                .attempt
                .as_ref()
                .filter(|attempt| attempt.phase == crate::core::operation::AttemptPhase::Started)
                .map(|attempt| (id.clone(), attempt.clone()))
        }) {
            let reason = format!("tool outcome became UNKNOWN after runtime error: {error}");
            if let Ok(blocked) = self.write(
                run_id,
                &format!("{}_runtime_unknown", operation_id),
                RunCommand::MarkToolUnknown {
                    operation_id: operation_id.clone(),
                    generation: attempt.generation,
                    reason: reason.clone(),
                    evidence: Vec::new(),
                    at_ms: crate::core::shared::now_ms(),
                },
            ) {
                run = blocked;
                let _ = self.system.recovery().open(
                    crate::core::identity::AggregateRef { kind: crate::core::identity::AggregateKind::BotRun, id: run.spec.run_id.clone() },
                    reason,
                    vec![operation_id.to_string()],
                    crate::core::shared::now_ms(),
                );
            }
            return;
        }
        if run.tool_operations.values().any(|operation| {
            operation.attempt.as_ref().is_some_and(|attempt| attempt.phase == crate::core::operation::AttemptPhase::OutcomeKnown)
        }) {
            // The effect and outcome are durable. The next execution settles
            // the operation and replays it instead of invoking it again.
            return;
        }
        if run.status == RunStatus::Queued {
            if let Ok(started) = self.write(run_id, "error_start", RunCommand::Start { at_ms: crate::core::shared::now_ms() }) {
                run = started;
            } else {
                return;
            }
        }
        if run.status == RunStatus::Running {
            let _ = self.write(
                run_id,
                "execution_error",
                RunCommand::Fail {
                    code: "runtime_execution_error".into(),
                    message: error.to_string(),
                    usage: run.usage,
                    at_ms: crate::core::shared::now_ms(),
                },
            );
        }
    }
}
