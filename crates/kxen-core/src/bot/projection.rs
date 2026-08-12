use super::BotError;
use super::events::BotEvent;
use super::types::{BotLifecycle, BotState};

pub fn apply(state: &mut Option<BotState>, event: &BotEvent) -> Result<(), BotError> {
    match event {
        BotEvent::Created { bot_id, draft, at_ms } => {
            if state.is_some() {
                return Err(BotError::InvalidEvent("bot_created must be the first event".into()));
            }
            *state = Some(BotState {
                bot_id: bot_id.clone(),
                lifecycle: BotLifecycle::Draft,
                event_version: 1,
                draft_version_counter: draft.version,
                draft: Some(draft.clone()),
                current_revision_id: None,
                revisions: Default::default(),
                blocked_reason: None,
                created_at_ms: *at_ms,
                updated_at_ms: *at_ms,
            });
            return Ok(());
        }
        _ if state.is_none() => return Err(BotError::InvalidEvent("bot event precedes bot_created".into())),
        _ => {}
    }
    let state = state.as_mut().expect("checked above");
    match event {
        BotEvent::Created { .. } => unreachable!(),
        BotEvent::DraftReplaced { draft, .. } => {
            if draft.version <= state.draft_version_counter {
                return Err(BotError::InvalidEvent("draft version must increase".into()));
            }
            state.draft_version_counter = draft.version;
            state.draft = Some(draft.clone());
        }
        BotEvent::RevisionPublished { revision, .. } => {
            if state.draft.as_ref().is_none_or(|draft| draft.content_hash != revision.content_hash) {
                return Err(BotError::InvalidEvent("published revision does not match current draft".into()));
            }
            let expected = state.revisions.len() as u64 + 1;
            if revision.revision_number != expected || state.revisions.contains_key(&revision.revision_number) {
                return Err(BotError::InvalidEvent("revision number is not contiguous".into()));
            }
            state.current_revision_id = Some(revision.revision_id.clone());
            state.revisions.insert(revision.revision_number, revision.clone());
            state.draft = None;
            if state.lifecycle == BotLifecycle::Draft {
                state.lifecycle = BotLifecycle::Active;
            }
        }
        BotEvent::Paused { .. } => state.lifecycle = transition(state.lifecycle, &[BotLifecycle::Active], BotLifecycle::Paused)?,
        BotEvent::Resumed { .. } => state.lifecycle = transition(state.lifecycle, &[BotLifecycle::Paused], BotLifecycle::Active)?,
        BotEvent::Archived { .. } => {
            state.lifecycle = transition(state.lifecycle, &[BotLifecycle::Active, BotLifecycle::Paused], BotLifecycle::Archived)?;
        }
        BotEvent::Trashed { .. } => {
            state.lifecycle = transition(
                state.lifecycle,
                &[BotLifecycle::Draft, BotLifecycle::Active, BotLifecycle::Paused, BotLifecycle::Archived],
                BotLifecycle::Trashed,
            )?;
        }
        BotEvent::Restored { .. } => state.lifecycle = transition(state.lifecycle, &[BotLifecycle::Trashed], BotLifecycle::Paused)?,
        BotEvent::Blocked { reason, .. } => {
            state.lifecycle = transition(
                state.lifecycle,
                &[BotLifecycle::Draft, BotLifecycle::Active, BotLifecycle::Paused, BotLifecycle::Archived],
                BotLifecycle::Blocked,
            )?;
            state.blocked_reason = Some(reason.clone());
        }
        BotEvent::RecoveryCleared { .. } => {
            state.lifecycle = transition(state.lifecycle, &[BotLifecycle::Blocked], BotLifecycle::Paused)?;
            state.blocked_reason = None;
        }
    }
    state.event_version = state.event_version.checked_add(1).ok_or_else(|| BotError::InvalidEvent("event version overflow".into()))?;
    state.updated_at_ms = event.at_ms();
    Ok(())
}

fn transition(current: BotLifecycle, allowed: &[BotLifecycle], target: BotLifecycle) -> Result<BotLifecycle, BotError> {
    if allowed.contains(&current) { Ok(target) } else { Err(BotError::LifecycleRejected(format!("{current:?} -> {target:?}"))) }
}
