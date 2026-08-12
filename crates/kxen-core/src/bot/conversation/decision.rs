use crate::core::delivery::{ClaimMode, DeliveryCommand, DeliveryEnvelope, QueuePosition};
use crate::core::identity::{ActorRef, ResourceId};

use super::ConversationError;
use super::command::ConversationCommand;
use super::events::ConversationEvent;
use super::routing;
use super::types::{CollaborationTask, ConversationKind, ConversationLifecycle, ConversationState, MessageDelivery, TaskStatus};

pub fn decide(
    state: Option<&ConversationState>,
    actor: &ActorRef,
    command: ConversationCommand,
) -> Result<Vec<ConversationEvent>, ConversationError> {
    match command {
        ConversationCommand::Create { conversation_id, kind, members, moderator_bot_id, at_ms } => {
            if state.is_some() || actor != &ActorRef::Owner {
                return Err(ConversationError::Rejected("only owner can create a new conversation".into()));
            }
            validate_members(kind, &members, moderator_bot_id.as_ref())?;
            Ok(vec![ConversationEvent::Created { conversation_id, kind, members, moderator_bot_id, at_ms }])
        }
        command => decide_existing(state.ok_or_else(|| ConversationError::NotFound("uninitialized".into()))?, actor, command),
    }
}

fn decide_existing(
    state: &ConversationState,
    actor: &ActorRef,
    command: ConversationCommand,
) -> Result<Vec<ConversationEvent>, ConversationError> {
    if state.lifecycle == ConversationLifecycle::Blocked {
        return Err(ConversationError::Rejected("conversation is blocked".into()));
    }
    match command {
        ConversationCommand::Create { .. } => unreachable!(),
        ConversationCommand::Post { message, task, at_ms } => post(state, actor, *message, task, at_ms),
        ConversationCommand::ClaimDelivery { generation, at_ms } => {
            require_active(state)?;
            let decision = state.deliveries.decide(DeliveryCommand::Claim { mode: ClaimMode::One, generation })?;
            Ok(decision.events.into_iter().map(|event| ConversationEvent::Delivery { event, at_ms }).collect())
        }
        ConversationCommand::AcknowledgeDelivery { token, run_ids, at_ms } => {
            if token.delivery_ids.iter().any(|id| !run_ids.contains_key(id)) || token.delivery_ids.len() != run_ids.len() {
                return Err(ConversationError::Rejected("every claimed Delivery requires exactly one durable Run".into()));
            }
            state.deliveries.decide(DeliveryCommand::Acknowledge { token: token.clone() })?;
            let mut events = token
                .delivery_ids
                .iter()
                .map(|id| ConversationEvent::DeliveryRunLinked { delivery_id: id.clone(), run_id: run_ids[id].clone(), at_ms })
                .collect::<Vec<_>>();
            events.push(ConversationEvent::Delivery { event: crate::core::delivery::DeliveryEvent::Acknowledged { token }, at_ms });
            Ok(events)
        }
        ConversationCommand::ReleaseDelivery { token, at_ms } => {
            let decision = state.deliveries.decide(DeliveryCommand::Release { token })?;
            Ok(decision.events.into_iter().map(|event| ConversationEvent::Delivery { event, at_ms }).collect())
        }
        ConversationCommand::RejectDelivery { delivery_id, generation, reason, at_ms } => {
            if reason.trim().is_empty() {
                return Err(ConversationError::Rejected("Delivery rejection requires a reason".into()));
            }
            let decision = state.deliveries.decide(DeliveryCommand::Reject { delivery_id, generation, reason })?;
            Ok(decision.events.into_iter().map(|event| ConversationEvent::Delivery { event, at_ms }).collect())
        }
        ConversationCommand::ChangeTask { task_id, status, result, at_ms } => change_task(state, actor, task_id, status, result, at_ms),
        ConversationCommand::ReassignTask { task_id, owner_bot_id, at_ms } => reassign(state, actor, task_id, owner_bot_id, at_ms),
        ConversationCommand::AddMember { participant, at_ms } => {
            require_owner_group(state, actor)?;
            if state.active_members().count() >= 6 || state.members.get(&participant.bot_id).is_some_and(|member| member.active) {
                return Err(ConversationError::Rejected("Group member limit or duplicate member".into()));
            }
            Ok(vec![ConversationEvent::ParticipantAdded { participant, at_ms }])
        }
        ConversationCommand::RemoveMember { bot_id, at_ms } => {
            require_owner_group(state, actor)?;
            if state.active_members().count() <= 2 || state.moderator_bot_id.as_ref() == Some(&bot_id) {
                return Err(ConversationError::Rejected("Group must keep two Bots and an active moderator".into()));
            }
            require_member(state, &bot_id)?;
            Ok(vec![ConversationEvent::ParticipantRemoved { bot_id, at_ms }])
        }
        ConversationCommand::SetModerator { bot_id, at_ms } => {
            require_owner_group(state, actor)?;
            require_member(state, &bot_id)?;
            Ok(vec![ConversationEvent::ModeratorChanged { bot_id, at_ms }])
        }
        ConversationCommand::Pause { at_ms } => {
            require_owner(actor)?;
            require_lifecycle(state, ConversationLifecycle::Active)?;
            Ok(vec![ConversationEvent::Paused { at_ms }])
        }
        ConversationCommand::Resume { at_ms } => {
            require_owner(actor)?;
            require_lifecycle(state, ConversationLifecycle::Paused)?;
            Ok(vec![ConversationEvent::Resumed { at_ms }])
        }
        ConversationCommand::Archive { at_ms } => {
            require_owner(actor)?;
            if !matches!(state.lifecycle, ConversationLifecycle::Active | ConversationLifecycle::Paused) {
                return Err(ConversationError::Rejected("conversation cannot be archived".into()));
            }
            Ok(vec![ConversationEvent::Archived { at_ms }])
        }
        ConversationCommand::Block { reason, at_ms } => {
            if reason.trim().is_empty() {
                return Err(ConversationError::Rejected("blocked reason cannot be empty".into()));
            }
            Ok(vec![ConversationEvent::Blocked { reason, at_ms }])
        }
    }
}

