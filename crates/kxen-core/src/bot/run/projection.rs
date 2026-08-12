use crate::core::operation::AttemptPhase;

use super::RunError;
use super::events::RunEvent;
use super::types::{BotRunState, RunStatus};

pub fn apply(state: &mut Option<BotRunState>, event: &RunEvent) -> Result<(), RunError> {
    match event {
        RunEvent::Queued { spec, at_ms } => {
            if state.is_some() {
                return Err(RunError::InvalidEvent("run_queued must be first".into()));
            }
            *state = Some(BotRunState {
                spec: spec.as_ref().clone(),
                status: RunStatus::Queued,
                event_version: 1,
                turns: Vec::new(),
                tool_operations: Default::default(),
                approved_operations: Default::default(),
                approval: None,
                input_request: None,
                bound_inputs: Vec::new(),
                artifacts: Vec::new(),
                usage: Default::default(),
                result: Vec::new(),
                error_code: None,
                error_message: None,
                created_at_ms: *at_ms,
                updated_at_ms: *at_ms,
            });
            return Ok(());
        }
        _ if state.is_none() => return Err(RunError::InvalidEvent("event precedes run_queued".into())),
        _ => {}
    }
    let state = state.as_mut().expect("checked above");
    if state.status.is_terminal() {
        return Err(RunError::InvalidEvent(format!("event after terminal {:?}", state.status)));
    }
    match event {
        RunEvent::Queued { .. } => unreachable!(),
        RunEvent::Started { .. } => set_status(state, &[RunStatus::Queued], RunStatus::Running)?,
        RunEvent::TurnRecorded { record, .. } => {
            if !matches!(state.status, RunStatus::Running | RunStatus::ApprovalRequired | RunStatus::InputRequired) {
                return Err(RunError::InvalidEvent(format!("turn recorded from {:?}", state.status)));
            }
            if state.turns.iter().any(|item| item.record_id == record.record_id) {
                return Err(RunError::InvalidEvent(format!("duplicate turn record {}", record.record_id)));
            }
            state.turns.push(record.clone());
        }
        RunEvent::ToolOperation { operation_id, event, .. } => {
            let finishing_paused = matches!(state.status, RunStatus::ApprovalRequired | RunStatus::InputRequired)
                && matches!(
                    event,
                    crate::core::operation::OperationEvent::OutcomeRecorded { .. } | crate::core::operation::OperationEvent::Settled { .. }
                );
            if !finishing_paused {
                require(state, RunStatus::Running)?;
            }
            if let crate::core::operation::OperationEvent::Prepared { operation_id: embedded, .. } = event
                && embedded != operation_id
            {
                return Err(RunError::InvalidEvent("tool operation id mismatch".into()));
            }
            let operation = state.tool_operations.entry(operation_id.clone()).or_default();
            operation.apply(event.clone())?;
        }
        RunEvent::ApprovalRequested { request, .. } => {
            require(state, RunStatus::Running)?;
            if state.approval.is_some() {
                return Err(RunError::InvalidEvent("approval already pending".into()));
            }
            state.approval = Some(request.clone());
            state.status = RunStatus::ApprovalRequired;
        }
        RunEvent::ApprovalResolved { decision, .. } => {
            require(state, RunStatus::ApprovalRequired)?;
            let operation_id = state.approval.as_ref().map(|request| request.operation_id.clone());
            state.approval = None;
            state.status = match decision {
                super::types::ApprovalDecision::Approved => {
                    if let Some(operation_id) = operation_id {
                        state.approved_operations.insert(operation_id);
                    }
                    RunStatus::Running
                }
                super::types::ApprovalDecision::Denied => RunStatus::Rejected,
            };
        }
        RunEvent::InputRequired { request, .. } => {
            require(state, RunStatus::Running)?;
            if state.input_request.is_some() {
                return Err(RunError::InvalidEvent("input already pending".into()));
            }
            state.input_request = Some(request.clone());
            state.status = RunStatus::InputRequired;
        }
        RunEvent::InputBound { parts, .. } => {
            require(state, RunStatus::InputRequired)?;
            state.bound_inputs.extend(parts.clone());
            state.input_request = None;
            state.status = RunStatus::Running;
        }
        RunEvent::ArtifactCommitted { artifact, .. } => {
            require(state, RunStatus::Running)?;
            if state.artifacts.iter().any(|item| item.artifact_id == artifact.artifact_id) {
                return Err(RunError::InvalidEvent(format!("duplicate artifact {}", artifact.artifact_id)));
            }
            state.artifacts.push(artifact.clone());
        }
        RunEvent::Completed { result, usage, .. } => {
            require(state, RunStatus::Running)?;
            if state.tool_operations.values().any(|operation| {
                operation
                    .attempt
                    .as_ref()
                    .is_some_and(|attempt| !matches!(attempt.phase, AttemptPhase::Settled | AttemptPhase::CanceledBeforeStart))
            }) {
                return Err(RunError::InvalidEvent("completion has unsettled tool operation".into()));
            }
            state.status = RunStatus::Completed;
            state.result = result.clone();
            state.usage = usage.clone();
        }
        RunEvent::Failed { code, message, usage, .. } => {
            require(state, RunStatus::Running)?;
            state.status = RunStatus::Failed;
            state.error_code = Some(code.clone());
            state.error_message = Some(message.clone());
            state.usage = usage.clone();
        }
        RunEvent::Canceled { reason, usage, .. } => {
            if !matches!(state.status, RunStatus::Queued | RunStatus::Running | RunStatus::ApprovalRequired | RunStatus::InputRequired) {
                return Err(RunError::InvalidEvent("run cannot be canceled from current status".into()));
            }
            state.status = RunStatus::Canceled;
            state.error_message = Some(reason.clone());
            state.usage = usage.clone();
            state.approval = None;
            state.input_request = None;
        }
        RunEvent::Rejected { code, message, .. } => {
            if !matches!(state.status, RunStatus::Queued | RunStatus::Running) {
                return Err(RunError::InvalidEvent("run cannot be rejected from current status".into()));
            }
            state.status = RunStatus::Rejected;
            state.error_code = Some(code.clone());
            state.error_message = Some(message.clone());
        }
        RunEvent::Blocked { reason, .. } => {
            state.status = RunStatus::Blocked;
            state.error_message = Some(reason.clone());
            state.approval = None;
            state.input_request = None;
        }
    }
    state.event_version = state.event_version.checked_add(1).ok_or_else(|| RunError::InvalidEvent("event version overflow".into()))?;
    state.updated_at_ms = event.at_ms();
    Ok(())
}

fn require(state: &BotRunState, expected: RunStatus) -> Result<(), RunError> {
    if state.status == expected { Ok(()) } else { Err(RunError::InvalidEvent(format!("expected {expected:?}, actual {:?}", state.status))) }
}

fn set_status(state: &mut BotRunState, allowed: &[RunStatus], target: RunStatus) -> Result<(), RunError> {
    if allowed.contains(&state.status) {
        state.status = target;
        Ok(())
    } else {
        Err(RunError::InvalidEvent(format!("{:?} -> {target:?}", state.status)))
    }
}
