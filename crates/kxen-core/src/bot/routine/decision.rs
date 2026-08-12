use crate::core::identity::{ActorRef, SystemActor};
use crate::core::scheduler::OccurrenceDecision;

use super::RoutineError;
use super::command::RoutineCommand;
use super::events::RoutineEvent;
use super::types::{OccurrenceStatus, RevisionPolicy, RoutineLifecycle, RoutineOccurrence, RoutineState};

pub fn decide(state: Option<&RoutineState>, actor: &ActorRef, command: RoutineCommand) -> Result<Vec<RoutineEvent>, RoutineError> {
    match command {
        RoutineCommand::Create { routine_id, definition, at_ms } => {
            require_owner(actor)?;
            if state.is_some() {
                return Err(RoutineError::Rejected("Routine already exists".into()));
            }
            definition.validate().map_err(RoutineError::Rejected)?;
            let next_scheduled_at_ms = definition.schedule.next_after(at_ms).map_err(|error| RoutineError::Rejected(error.to_string()))?;
            Ok(vec![RoutineEvent::Created { routine_id, definition, next_scheduled_at_ms, at_ms }])
        }
        command => decide_existing(state.ok_or_else(|| RoutineError::NotFound("uninitialized".into()))?, actor, command),
    }
}

fn decide_existing(state: &RoutineState, actor: &ActorRef, command: RoutineCommand) -> Result<Vec<RoutineEvent>, RoutineError> {
    if matches!(state.lifecycle, RoutineLifecycle::Trashed | RoutineLifecycle::Blocked) {
        return Err(RoutineError::Rejected(format!("Routine is {:?}", state.lifecycle)));
    }
    match command {
        RoutineCommand::Create { .. } => unreachable!(),
        RoutineCommand::Update { definition, at_ms } => {
            require_owner(actor)?;
            definition.validate().map_err(RoutineError::Rejected)?;
            let next_scheduled_at_ms = definition.schedule.next_after(at_ms).map_err(|error| RoutineError::Rejected(error.to_string()))?;
            Ok(vec![RoutineEvent::Updated { definition, next_scheduled_at_ms, at_ms }])
        }
        RoutineCommand::Tick { observed_at_ms, resolved_revision_id } => {
            require_system(actor, SystemActor::Scheduler)?;
            if state.lifecycle != RoutineLifecycle::Active {
                return Ok(Vec::new());
            }
            let plan = state
                .definition
                .schedule
                .plan(&state.routine_id, state.last_observed_at_ms, observed_at_ms)
                .map_err(|error| RoutineError::Rejected(error.to_string()))?;
            let Some(plan) = plan else { return Ok(Vec::new()) };
            if state.occurrences.contains_key(&plan.occurrence_id) {
                return Ok(Vec::new());
            }
            let next_scheduled_at_ms =
                state.definition.schedule.next_after(observed_at_ms).map_err(|error| RoutineError::Rejected(error.to_string()))?;
            let revision = match plan.decision {
                OccurrenceDecision::Run => Some(resolve_revision(state, resolved_revision_id)?),
                OccurrenceDecision::Skip => None,
            };
            let occurrence = RoutineOccurrence {
                occurrence_id: plan.occurrence_id,
                scheduled_at_ms: plan.scheduled_at_ms,
                observed_at_ms: plan.observed_at_ms,
                missed_before: plan.missed_before,
                manual: false,
                status: if plan.decision == OccurrenceDecision::Run { OccurrenceStatus::Recorded } else { OccurrenceStatus::Skipped },
                resolved_revision_id: revision,
                run_id: None,
                error: None,
            };
            Ok(vec![if plan.decision == OccurrenceDecision::Run {
                RoutineEvent::OccurrenceRecorded { occurrence, next_scheduled_at_ms, at_ms: observed_at_ms }
            } else {
                RoutineEvent::OccurrenceSkipped { occurrence, next_scheduled_at_ms, at_ms: observed_at_ms }
            }])
        }
        RoutineCommand::RunNow { occurrence_id, resolved_revision_id, at_ms } => {
            require_owner(actor)?;
            if state.lifecycle != RoutineLifecycle::Active || state.occurrences.contains_key(&occurrence_id) {
                return Err(RoutineError::Rejected("manual occurrence is duplicated or Routine inactive".into()));
            }
            let revision = resolve_revision(state, Some(resolved_revision_id))?;
            Ok(vec![RoutineEvent::OccurrenceRecorded {
                occurrence: RoutineOccurrence {
                    occurrence_id,
                    scheduled_at_ms: at_ms,
                    observed_at_ms: at_ms,
                    missed_before: 0,
                    manual: true,
                    status: OccurrenceStatus::Recorded,
                    resolved_revision_id: Some(revision),
                    run_id: None,
                    error: None,
                },
                next_scheduled_at_ms: state.next_scheduled_at_ms,
                at_ms,
            }])
        }
        RoutineCommand::LinkRun { occurrence_id, run_id, at_ms } => {
            require_system(actor, SystemActor::Runtime)?;
            let occurrence = occurrence(state, &occurrence_id)?;
            if occurrence.status != OccurrenceStatus::Recorded || occurrence.run_id.is_some() {
                return Err(RoutineError::Rejected("occurrence cannot link Run".into()));
            }
            Ok(vec![RoutineEvent::RunLinked { occurrence_id, run_id, at_ms }])
        }
        RoutineCommand::RecordResult { occurrence_id, error, at_ms } => {
            require_system(actor, SystemActor::Runtime)?;
            let occurrence = occurrence(state, &occurrence_id)?;
            if occurrence.status != OccurrenceStatus::RunLinked {
                return Err(RoutineError::Rejected("occurrence Run is not active".into()));
            }
            match error {
                None => Ok(vec![RoutineEvent::OccurrenceCompleted { occurrence_id, at_ms }]),
                Some(error) if error.trim().is_empty() => Err(RoutineError::Rejected("failure error cannot be empty".into())),
                Some(error) => {
                    let mut events = vec![RoutineEvent::OccurrenceFailed { occurrence_id, error, at_ms }];
                    if state.consecutive_failures.saturating_add(1) >= state.definition.failure_threshold {
                        events.push(RoutineEvent::Paused { reason: "consecutive failure threshold reached".into(), at_ms });
                    }
                    Ok(events)
                }
            }
        }
        RoutineCommand::Pause { reason, at_ms } => {
            require_owner_or_runtime(actor)?;
            if reason.trim().is_empty() || state.lifecycle != RoutineLifecycle::Active {
                return Err(RoutineError::Rejected("Routine cannot be paused".into()));
            }
            Ok(vec![RoutineEvent::Paused { reason, at_ms }])
        }
        RoutineCommand::Resume { at_ms } => {
            require_owner(actor)?;
            if state.lifecycle != RoutineLifecycle::Paused {
                return Err(RoutineError::Rejected("Routine is not paused".into()));
            }
            let next_scheduled_at_ms =
                state.definition.schedule.next_after(at_ms).map_err(|error| RoutineError::Rejected(error.to_string()))?;
            Ok(vec![RoutineEvent::Resumed { next_scheduled_at_ms, at_ms }])
        }
        RoutineCommand::Trash { at_ms } => {
            require_owner(actor)?;
            Ok(vec![RoutineEvent::Trashed { at_ms }])
        }
        RoutineCommand::Block { reason, at_ms } => {
            if reason.trim().is_empty() {
                return Err(RoutineError::Rejected("blocked reason cannot be empty".into()));
            }
            Ok(vec![RoutineEvent::Blocked { reason, at_ms }])
        }
    }
}

