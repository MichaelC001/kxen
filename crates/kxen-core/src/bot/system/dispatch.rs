use std::collections::BTreeMap;

use crate::agent::dcp::ProviderNeutralPart;
use crate::core::delivery::DeliveryStatus;
use crate::core::identity::{ActorRef, IdempotencyKey, SystemActor, TraceContext};

use crate::bot::conversation::{ConversationCommand, ConversationWrite, MessagePart, TaskStatus};
use crate::bot::run::{RunTrigger, RunTriggerKind};

use super::{BotSystem, BotSystemError, DispatchReceipt, QueueRun};

impl BotSystem {
    pub fn dispatch_next_delivery(&self, at_ms: u64) -> Result<Option<DispatchReceipt>, BotSystemError> {
        for conversation in self.conversations.list()? {
            if conversation.lifecycle != crate::bot::conversation::ConversationLifecycle::Active {
                continue;
            }
            if (conversation.deliveries.in_flight.is_some() || !conversation.deliveries.queued.is_empty())
                && let Some(receipt) = self.dispatch_from_conversation(conversation, at_ms)?
            {
                return Ok(Some(receipt));
            }
        }
        Ok(None)
    }

    fn dispatch_from_conversation(
        &self,
        mut conversation: crate::bot::conversation::ConversationState,
        at_ms: u64,
    ) -> Result<Option<DispatchReceipt>, BotSystemError> {
        if conversation.deliveries.in_flight.is_none() {
            let head = conversation.deliveries.queued.front().ok_or_else(|| BotSystemError::Rejected("Delivery queue is empty".into()))?;
            let generation = crate::bot::ids::deterministic_id(
                "claim",
                &[conversation.conversation_id.as_str(), head.as_str(), &conversation.event_version.to_string()],
            )
            .map_err(BotSystemError::InvalidId)?;
            conversation = self.conversations.execute(ConversationWrite {
                conversation_id: conversation.conversation_id.clone(),
                expected_version: conversation.event_version,
                idempotency_key: stable_key("claim", &[conversation.conversation_id.as_str(), head.as_str(), generation.as_str()])?,
                actor: runtime_actor(),
                trace: TraceContext::default(),
                command: ConversationCommand::ClaimDelivery { generation, at_ms },
            })?;
        }
        let token =
            conversation.deliveries.in_flight.clone().ok_or_else(|| BotSystemError::Rejected("Delivery claim is missing".into()))?;
        let delivery_id = token.delivery_ids[0].clone();
        let delivery = conversation
            .deliveries
            .records
            .get(&delivery_id)
            .ok_or_else(|| BotSystemError::Rejected("claimed Delivery is missing".into()))?;
        if delivery.status != DeliveryStatus::InFlight {
            return Err(BotSystemError::Rejected("claimed Delivery is not in flight".into()));
        }
        let bot_id = match &delivery.envelope.recipient {
            ActorRef::Bot { id } => id.clone(),
            _ => return Err(BotSystemError::Rejected("Delivery recipient is not a Bot".into())),
        };
        if self.runs.list()?.into_iter().any(|run| {
            !run.status.is_terminal()
                && run.spec.bot_id == bot_id
                && run.spec.conversation_id.as_ref() == Some(&conversation.conversation_id)
        }) {
            self.conversations.execute(ConversationWrite {
                conversation_id: conversation.conversation_id.clone(),
                expected_version: conversation.event_version,
                idempotency_key: stable_key("busy_release", &[conversation.conversation_id.as_str(), token.generation.as_str()])?,
                actor: runtime_actor(),
                trace: TraceContext::default(),
                command: ConversationCommand::ReleaseDelivery { token, at_ms },
            })?;
            return Ok(None);
        }
        let message = conversation
            .messages
            .iter()
            .find(|message| message.message_id == delivery.envelope.payload.message_id)
            .ok_or_else(|| BotSystemError::Rejected("Delivery message is missing".into()))?;
        let bot = match self.active_bot(&bot_id) {
            Ok(bot) => bot,
            Err(error) => {
                self.reject_claimed_delivery(&conversation, &delivery_id, &bot_id, &token, error.to_string(), at_ms)?;
                return Ok(None);
            }
        };
        let revision = bot.current_revision().expect("active_bot requires a published revision");
        let run_id = crate::bot::run::deterministic_run_id(&delivery_id, &revision.revision_id, 0).map_err(BotSystemError::InvalidId)?;
        let run = match self.queue_run(QueueRun {
            run_id: run_id.clone(),
            bot_id: bot_id.clone(),
            revision_id: Some(revision.revision_id.clone()),
            trigger: RunTrigger { kind: RunTriggerKind::BotRequest, source_id: Some(delivery_id.clone()), occurrence_id: None },
            input: message.parts.iter().map(provider_part).collect(),
            conversation_id: Some(conversation.conversation_id.clone()),
            task_id: delivery.envelope.payload.task_id.clone(),
            budget_override: delivery
                .envelope
                .payload
                .task_id
                .as_ref()
                .and_then(|task_id| conversation.tasks.get(task_id))
                .map(|task| task.budget.clone()),
            actor: runtime_actor(),
            trace: TraceContext {
                causation_id: message.origin_run_id.clone(),
                correlation_id: message.correlation_id.clone().or_else(|| message.task_id.clone()),
                parent_operation_id: None,
            },
            idempotency_key: stable_key("queue", &[delivery_id.as_str(), revision.revision_id.as_str()])?,
            at_ms,
        }) {
            Ok(run) => run,
            Err(error) => {
                self.reject_claimed_delivery(&conversation, &delivery_id, &bot_id, &token, error.to_string(), at_ms)?;
                return Ok(None);
            }
        };
        if let Some(task_id) = &delivery.envelope.payload.task_id
            && conversation.tasks.get(task_id).is_some_and(|task| task.status == TaskStatus::Submitted)
        {
            conversation = self.conversations.execute(ConversationWrite {
                conversation_id: conversation.conversation_id.clone(),
                expected_version: conversation.event_version,
                idempotency_key: stable_key("task_start", &[task_id.as_str(), run_id.as_str()])?,
                actor: ActorRef::Bot { id: bot_id },
                trace: TraceContext::default(),
                command: ConversationCommand::ChangeTask {
                    task_id: task_id.clone(),
                    status: TaskStatus::Working,
                    result: Vec::new(),
                    at_ms,
                },
            })?;
        }
        self.conversations.execute(ConversationWrite {
            conversation_id: conversation.conversation_id.clone(),
            expected_version: conversation.event_version,
            idempotency_key: stable_key("ack", &[delivery_id.as_str(), run_id.as_str()])?,
            actor: runtime_actor(),
            trace: TraceContext::default(),
            command: ConversationCommand::AcknowledgeDelivery { token, run_ids: BTreeMap::from([(delivery_id.clone(), run_id)]), at_ms },
        })?;
        Ok(Some(DispatchReceipt { conversation_id: conversation.conversation_id, delivery_id, run }))
    }

