use crate::core::operation::{OperationCommand, OperationProjection};

use super::RunError;
use super::command::RunCommand;
use super::events::RunEvent;
use super::types::{BotRunState, RunStatus};

pub fn decide(state: Option<&BotRunState>, command: RunCommand) -> Result<Vec<RunEvent>, RunError> {
    match command {
        RunCommand::Queue { spec, at_ms } => {
            if state.is_some() {
                return Err(RunError::Transition("run already queued".into()));
            }
            if spec.input.is_empty() {
                return Err(RunError::Transition("run input cannot be empty".into()));
            }
            Ok(vec![RunEvent::Queued { spec, at_ms }])
        }
        command => decide_existing(state.ok_or_else(|| RunError::NotFound("uninitialized".into()))?, command),
    }
}

fn decide_existing(state: &BotRunState, command: RunCommand) -> Result<Vec<RunEvent>, RunError> {
    if state.status.is_terminal() {
        return Err(RunError::Transition(format!("run is terminal: {:?}", state.status)));
    }
    let event = match command {
        RunCommand::Queue { .. } => unreachable!(),
        RunCommand::Start { at_ms } => RunEvent::Started { at_ms },
        RunCommand::RecordTurn { record, at_ms } => RunEvent::TurnRecorded { record, at_ms },
        RunCommand::PrepareTool { operation_id, generation, intent, at_ms } => {
            return operation_events(
                state,
                operation_id.clone(),
                OperationCommand::Prepare { operation_id, generation, intent, at_ms },
                at_ms,
            );
        }
        RunCommand::MarkToolStarted { operation_id, generation, at_ms } => {
            return operation_events(state, operation_id, OperationCommand::MarkStarted { generation, at_ms }, at_ms);
        }
        RunCommand::RecordToolOutcome { operation_id, generation, outcome, evidence, at_ms } => {
            return operation_events(state, operation_id, OperationCommand::RecordOutcome { generation, outcome, evidence }, at_ms);
        }
        RunCommand::MarkToolUnknown { operation_id, generation, reason, evidence, at_ms } => {
            let operation_id_for_event = operation_id.clone();
            let mut events = operation_events(
                state,
                operation_id,
                OperationCommand::MarkOutcomeUnknown { generation, reason: reason.clone(), evidence },
                at_ms,
            )?;
            if events.is_empty() {
                return Ok(events);
            }
            events.push(RunEvent::Blocked { reason: format!("tool outcome unknown: {operation_id_for_event}: {reason}"), at_ms });
            return Ok(events);
        }
        RunCommand::SettleTool { operation_id, generation, at_ms } => {
            return operation_events(state, operation_id, OperationCommand::Settle { generation, at_ms }, at_ms);
        }
        RunCommand::RequestApproval { request, at_ms } => {
            if request.summary.trim().is_empty() {
                return Err(RunError::Transition("approval summary cannot be empty".into()));
            }
            RunEvent::ApprovalRequested { request, at_ms }
        }
        RunCommand::ResolveApproval { approval_id, decision, at_ms } => {
            if state.approval.as_ref().map(|request| &request.approval_id) != Some(&approval_id) {
                return Err(RunError::Transition("approval id is stale".into()));
            }
            RunEvent::ApprovalResolved { decision, at_ms }
        }
        RunCommand::RequireInput { request, at_ms } => {
            if request.prompt.trim().is_empty() {
                return Err(RunError::Transition("input prompt cannot be empty".into()));
            }
            RunEvent::InputRequired { request, at_ms }
        }
        RunCommand::BindInput { request_id, parts, at_ms } => {
            if state.input_request.as_ref().map(|request| &request.request_id) != Some(&request_id) || parts.is_empty() {
                return Err(RunError::Transition("input request is stale or response is empty".into()));
            }
            RunEvent::InputBound { parts, at_ms }
        }
        RunCommand::CommitArtifact { artifact, at_ms } => RunEvent::ArtifactCommitted { artifact, at_ms },
        RunCommand::Complete { result, usage, at_ms } => {
            if result.is_empty() {
                return Err(RunError::Transition("completed run requires a result".into()));
            }
            RunEvent::Completed { result, usage, at_ms }
        }
        RunCommand::Fail { code, message, usage, at_ms } => {
            require_error(&code, &message)?;
            reject_uncertain_terminal(state)?;
            RunEvent::Failed { code, message, usage, at_ms }
        }
        RunCommand::Cancel { reason, usage, at_ms } => {
            if reason.trim().is_empty() {
                return Err(RunError::Transition("cancel reason cannot be empty".into()));
            }
            RunEvent::Canceled { reason, usage, at_ms }
        }
        RunCommand::Reject { code, message, at_ms } => {
            require_error(&code, &message)?;
            RunEvent::Rejected { code, message, at_ms }
        }
        RunCommand::Block { reason, at_ms } => {
            if reason.trim().is_empty() {
                return Err(RunError::Transition("blocked reason cannot be empty".into()));
            }
            RunEvent::Blocked { reason, at_ms }
        }
    };
    Ok(vec![event])
}

fn operation_events(
    state: &BotRunState,
    operation_id: crate::core::identity::ResourceId,
    command: OperationCommand<super::types::ToolIntent, super::types::ToolExecutionResult>,
    at_ms: u64,
) -> Result<Vec<RunEvent>, RunError> {
    let finishing_paused = matches!(state.status, RunStatus::ApprovalRequired | RunStatus::InputRequired)
        && matches!(&command, OperationCommand::RecordOutcome { .. } | OperationCommand::Settle { .. });
    if state.status != RunStatus::Running && !finishing_paused {
        return Err(RunError::Transition(format!("tool operation from {:?}", state.status)));
    }
    let empty = OperationProjection::default();
    let operation = state.tool_operations.get(&operation_id).unwrap_or(&empty);
    let decision = operation.decide(command)?;
    Ok(decision.events.into_iter().map(|event| RunEvent::ToolOperation { operation_id: operation_id.clone(), event, at_ms }).collect())
}

fn require_error(code: &str, message: &str) -> Result<(), RunError> {
    if code.trim().is_empty() || message.trim().is_empty() {
        Err(RunError::Transition("terminal error requires code and message".into()))
    } else {
        Ok(())
    }
}

fn reject_uncertain_terminal(state: &BotRunState) -> Result<(), RunError> {
    let unsettled = state.tool_operations.values().any(|operation| {
        operation.attempt.as_ref().is_some_and(|attempt| {
            matches!(
                attempt.phase,
                crate::core::operation::AttemptPhase::Started
                    | crate::core::operation::AttemptPhase::OutcomeKnown
                    | crate::core::operation::AttemptPhase::OutcomeUnknown
            )
        })
    });
    if unsettled { Err(RunError::Transition("run with an unsettled tool operation cannot fail terminally".into())) } else { Ok(()) }
}