fn resolve_revision(
    state: &RoutineState,
    supplied: Option<crate::core::identity::ResourceId>,
) -> Result<crate::core::identity::ResourceId, RoutineError> {
    match &state.definition.revision_policy {
        RevisionPolicy::FollowCurrent => supplied.ok_or_else(|| RoutineError::Rejected("current published revision is unavailable".into())),
        RevisionPolicy::Pinned { revision_id } if supplied.as_ref().is_none_or(|value| value == revision_id) => Ok(revision_id.clone()),
        RevisionPolicy::Pinned { .. } => Err(RoutineError::Rejected("resolved revision does not match pinned revision".into())),
    }
}

fn occurrence<'a>(state: &'a RoutineState, id: &crate::core::identity::ResourceId) -> Result<&'a RoutineOccurrence, RoutineError> {
    state.occurrences.get(id).ok_or_else(|| RoutineError::NotFound(id.to_string()))
}

fn require_owner(actor: &ActorRef) -> Result<(), RoutineError> {
    if actor == &ActorRef::Owner { Ok(()) } else { Err(RoutineError::Rejected("owner action required".into())) }
}

fn require_owner_or_runtime(actor: &ActorRef) -> Result<(), RoutineError> {
    if actor == &ActorRef::Owner || actor == &(ActorRef::System { actor: SystemActor::Runtime }) {
        Ok(())
    } else {
        Err(RoutineError::Rejected("owner or runtime action required".into()))
    }
}

fn require_system(actor: &ActorRef, expected: SystemActor) -> Result<(), RoutineError> {
    if actor == &(ActorRef::System { actor: expected }) { Ok(()) } else { Err(RoutineError::Rejected("system actor mismatch".into())) }
}
