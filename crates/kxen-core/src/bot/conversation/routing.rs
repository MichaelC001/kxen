use std::collections::BTreeSet;

use crate::core::identity::{ActorRef, ResourceId};

use super::ConversationError;
use super::types::{ConversationKind, ConversationState, Message, MessageKind};

pub fn recipients(state: &ConversationState, message: &Message) -> Result<BTreeSet<ResourceId>, ConversationError> {
    if message.conversation_id != state.conversation_id {
        return Err(ConversationError::Rejected("message belongs to another conversation".into()));
    }
    if message.parts.is_empty() {
        return Err(ConversationError::Rejected("message parts cannot be empty".into()));
    }
    if message.delegation_depth > 8 || message.hop_count > 32 {
        return Err(ConversationError::Rejected("delegation depth or hop limit exceeded".into()));
    }
    match (&state.kind, &message.actor) {
        (ConversationKind::HumanBot, ActorRef::Owner) => only_member(state),
        (ConversationKind::HumanBot, ActorRef::Bot { id }) => human_bot_result(state, id, message),
        (ConversationKind::BotDirect, ActorRef::Bot { id }) => direct(state, id, message),
        (ConversationKind::BotGroup, ActorRef::Owner) => group_owner(state, message),
        (ConversationKind::BotGroup, ActorRef::Bot { id }) => group_bot(state, id, message),
        (_, ActorRef::System { .. }) => Ok(BTreeSet::new()),
        _ => Err(ConversationError::Rejected("actor is not permitted for this conversation kind".into())),
    }
}

fn human_bot_result(state: &ConversationState, sender: &ResourceId, message: &Message) -> Result<BTreeSet<ResourceId>, ConversationError> {
    ensure_sender(state, sender)?;
    if message.everyone
        || !message.mentions.is_empty()
        || message.target_bot_id.is_some()
        || !matches!(message.kind, MessageKind::Response | MessageKind::Notice | MessageKind::Status | MessageKind::Artifact)
    {
        return Err(ConversationError::Rejected("human_bot result must be timeline-only".into()));
    }
    Ok(BTreeSet::new())
}

fn only_member(state: &ConversationState) -> Result<BTreeSet<ResourceId>, ConversationError> {
    let members = active(state);
    if members.len() != 1 {
        return Err(ConversationError::Rejected("human_bot requires one active Bot".into()));
    }
    Ok(members)
}

fn direct(state: &ConversationState, sender: &ResourceId, message: &Message) -> Result<BTreeSet<ResourceId>, ConversationError> {
    ensure_sender(state, sender)?;
    if message.everyone || !message.mentions.is_empty() {
        return Err(ConversationError::Rejected("direct Bot messages cannot use mentions or everyone".into()));
    }
    match message.kind {
        MessageKind::Request | MessageKind::Response => {
            let target =
                message.target_bot_id.as_ref().ok_or_else(|| ConversationError::Rejected("direct message requires target".into()))?;
            ensure_target(state, sender, target)?;
            Ok([target.clone()].into_iter().collect())
        }
        MessageKind::Notice | MessageKind::Status | MessageKind::Artifact if message.target_bot_id.is_none() => Ok(BTreeSet::new()),
        _ => Err(ConversationError::Rejected("direct message kind or target is invalid".into())),
    }
}

fn group_owner(state: &ConversationState, message: &Message) -> Result<BTreeSet<ResourceId>, ConversationError> {
    if message.kind != MessageKind::Instruction || message.target_bot_id.is_some() {
        return Err(ConversationError::Rejected("owner Group post must be an instruction".into()));
    }
    if message.everyone {
        if !message.mentions.is_empty() {
            return Err(ConversationError::Rejected("everyone and mentions are mutually exclusive".into()));
        }
        return Ok(active(state));
    }
    if !message.mentions.is_empty() {
        for target in &message.mentions {
            ensure_active(state, target)?;
        }
        return Ok(message.mentions.clone());
    }
    let moderator = state.moderator_bot_id.as_ref().ok_or_else(|| ConversationError::Rejected("Group moderator is missing".into()))?;
    ensure_active(state, moderator)?;
    Ok([moderator.clone()].into_iter().collect())
}

fn group_bot(state: &ConversationState, sender: &ResourceId, message: &Message) -> Result<BTreeSet<ResourceId>, ConversationError> {
    ensure_sender(state, sender)?;
    if message.everyone || !message.mentions.is_empty() {
        return Err(ConversationError::Rejected("Bot cannot use Group mentions or everyone".into()));
    }
    match message.kind {
        MessageKind::Request | MessageKind::Response => {
            let target = message.target_bot_id.as_ref().ok_or_else(|| ConversationError::Rejected("Bot request requires target".into()))?;
            ensure_target(state, sender, target)?;
            Ok([target.clone()].into_iter().collect())
        }
        MessageKind::Notice | MessageKind::Status | MessageKind::Artifact => {
            if message.target_bot_id.is_some() {
                return Err(ConversationError::Rejected("timeline-only message cannot target a Bot".into()));
            }
            Ok(BTreeSet::new())
        }
        MessageKind::Instruction => Err(ConversationError::Rejected("Bot cannot post owner instruction".into())),
    }
}

fn active(state: &ConversationState) -> BTreeSet<ResourceId> {
    state.active_members().cloned().collect()
}

fn ensure_sender(state: &ConversationState, sender: &ResourceId) -> Result<(), ConversationError> {
    ensure_active(state, sender)
}

fn ensure_target(state: &ConversationState, sender: &ResourceId, target: &ResourceId) -> Result<(), ConversationError> {
    if sender == target {
        return Err(ConversationError::Rejected("sender cannot target itself".into()));
    }
    ensure_active(state, target)
}

fn ensure_active(state: &ConversationState, bot_id: &ResourceId) -> Result<(), ConversationError> {
    if state.members.get(bot_id).is_some_and(|member| member.active) {
        Ok(())
    } else {
        Err(ConversationError::Rejected(format!("Bot is not an active member: {bot_id}")))
    }
}
