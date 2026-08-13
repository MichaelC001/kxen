use crate::bot::BotLifecycle;
use crate::bot::conversation::{ConversationCommand, ConversationKind, ConversationState};
use crate::core::identity::{ActorRef, ResourceId};

use super::{BotSystem, BotSystemError};

impl BotSystem {
    pub(super) fn admit_conversation_command(
        &self,
        conversation_id: &ResourceId,
        command: &ConversationCommand,
        actor: &ActorRef,
    ) -> Result<(), BotSystemError> {
        match command {
            ConversationCommand::Create { kind, members, moderator_bot_id, .. } => {
                if actor != &ActorRef::Owner {
                    return Err(BotSystemError::Rejected("only owner can create Conversation".into()));
                }
                for member in members {
                    let bot = self.active_bot(&member.bot_id)?;
                    if *kind == ConversationKind::BotGroup && !bot.current_revision().unwrap().definition.communication.allow_groups {
                        return Err(BotSystemError::Rejected(format!("Bot does not allow Groups: {}", member.bot_id)));
                    }
                }
                if *kind == ConversationKind::BotDirect && members.len() == 2 {
                    self.admit_direct_pair(&members[0].bot_id, &members[1].bot_id)?;
                }
                if *kind == ConversationKind::BotGroup && moderator_bot_id.is_none() {
                    return Err(BotSystemError::Rejected("Group moderator is required".into()));
                }
            }
            ConversationCommand::AddMember { participant, .. } => {
                let bot = self.active_bot(&participant.bot_id)?;
                if !bot.current_revision().unwrap().definition.communication.allow_groups {
                    return Err(BotSystemError::Rejected("Bot does not allow Groups".into()));
                }
            }
            ConversationCommand::SetModerator { bot_id, .. } => {
                let bot = self.active_bot(bot_id)?;
                if !bot.current_revision().unwrap().definition.communication.allow_groups {
                    return Err(BotSystemError::Rejected("Bot does not allow Groups".into()));
                }
            }
            ConversationCommand::Reopen { .. } => {
                if actor != &ActorRef::Owner {
                    return Err(BotSystemError::Rejected("only owner can reopen Conversation".into()));
                }
                let conversation = self.conversations.get(conversation_id)?;
                let members = conversation.active_members().collect::<Vec<_>>();
                if conversation.kind != ConversationKind::BotDirect || members.len() != 2 {
                    return Err(BotSystemError::Rejected("only Bot Direct Conversation can be reopened".into()));
                }
                self.admit_direct_pair(members[0], members[1])?;
            }
            _ => {}
        }
        Ok(())
    }

    fn admit_direct_pair(&self, left_id: &ResourceId, right_id: &ResourceId) -> Result<(), BotSystemError> {
        let left = self.active_bot(left_id)?;
        let right = self.active_bot(right_id)?;
        let left_policy = &left.current_revision().unwrap().definition.communication;
        let right_policy = &right.current_revision().unwrap().definition.communication;
        if !left_policy.allow_direct
            || !right_policy.allow_direct
            || !left_policy.allowed_peers.contains(&right.bot_id)
            || !right_policy.allowed_peers.contains(&left.bot_id)
        {
            return Err(BotSystemError::Rejected("direct Conversation requires reciprocal allow_direct and peer allowlists".into()));
        }
        Ok(())
    }

    pub(super) fn admit_post(
        &self,
        state: &ConversationState,
        actor: &ActorRef,
        message: &crate::bot::conversation::Message,
    ) -> Result<(), BotSystemError> {
        if let Some(moderator) = &state.moderator_bot_id {
            self.active_bot(moderator)?;
        }
        if let ActorRef::Bot { id } = actor {
            let sender = self.active_bot(id)?;
            let policy = &sender.current_revision().unwrap().definition.communication;
            match state.kind {
                ConversationKind::BotDirect if !policy.allow_direct => {
                    return Err(BotSystemError::Rejected("sender does not allow direct Bot communication".into()));
                }
                ConversationKind::BotGroup if !policy.allow_groups => {
                    return Err(BotSystemError::Rejected("sender does not allow Group communication".into()));
                }
                _ => {}
            }
            if let Some(target) = &message.target_bot_id
                && state.kind == ConversationKind::BotDirect
            {
                if !policy.allowed_peers.contains(target) {
                    return Err(BotSystemError::Rejected("target is not in direct peer allowlist".into()));
                }
                let target_bot = self.active_bot(target)?;
                let target_policy = &target_bot.current_revision().unwrap().definition.communication;
                if !target_policy.allow_direct || !target_policy.allowed_peers.contains(id) {
                    return Err(BotSystemError::Rejected("direct target no longer permits the sender".into()));
                }
            }
        }
        Ok(())
    }

    pub(super) fn active_bot(&self, bot_id: &ResourceId) -> Result<crate::bot::BotState, BotSystemError> {
        let bot = self.definitions.get(bot_id)?;
        if bot.lifecycle == BotLifecycle::Active && bot.current_revision().is_some() {
            Ok(bot)
        } else {
            Err(BotSystemError::Rejected(format!("Bot is not active: {bot_id}")))
        }
    }
}
