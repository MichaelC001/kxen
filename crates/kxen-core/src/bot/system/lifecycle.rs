use crate::bot::BotLifecycle;
use crate::bot::conversation::{ConversationCommand, ConversationKind, ConversationLifecycle, ConversationWrite};
use crate::bot::routine::{RoutineCommand, RoutineLifecycle, RoutineWrite};
use crate::core::identity::{ActorRef, SystemActor, TraceContext};

use super::dispatch::stable_key;
use super::{BotSystem, BotSystemError};

impl BotSystem {
    pub fn reconcile_inactive_bot_work(&self, at_ms: u64) -> Result<usize, BotSystemError> {
        let mut changes = 0;
        for routine in self.routines.list()?.into_iter().filter(|routine| routine.lifecycle == RoutineLifecycle::Active) {
            if self
                .definitions
                .get(&routine.definition.bot_id)
                .is_ok_and(|bot| bot.lifecycle == BotLifecycle::Active && bot.current_revision().is_some())
            {
                continue;
            }
            self.routines.execute(RoutineWrite {
                routine_id: routine.routine_id.clone(),
                expected_version: routine.event_version,
                idempotency_key: stable_key("bot_lifecycle_pause_routine", &[routine.routine_id.as_str()])?,
                actor: ActorRef::System { actor: SystemActor::Runtime },
                trace: TraceContext::default(),
                command: RoutineCommand::Pause { reason: "owning Bot is not active".into(), at_ms },
            })?;
            changes += 1;
        }
        for mut conversation in self
            .conversations
            .list()?
            .into_iter()
            .filter(|conversation| !matches!(conversation.lifecycle, ConversationLifecycle::Archived | ConversationLifecycle::Blocked))
        {
            let candidates = conversation
                .deliveries
                .records
                .iter()
                .filter_map(|(delivery_id, record)| match &record.envelope.recipient {
                    ActorRef::Bot { id }
                        if !self
                            .definitions
                            .get(id)
                            .is_ok_and(|bot| bot.lifecycle == BotLifecycle::Active && bot.current_revision().is_some()) =>
                    {
                        Some((delivery_id.clone(), id.clone()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            for (delivery_id, bot_id) in candidates {
                let generation = conversation
                    .deliveries
                    .in_flight
                    .as_ref()
                    .filter(|token| token.delivery_ids.contains(&delivery_id))
                    .map(|token| token.generation.clone());
                self.reject_delivery(
                    &conversation,
                    &delivery_id,
                    &bot_id,
                    generation,
                    "Delivery recipient Bot is not active".into(),
                    at_ms,
                )?;
                conversation = self.conversations.get(&conversation.conversation_id)?;
                changes += 1;
            }
        }
        Ok(changes)
    }

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
