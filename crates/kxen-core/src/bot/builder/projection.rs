use super::BuilderError;
use super::events::BuilderEvent;
use super::types::{BuilderLifecycle, BuilderState};

pub fn apply(state: &mut Option<BuilderState>, event: &BuilderEvent) -> Result<(), BuilderError> {
    match event {
        BuilderEvent::Started { builder_session_id, bot_id, user_goal, at_ms } => {
            if state.is_some() {
                return Err(BuilderError::InvalidEvent("builder_started must be first".into()));
            }
            *state = Some(BuilderState {
                builder_session_id: builder_session_id.clone(),
                bot_id: bot_id.clone(),
                lifecycle: BuilderLifecycle::Active,
                event_version: 1,
                user_goal: user_goal.clone(),
                messages: Vec::new(),
                draft: None,
                grants: Vec::new(),
                reports: Vec::new(),
                tests: Vec::new(),
                active_test_run_id: None,
                blocked_reason: None,
                created_at_ms: *at_ms,
                updated_at_ms: *at_ms,
            });
            return Ok(());
        }
        _ if state.is_none() => return Err(BuilderError::InvalidEvent("event precedes builder_started".into())),
        _ => {}
    }
    let state = state.as_mut().expect("checked above");
    if state.lifecycle != BuilderLifecycle::Active {
        return Err(BuilderError::InvalidEvent(format!("event after {:?}", state.lifecycle)));
    }
    match event {
        BuilderEvent::Started { .. } => unreachable!(),
        BuilderEvent::MessageAppended { message, .. } => {
            if state.messages.iter().any(|item| item.message_id == message.message_id) {
                return Err(BuilderError::InvalidEvent("duplicate Builder message".into()));
            }
            state.messages.push(message.clone());
        }
        BuilderEvent::DraftReplaced { draft, .. } => {
            let expected = state.draft.as_ref().map_or(1, |current| current.version + 1);
            if draft.version != expected {
                return Err(BuilderError::InvalidEvent("Builder draft version gap".into()));
            }
            state.draft = Some(draft.as_ref().clone());
            state.active_test_run_id = None;
        }
        BuilderEvent::PermissionGranted { grant, .. } => state.grants.push(grant.clone()),
        BuilderEvent::ValidationRecorded { report, .. } => state.reports.push(report.clone()),
        BuilderEvent::TestRunLinked { run_id, .. } => state.active_test_run_id = Some(run_id.clone()),
        BuilderEvent::TestEvidenceRecorded { evidence, .. } => {
            if state.active_test_run_id.as_ref() != Some(&evidence.run_id) {
                return Err(BuilderError::InvalidEvent("test evidence Run mismatch".into()));
            }
            state.tests.push(evidence.clone());
            state.active_test_run_id = None;
        }
        BuilderEvent::Canceled { .. } => state.lifecycle = BuilderLifecycle::Canceled,
        BuilderEvent::Blocked { reason, .. } => {
            state.lifecycle = BuilderLifecycle::Blocked;
            state.blocked_reason = Some(reason.clone());
        }
    }
    state.event_version = state.event_version.checked_add(1).ok_or_else(|| BuilderError::InvalidEvent("event version overflow".into()))?;
    state.updated_at_ms = event.at_ms();
    Ok(())
}