fn post(
    state: &ConversationState,
    actor: &ActorRef,
    message: super::types::Message,
    new_task: Option<super::types::NewTask>,
    at_ms: u64,
) -> Result<Vec<ConversationEvent>, ConversationError> {
    require_active(state)?;
    if &message.actor != actor {
        return Err(ConversationError::Rejected("message actor does not match command actor".into()));
    }
    if let Some(existing) = state.messages.iter().find(|item| item.message_id == message.message_id) {
        return if existing == &message {
            Ok(Vec::new())
        } else {
            Ok(vec![ConversationEvent::Blocked { reason: format!("message id collision: {}", message.message_id), at_ms }])
        };
    }
    validate_parts(&message.parts)?;
    let recipients = routing::recipients(state, &message)?;
    let mut events = Vec::new();
    if let Some(task) = new_task {
        if message.task_id.as_ref() != Some(&task.task_id) || state.tasks.contains_key(&task.task_id) {
            return Err(ConversationError::Rejected("new task identity is missing or duplicated".into()));
        }
        require_member(state, &task.owner_bot_id)?;
        if !recipients.contains(&task.owner_bot_id) || task.title.trim().is_empty() || task.expected_output.trim().is_empty() {
            return Err(ConversationError::Rejected("task owner, title or output contract is invalid".into()));
        }
        events.push(ConversationEvent::TaskCreated {
            task: CollaborationTask {
                task_id: task.task_id,
                conversation_id: state.conversation_id.clone(),
                originator: actor.clone(),
                owner_bot_id: task.owner_bot_id,
                title: task.title,
                input: task.input,
                expected_output: task.expected_output,
                status: TaskStatus::Submitted,
                result: Vec::new(),
                origin_run_id: message.origin_run_id.clone(),
                parent_task_id: task.parent_task_id,
                delegation_depth: message.delegation_depth,
                hop_count: message.hop_count,
                budget: task.budget,
                created_at_ms: at_ms,
                updated_at_ms: at_ms,
            },
            at_ms,
        });
    }
    events.push(ConversationEvent::MessageAppended { message: message.clone(), at_ms });
    for recipient in recipients {
        let delivery_id = crate::bot::ids::deterministic_id("bdel", &[message.message_id.as_str(), recipient.as_str()])
            .map_err(ConversationError::InvalidId)?;
        let envelope = DeliveryEnvelope::new(
            delivery_id,
            ActorRef::Bot { id: recipient },
            at_ms,
            MessageDelivery {
                message_id: message.message_id.clone(),
                task_id: message.task_id.clone(),
                delegation_depth: message.delegation_depth,
                hop_count: message.hop_count,
            },
        )?;
        let event = state
            .deliveries
            .decide(DeliveryCommand::Enqueue { envelope, position: QueuePosition::Back })?
            .events
            .into_iter()
            .next()
            .ok_or_else(|| ConversationError::Rejected("delivery unexpectedly duplicated".into()))?;
        events.push(ConversationEvent::Delivery { event, at_ms });
    }
    Ok(events)
}

