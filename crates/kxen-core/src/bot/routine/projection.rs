use super::RoutineError;
use super::events::RoutineEvent;
use super::types::{OccurrenceStatus, RoutineLifecycle, RoutineState};

pub fn apply(state: &mut Option<RoutineState>, event: &RoutineEvent) -> Result<(), RoutineError> {
    match event {
        RoutineEvent::Created { routine_id, definition, next_scheduled_at_ms, at_ms } => {
            if state.is_some() {
                return Err(RoutineError::InvalidEvent("routine_created must be first".into()));
            }
            *state = Some(RoutineState {
                routine_id: routine_id.clone(),
                definition: definition.clone(),
                lifecycle: RoutineLifecycle::Active,
                event_version: 1,
                last_observed_at_ms: *at_ms,
                next_scheduled_at_ms: *next_scheduled_at_ms,
                occurrences: Default::default(),
                consecutive_failures: 0,
                blocked_reason: None,
                created_at_ms: *at_ms,
                updated_at_ms: *at_ms,
            });
            return Ok(());
        }
        _ if state.is_none() => return Err(RoutineError::InvalidEvent("event precedes routine_created".into())),
        _ => {}
    }
    let state = state.as_mut().expect("checked above");
    if matches!(state.lifecycle, RoutineLifecycle::Trashed | RoutineLifecycle::Blocked) {
        return Err(RoutineError::InvalidEvent(format!("event after {:?}", state.lifecycle)));
    }
    match event {
        RoutineEvent::Created { .. } => unreachable!(),
        RoutineEvent::Updated { definition, next_scheduled_at_ms, at_ms } => {
            state.definition = definition.clone();
            state.next_scheduled_at_ms = *next_scheduled_at_ms;
            state.last_observed_at_ms = *at_ms;
            state.consecutive_failures = 0;
        }
        RoutineEvent::Paused { .. } => {
            if state.lifecycle != RoutineLifecycle::Active {
                return Err(RoutineError::InvalidEvent("only active Routine can pause".into()));
            }
            state.lifecycle = RoutineLifecycle::Paused;
        }
        RoutineEvent::Resumed { next_scheduled_at_ms, at_ms } => {
            if state.lifecycle != RoutineLifecycle::Paused {
                return Err(RoutineError::InvalidEvent("only paused Routine can resume".into()));
            }
            state.lifecycle = RoutineLifecycle::Active;
            state.next_scheduled_at_ms = *next_scheduled_at_ms;
            state.last_observed_at_ms = *at_ms;
            state.consecutive_failures = 0;
        }
        RoutineEvent::OccurrenceRecorded { occurrence, next_scheduled_at_ms, .. }
        | RoutineEvent::OccurrenceSkipped { occurrence, next_scheduled_at_ms, .. } => {
            if state.lifecycle != RoutineLifecycle::Active
                || state.occurrences.insert(occurrence.occurrence_id.clone(), occurrence.clone()).is_some()
            {
                return Err(RoutineError::InvalidEvent("occurrence duplicated or Routine inactive".into()));
            }
            if !occurrence.manual {
                state.last_observed_at_ms = occurrence.observed_at_ms;
                state.next_scheduled_at_ms = *next_scheduled_at_ms;
            }
        }
        RoutineEvent::RunLinked { occurrence_id, run_id, .. } => {
            let occurrence = get_occurrence(state, occurrence_id)?;
            if occurrence.status != OccurrenceStatus::Recorded || occurrence.run_id.is_some() {
                return Err(RoutineError::InvalidEvent("occurrence cannot link Run".into()));
            }
            occurrence.status = OccurrenceStatus::RunLinked;
            occurrence.run_id = Some(run_id.clone());
        }
        RoutineEvent::OccurrenceCompleted { occurrence_id, .. } => {
            let occurrence = get_occurrence(state, occurrence_id)?;
            if occurrence.status != OccurrenceStatus::RunLinked {
                return Err(RoutineError::InvalidEvent("occurrence is not linked".into()));
            }
            occurrence.status = OccurrenceStatus::Completed;
            state.consecutive_failures = 0;
        }
        RoutineEvent::OccurrenceFailed { occurrence_id, error, .. } => {
            let occurrence = get_occurrence(state, occurrence_id)?;
            if occurrence.status != OccurrenceStatus::RunLinked {
                return Err(RoutineError::InvalidEvent("occurrence is not linked".into()));
            }
            occurrence.status = OccurrenceStatus::Failed;
            occurrence.error = Some(error.clone());
            state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        }
        RoutineEvent::Trashed { .. } => state.lifecycle = RoutineLifecycle::Trashed,
        RoutineEvent::Blocked { reason, .. } => {
            state.lifecycle = RoutineLifecycle::Blocked;
            state.blocked_reason = Some(reason.clone());
        }
    }
    state.event_version = state.event_version.checked_add(1).ok_or_else(|| RoutineError::InvalidEvent("event version overflow".into()))?;
    state.updated_at_ms = event.at_ms();
    Ok(())
}

fn get_occurrence<'a>(
    state: &'a mut RoutineState,
    id: &crate::core::identity::ResourceId,
) -> Result<&'a mut super::types::RoutineOccurrence, RoutineError> {
    state.occurrences.get_mut(id).ok_or_else(|| RoutineError::InvalidEvent("occurrence missing".into()))
}
