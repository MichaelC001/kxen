use crate::core::identity::{ActorRef, ResourceId};

use super::ConversationError;
use super::types::{BotParticipant, ConversationKind, ConversationLifecycle, ConversationState, MessagePart, TaskStatus};

pub(super) fn validate_members(
    kind: ConversationKind,
    members: &[BotParticipant],
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

pub(super) fn validate_parts(parts: &[MessagePart]) -> Result<(), ConversationError> {
    if parts.is_empty() || parts.iter().any(|part| matches!(part, MessagePart::Text { text } if text.trim().is_empty())) {
        Err(ConversationError::Rejected("message contains no usable content".into()))
    } else {
        Ok(())
    }
}

pub(super) fn task_transition(from: TaskStatus, to: TaskStatus) -> bool {
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
            matches!(to, TaskStatus::Working | TaskStatus::Canceled | TaskStatus::Rejected | TaskStatus::Blocked)
        }
        _ => false,
    }
}

pub(super) fn require_owner(actor: &ActorRef) -> Result<(), ConversationError> {
    if actor == &ActorRef::Owner { Ok(()) } else { Err(ConversationError::Rejected("owner action required".into())) }
}

pub(super) fn require_owner_group(state: &ConversationState, actor: &ActorRef) -> Result<(), ConversationError> {
    require_owner(actor)?;
    if state.kind == ConversationKind::BotGroup { Ok(()) } else { Err(ConversationError::Rejected("Group action required".into())) }
}

pub(super) fn require_active(state: &ConversationState) -> Result<(), ConversationError> {
    require_lifecycle(state, ConversationLifecycle::Active)
}

pub(super) fn require_lifecycle(state: &ConversationState, lifecycle: ConversationLifecycle) -> Result<(), ConversationError> {
    if state.lifecycle == lifecycle { Ok(()) } else { Err(ConversationError::Rejected(format!("conversation is {:?}", state.lifecycle))) }
}

pub(super) fn require_member(state: &ConversationState, bot_id: &ResourceId) -> Result<(), ConversationError> {
    if state.members.get(bot_id).is_some_and(|member| member.active) {
        Ok(())
    } else {
        Err(ConversationError::Rejected(format!("Bot is not active member: {bot_id}")))
    }
}