fn change_task(
    state: &ConversationState,
    actor: &ActorRef,
    task_id: ResourceId,
    status: TaskStatus,
    result: Vec<super::types::MessagePart>,
    at_ms: u64,
) -> Result<Vec<ConversationEvent>, ConversationError> {
    let task = state.tasks.get(&task_id).ok_or_else(|| ConversationError::NotFound(task_id.to_string()))?;
    if task.status.is_terminal() || !task_transition(task.status, status) {
        return Err(ConversationError::Rejected(format!("task {:?} -> {status:?}", task.status)));
    }
    if actor != &ActorRef::Owner && actor != &(ActorRef::Bot { id: task.owner_bot_id.clone() }) {
        return Err(ConversationError::Rejected("only owner Bot or user owner can change task".into()));
    }
    if (status == TaskStatus::Completed) != !result.is_empty() {
        return Err(ConversationError::Rejected("only completed task requires non-empty result".into()));
    }
    if status == TaskStatus::Completed
        && state.tasks.values().any(|child| child.parent_task_id.as_ref() == Some(&task_id) && !child.status.is_terminal())
    {
        return Err(ConversationError::Rejected("parent task cannot complete before every child task is terminal".into()));
    }
    Ok(vec![ConversationEvent::TaskStatusChanged { task_id, status, result, at_ms }])
}

fn reassign(
    state: &ConversationState,
    actor: &ActorRef,
    task_id: ResourceId,
    owner_bot_id: ResourceId,
    at_ms: u64,
) -> Result<Vec<ConversationEvent>, ConversationError> {
    let task = state.tasks.get(&task_id).ok_or_else(|| ConversationError::NotFound(task_id.to_string()))?;
    if task.status.is_terminal() || (actor != &ActorRef::Owner && actor != &task.originator) {
        return Err(ConversationError::Rejected("task cannot be reassigned by this actor".into()));
    }
    require_member(state, &owner_bot_id)?;
    Ok(vec![ConversationEvent::TaskReassigned { task_id, owner_bot_id, at_ms }])
}

fn validate_members(
    kind: ConversationKind,
    members: &[super::types::BotParticipant],
    moderator: Option<&ResourceId>,
) -> Result<(), ConversationError> {
    let ids = members.iter().filter(|member| member.active).map(|member| &member.bot_id).collect::<std::collections::BTreeSet<_>>();
    if ids.len() != members.len() {
        return Err(ConversationError::Rejected("members must be unique and active".into()));
    }
    let valid = match kind {
        ConversationKind::HumanBot => ids.len() == 1 && moderator.is_none(),
        ConversationKind::BotDirect => ids.len() == 2 && moderator.is_none(),
        ConversationKind::BotGroup => (2..=6).contains(&ids.len()) && moderator.is_some_and(|id| ids.contains(id)),
    };
    if valid { Ok(()) } else { Err(ConversationError::Rejected("conversation membership shape is invalid".into())) }
}

fn validate_parts(parts: &[super::types::MessagePart]) -> Result<(), ConversationError> {
    if parts.is_empty() || parts.iter().any(|part| matches!(part, super::types::MessagePart::Text { text } if text.trim().is_empty())) {
        Err(ConversationError::Rejected("message contains no usable content".into()))
    } else {
        Ok(())
    }
}

fn task_transition(from: TaskStatus, to: TaskStatus) -> bool {
    match from {
        TaskStatus::Submitted => matches!(to, TaskStatus::Working | TaskStatus::Canceled | TaskStatus::Rejected | TaskStatus::Blocked),
        TaskStatus::Working => matches!(
            to,
            TaskStatus::Completed
                | TaskStatus::Failed
                | TaskStatus::Canceled
                | TaskStatus::Rejected
                | TaskStatus::InputRequired
                | TaskStatus::ApprovalRequired
                | TaskStatus::Blocked
        ),
        TaskStatus::InputRequired | TaskStatus::ApprovalRequired => {
            matches!(to, TaskStatus::Working | TaskStatus::Canceled | TaskStatus::Blocked)
        }
        _ => false,
    }
}

fn require_owner(actor: &ActorRef) -> Result<(), ConversationError> {
    if actor == &ActorRef::Owner { Ok(()) } else { Err(ConversationError::Rejected("owner action required".into())) }
}

fn require_owner_group(state: &ConversationState, actor: &ActorRef) -> Result<(), ConversationError> {
    require_owner(actor)?;
    if state.kind == ConversationKind::BotGroup { Ok(()) } else { Err(ConversationError::Rejected("Group action required".into())) }
}

fn require_active(state: &ConversationState) -> Result<(), ConversationError> {
    require_lifecycle(state, ConversationLifecycle::Active)
}

fn require_lifecycle(state: &ConversationState, lifecycle: ConversationLifecycle) -> Result<(), ConversationError> {
    if state.lifecycle == lifecycle { Ok(()) } else { Err(ConversationError::Rejected(format!("conversation is {:?}", state.lifecycle))) }
}

fn require_member(state: &ConversationState, bot_id: &ResourceId) -> Result<(), ConversationError> {
    if state.members.get(bot_id).is_some_and(|member| member.active) {
        Ok(())
    } else {
        Err(ConversationError::Rejected(format!("Bot is not active member: {bot_id}")))
    }
}