    fn reject_claimed_delivery(
        &self,
        conversation: &crate::bot::conversation::ConversationState,
        delivery_id: &crate::core::identity::ResourceId,
        bot_id: &crate::core::identity::ResourceId,
        token: &crate::core::delivery::ClaimToken,
        reason: String,
        at_ms: u64,
    ) -> Result<(), BotSystemError> {
        let rejected = self.conversations.execute(ConversationWrite {
            conversation_id: conversation.conversation_id.clone(),
            expected_version: conversation.event_version,
            idempotency_key: stable_key("reject", &[delivery_id.as_str()])?,
            actor: runtime_actor(),
            trace: TraceContext::default(),
            command: ConversationCommand::RejectDelivery {
                delivery_id: delivery_id.clone(),
                generation: Some(token.generation.clone()),
                reason: reason.clone(),
                at_ms,
            },
        })?;
        if let Some(task_id) =
            conversation.deliveries.records.get(delivery_id).and_then(|delivery| delivery.envelope.payload.task_id.as_ref())
            && rejected.tasks.get(task_id).is_some_and(|task| !task.status.is_terminal())
        {
            self.conversations.execute(ConversationWrite {
                conversation_id: rejected.conversation_id.clone(),
                expected_version: rejected.event_version,
                idempotency_key: stable_key("task_reject", &[task_id.as_str(), delivery_id.as_str()])?,
                actor: ActorRef::Bot { id: bot_id.clone() },
                trace: TraceContext::default(),
                command: ConversationCommand::ChangeTask {
                    task_id: task_id.clone(),
                    status: TaskStatus::Rejected,
                    result: Vec::new(),
                    at_ms,
                },
            })?;
        }
        Ok(())
    }
}

pub(super) fn stable_key(prefix: &str, parts: &[&str]) -> Result<IdempotencyKey, BotSystemError> {
    let id = crate::bot::ids::deterministic_id(prefix, parts).map_err(BotSystemError::InvalidId)?;
    IdempotencyKey::parse(id.to_string()).map_err(BotSystemError::InvalidId)
}

pub(super) fn runtime_actor() -> ActorRef {
    ActorRef::System { actor: SystemActor::Runtime }
}

fn provider_part(part: &MessagePart) -> ProviderNeutralPart {
    match part {
        MessagePart::Text { text } => ProviderNeutralPart::Text { text: text.clone() },
        MessagePart::Data { schema_id, fields } => ProviderNeutralPart::Data { schema_id: schema_id.clone(), fields: fields.clone() },
        MessagePart::ArtifactRef { artifact } => ProviderNeutralPart::Artifact {
            artifact_id: artifact.artifact_id.clone(),
            content_hash: artifact.content_hash.clone(),
            media_type: artifact.media_type.clone(),
            display_name: artifact.display_name.clone(),
        },
    }
}
