use crate::bot::BotLifecycle;
use crate::bot::conversation::{ConversationCommand, ConversationKind, ConversationLifecycle, ConversationWrite};
use crate::core::identity::{ActorRef, TraceContext};

use super::dispatch::stable_key;
use super::{BotSystem, BotSystemError};

impl BotSystem {
    pub fn reconcile_group_lifecycle(&self, at_ms: u64) -> Result<usize, BotSystemError> {
        let mut paused = 0;
        for conversation in self.conversations.list()?.into_iter().filter(|conversation| {
            conversation.kind == ConversationKind::BotGroup && conversation.lifecycle == ConversationLifecycle::Active
        }) {
            let Some(moderator_id) = &conversation.moderator_bot_id else { continue };
            let active = self
                .definitions
                .get(moderator_id)
                .is_ok_and(|bot| bot.lifecycle == BotLifecycle::Active && bot.current_revision().is_some());
            if active {
                continue;
            }
            self.conversations.execute(ConversationWrite {
                conversation_id: conversation.conversation_id.clone(),
                expected_version: conversation.event_version,
                idempotency_key: stable_key(
                    "moderator_pause",
                    &[conversation.conversation_id.as_str(), moderator_id.as_str(), &conversation.event_version.to_string()],
                )?,
                actor: ActorRef::Owner,
                trace: TraceContext::default(),
                command: ConversationCommand::Pause { at_ms },
            })?;
            paused += 1;
        }
        Ok(paused)
    }
}
