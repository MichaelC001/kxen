use crate::core::delivery::DeliveryStatus;

use super::ConversationError;
use super::events::ConversationEvent;
use super::types::{ConversationLifecycle, ConversationState, TaskStatus};

pub fn apply(state: &mut Option<ConversationState>, event: &ConversationEvent) -> Result<(), ConversationError> {
    match event {
        ConversationEvent::Created { conversation_id, kind, members, moderator_bot_id, at_ms } => {
            if state.is_some() {
                return Err(ConversationError::InvalidEvent("conversation_created must be first".into()));
            }
            let members = members.iter().cloned().map(|member| (member.bot_id.clone(), member)).collect();
            *state = Some(ConversationState {
                conversation_id: conversation_id.clone(),
                kind: *kind,
                lifecycle: ConversationLifecycle::Active,
                event_version: 1,
                members,
                moderator_bot_id: moderator_bot_id.clone(),
                blocked_reason: None,
                messages: Vec::new(),
                message_sequences: Default::default(),
                deliveries: crate::core::delivery::DeliveryProjection::new(1024),
                delivery_runs: Default::default(),
                tasks: Default::default(),
                created_at_ms: *at_ms,
                updated_at_ms: *at_ms,
            });
            return Ok(());
        }
        _ if state.is_none() => return Err(ConversationError::InvalidEvent("event precedes conversation_created".into())),
        _ => {}
    }
    let state = state.as_mut().expect("checked above");
    if state.lifecycle == ConversationLifecycle::Archived && !matches!(event, ConversationEvent::Reopened { .. }) {
        return Err(ConversationError::InvalidEvent(format!("event after {:?}", state.lifecycle)));
    }
    if state.lifecycle == ConversationLifecycle::Blocked {
        return Err(ConversationError::InvalidEvent(format!("event after {:?}", state.lifecycle)));
    }
    match event {
        ConversationEvent::Created { .. } => unreachable!(),
        ConversationEvent::ParticipantAdded { participant, .. } => {
            if state.members.get(&participant.bot_id).is_some_and(|member| member.active) {
                return Err(ConversationError::InvalidEvent("participant already active".into()));
            }
            state.members.insert(participant.bot_id.clone(), participant.clone());
        }
        ConversationEvent::ParticipantRemoved { bot_id, .. } => {
            let participant = state.members.get_mut(bot_id).ok_or_else(|| ConversationError::InvalidEvent("participant missing".into()))?;
            if !participant.active {
                return Err(ConversationError::InvalidEvent("participant already removed".into()));
            }
            participant.active = false;
        }
        ConversationEvent::ModeratorChanged { bot_id, .. } => {
            if !state.members.get(bot_id).is_some_and(|member| member.active) {
                return Err(ConversationError::InvalidEvent("moderator is not active member".into()));
            }
            state.moderator_bot_id = Some(bot_id.clone());
        }
        ConversationEvent::MessageAppended { message, .. } => {
            if message.conversation_id != state.conversation_id || state.messages.iter().any(|item| item.message_id == message.message_id) {
                return Err(ConversationError::InvalidEvent("message identity mismatch or duplicate".into()));
            }
            let sequence =
                state.event_version.checked_add(1).ok_or_else(|| ConversationError::InvalidEvent("event version overflow".into()))?;
            state.message_sequences.insert(message.message_id.clone(), sequence);
            state.messages.push(message.clone());
        }
        ConversationEvent::Delivery { event, .. } => state.deliveries.apply(event.clone())?,
        ConversationEvent::DeliveryRunLinked { delivery_id, run_id, .. } => {
            let record = state
                .deliveries
                .records
                .get(delivery_id)
                .ok_or_else(|| ConversationError::InvalidEvent("linked Delivery missing".into()))?;
            if record.status != DeliveryStatus::InFlight || state.delivery_runs.insert(delivery_id.clone(), run_id.clone()).is_some() {
                return Err(ConversationError::InvalidEvent("Delivery link requires unique in-flight Delivery".into()));
            }
        }
        ConversationEvent::TaskCreated { task, .. } => {
            if task.conversation_id != state.conversation_id || state.tasks.insert(task.task_id.clone(), task.clone()).is_some() {
                return Err(ConversationError::InvalidEvent("task identity mismatch or duplicate".into()));
            }
        }
        ConversationEvent::TaskStatusChanged { task_id, status, result, at_ms } => {
            let task = state.tasks.get_mut(task_id).ok_or_else(|| ConversationError::InvalidEvent("task missing".into()))?;
            task.status = *status;
            task.result = result.clone();
            task.updated_at_ms = *at_ms;
        }
        ConversationEvent::TaskReassigned { task_id, owner_bot_id, at_ms } => {
            let task = state.tasks.get_mut(task_id).ok_or_else(|| ConversationError::InvalidEvent("task missing".into()))?;
            if task.status.is_terminal() {
                return Err(ConversationError::InvalidEvent("terminal task cannot be reassigned".into()));
            }
            task.owner_bot_id = owner_bot_id.clone();
            task.status = TaskStatus::Submitted;
            task.updated_at_ms = *at_ms;
        }
        ConversationEvent::Paused { .. } => set_lifecycle(state, ConversationLifecycle::Active, ConversationLifecycle::Paused)?,
        ConversationEvent::Resumed { .. } => set_lifecycle(state, ConversationLifecycle::Paused, ConversationLifecycle::Active)?,
        ConversationEvent::Archived { .. } => state.lifecycle = ConversationLifecycle::Archived,
        ConversationEvent::Reopened { .. } => {
            if state.kind != crate::bot::conversation::ConversationKind::BotDirect {
                return Err(ConversationError::InvalidEvent("only Bot Direct Conversation can be reopened".into()));
            }
            set_lifecycle(state, ConversationLifecycle::Archived, ConversationLifecycle::Active)?;
            state.blocked_reason = None;
        }
        ConversationEvent::Blocked { reason, .. } => {
            state.lifecycle = ConversationLifecycle::Blocked;
            state.blocked_reason = Some(reason.clone());
        }
    }
    state.event_version =
        state.event_version.checked_add(1).ok_or_else(|| ConversationError::InvalidEvent("event version overflow".into()))?;
    state.updated_at_ms = event.at_ms();
    Ok(())
}

fn set_lifecycle(
    state: &mut ConversationState,
    expected: ConversationLifecycle,
    target: ConversationLifecycle,
) -> Result<(), ConversationError> {
    if state.lifecycle != expected {
        return Err(ConversationError::InvalidEvent(format!("{:?} -> {target:?}", state.lifecycle)));
    }
    state.lifecycle = target;
    Ok(())
}
