use crate::core::identity::{ActorRef, SystemActor, TraceContext};

use crate::bot::conversation::{ConversationCommand, ConversationWrite, Message, MessageKind, MessagePart};
use crate::bot::routine::{ContextMode, OccurrenceStatus, RoutineCommand, RoutineLifecycle, RoutineWrite};
use crate::bot::run::{RunTrigger, RunTriggerKind};

use super::dispatch::{runtime_actor, stable_key};
use super::{BotSystem, BotSystemError, QueueRun, RoutineTickReport};

impl BotSystem {
    pub fn tick_routines(&self, observed_at_ms: u64) -> RoutineTickReport {
        let mut report = RoutineTickReport::default();
        let routines = match self.routines.list() {
            Ok(routines) => routines,
            Err(error) => {
                report.errors.push(error.to_string());
                return report;
            }
        };
        for routine in routines.into_iter().filter(|routine| routine.lifecycle == RoutineLifecycle::Active) {
            if let Err(error) = self.tick_routine(routine, observed_at_ms, &mut report) {
                report.errors.push(error.to_string());
            }
        }
        report
    }

    fn tick_routine(
        &self,
        routine: crate::bot::routine::RoutineState,
        observed_at_ms: u64,
        report: &mut RoutineTickReport,
    ) -> Result<(), BotSystemError> {
        let bot = self.definitions.get(&routine.definition.bot_id)?;
        let current_revision = bot.current_revision().map(|revision| revision.revision_id.clone());
        let state = self.routines.execute(RoutineWrite {
            routine_id: routine.routine_id.clone(),
            expected_version: routine.event_version,
            idempotency_key: stable_key("rtick", &[routine.routine_id.as_str(), &observed_at_ms.to_string()])?,
            actor: ActorRef::System { actor: SystemActor::Scheduler },
            trace: TraceContext::default(),
            command: RoutineCommand::Tick { observed_at_ms, resolved_revision_id: current_revision },
        })?;
        report.skipped_occurrences += state
            .occurrences
            .values()
            .filter(|occurrence| occurrence.observed_at_ms == observed_at_ms && occurrence.status == OccurrenceStatus::Skipped)
            .count();
        for occurrence in state
            .occurrences
            .values()
            .filter(|occurrence| occurrence.status == OccurrenceStatus::Recorded && occurrence.run_id.is_none())
            .cloned()
            .collect::<Vec<_>>()
        {
            self.dispatch_occurrence(&state, occurrence, observed_at_ms, report)?;
        }
        Ok(())
    }

    fn dispatch_occurrence(
        &self,
        state: &crate::bot::routine::RoutineState,
        occurrence: crate::bot::routine::RoutineOccurrence,
        at_ms: u64,
        report: &mut RoutineTickReport,
    ) -> Result<(), BotSystemError> {
        let revision_id = occurrence
            .resolved_revision_id
            .as_ref()
            .ok_or_else(|| BotSystemError::Rejected("Routine occurrence has no resolved revision".into()))?;
        let conversation_id = if state.definition.context_mode == ContextMode::ContinueConversation {
            let conversation_id = state
                .definition
                .target_conversation_id
                .clone()
                .ok_or_else(|| BotSystemError::Rejected("Routine continuation Conversation is missing".into()))?;
            let conversation = self.conversations.get(&conversation_id)?;
            let message_id =
                crate::bot::deterministic_id("bmsg", &[occurrence.occurrence_id.as_str(), "routine"]).map_err(BotSystemError::InvalidId)?;
            let mut parts = state.definition.input.iter().filter_map(routine_message_part).collect::<Vec<_>>();
            if parts.is_empty() {
                parts.push(MessagePart::Text { text: format!("Routine '{}' triggered", state.definition.name) });
            }
            self.conversations.execute(ConversationWrite {
                conversation_id: conversation_id.clone(),
                expected_version: conversation.event_version,
                idempotency_key: stable_key("routine_message", &[occurrence.occurrence_id.as_str()])?,
                actor: ActorRef::System { actor: SystemActor::Scheduler },
                trace: TraceContext::default(),
                command: ConversationCommand::Post {
                    message: Box::new(Message {
                        message_id,
                        conversation_id: conversation_id.clone(),
                        actor: ActorRef::System { actor: SystemActor::Scheduler },
                        kind: MessageKind::Status,
                        parts,
                        mentions: Default::default(),
                        everyone: false,
                        target_bot_id: None,
                        reply_to_message_id: None,
                        task_id: None,
                        origin_run_id: None,
                        causation_id: Some(occurrence.occurrence_id.clone()),
                        correlation_id: Some(state.routine_id.clone()),
                        delegation_depth: 0,
                        hop_count: 0,
                        created_at_ms: at_ms,
                    }),
                    task: None,
                    at_ms,
                },
            })?;
            Some(conversation_id)
        } else {
            None
        };
        let run_id = crate::bot::run::deterministic_run_id(&occurrence.occurrence_id, revision_id, 0).map_err(BotSystemError::InvalidId)?;
        let run = self.queue_run(QueueRun {
            run_id: run_id.clone(),
            bot_id: state.definition.bot_id.clone(),
            revision_id: Some(revision_id.clone()),
            trigger: RunTrigger {
                kind: RunTriggerKind::Routine,
                source_id: Some(state.routine_id.clone()),
                occurrence_id: Some(occurrence.occurrence_id.clone()),
            },
            input: state.definition.input.clone(),
            conversation_id,
            task_id: None,
            budget_override: state.definition.budget_override.clone(),
            actor: runtime_actor(),
            trace: TraceContext {
                causation_id: Some(occurrence.occurrence_id.clone()),
                correlation_id: Some(state.routine_id.clone()),
                parent_operation_id: None,
            },
            idempotency_key: stable_key("rqueue", &[occurrence.occurrence_id.as_str(), revision_id.as_str()])?,
            at_ms,
        })?;
        let latest = self.routines.get(&state.routine_id)?;
        self.routines.execute(RoutineWrite {
            routine_id: state.routine_id.clone(),
            expected_version: latest.event_version,
            idempotency_key: stable_key("rlink", &[occurrence.occurrence_id.as_str(), run_id.as_str()])?,
            actor: runtime_actor(),
            trace: TraceContext::default(),
            command: RoutineCommand::LinkRun { occurrence_id: occurrence.occurrence_id, run_id: run_id.clone(), at_ms },
        })?;
        report.queued_run_ids.push(run.spec.run_id);
        Ok(())
    }
}

fn routine_message_part(part: &crate::agent::dcp::ProviderNeutralPart) -> Option<MessagePart> {
    match part {
        crate::agent::dcp::ProviderNeutralPart::Text { text } => Some(MessagePart::Text { text: text.clone() }),
        crate::agent::dcp::ProviderNeutralPart::Data { schema_id, fields } => {
            Some(MessagePart::Data { schema_id: schema_id.clone(), fields: fields.clone() })
        }
        crate::agent::dcp::ProviderNeutralPart::Artifact { artifact_id, content_hash, media_type, display_name } => {
            Some(MessagePart::ArtifactRef {
                artifact: crate::bot::run::ArtifactRef {
                    artifact_id: artifact_id.clone(),
                    display_name: display_name.clone(),
                    media_type: media_type.clone(),
                    content_hash: content_hash.clone(),
                    size_bytes: 0,
                },
            })
        }
        crate::agent::dcp::ProviderNeutralPart::ToolCall { .. } | crate::agent::dcp::ProviderNeutralPart::ToolResult { .. } => None,
    }
}
