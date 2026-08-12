use std::collections::BTreeSet;

use crate::agent::dcp::ProviderNeutralPart;
use crate::bot::builder::{BuilderCommand, BuilderWrite, TestEvidence};
use crate::bot::conversation::{ConversationCommand, ConversationWrite, Message, MessageKind, MessagePart, TaskStatus};
use crate::bot::routine::{RoutineCommand, RoutineWrite};
use crate::bot::run::{BotRunState, RunStatus};
use crate::core::identity::{ActorRef, SystemActor, TraceContext};

use super::dispatch::{runtime_actor, stable_key};
use super::{BotSystem, BotSystemError};

impl BotSystem {
    pub fn reconcile_runs(&self, at_ms: u64) -> Vec<String> {
        let runs = match self.runs.list() {
            Ok(runs) => runs,
            Err(error) => return vec![error.to_string()],
        };
        runs.iter()
            .filter(|run| run.spec.task_id.is_some() || run.status.is_terminal())
            .filter_map(|run| self.settle_run(run, at_ms).err().map(|error| format!("{}: {error}", run.spec.run_id)))
            .collect()
    }

    pub fn settle_run(&self, run: &BotRunState, at_ms: u64) -> Result<(), BotSystemError> {
        self.sync_collaboration(run, at_ms)?;
        if !run.status.is_terminal() {
            return Ok(());
        }
        match run.spec.trigger.kind {
            crate::bot::run::RunTriggerKind::BuilderTest => self.record_builder_evidence(run, at_ms)?,
            crate::bot::run::RunTriggerKind::Routine => self.record_routine_result(run, at_ms)?,
            _ => {}
        }
        Ok(())
    }

    fn sync_collaboration(&self, run: &BotRunState, at_ms: u64) -> Result<(), BotSystemError> {
        let Some(conversation_id) = &run.spec.conversation_id else { return Ok(()) };
        let mut conversation = self.conversations.get(conversation_id)?;
        if let Some(task_id) = &run.spec.task_id
            && let Some(task) = conversation.tasks.get(task_id)
        {
            let target = task_status(run.status);
            if !task.status.is_terminal() && task.status != target {
                let result = if target == TaskStatus::Completed { result_parts(run) } else { Vec::new() };
                conversation = self.conversations.execute(ConversationWrite {
                    conversation_id: conversation_id.clone(),
                    expected_version: conversation.event_version,
                    idempotency_key: stable_key("run_task", &[run.spec.run_id.as_str(), status_name(target)])?,
                    actor: ActorRef::Bot { id: run.spec.bot_id.clone() },
                    trace: TraceContext::default(),
                    command: ConversationCommand::ChangeTask { task_id: task_id.clone(), status: target, result, at_ms },
                })?;
            }
        }
        if !run.status.is_terminal() || run.spec.trigger.kind == crate::bot::run::RunTriggerKind::BuilderTest {
            return Ok(());
        }
        let message_id =
            crate::bot::deterministic_id("bmsg", &[run.spec.run_id.as_str(), "terminal"]).map_err(BotSystemError::InvalidId)?;
        if conversation.messages.iter().any(|message| message.message_id == message_id) {
            return Ok(());
        }
        let source = source_message(&conversation, run);
        let task_originator = run.spec.task_id.as_ref().and_then(|task_id| conversation.tasks.get(task_id)).map(|task| &task.originator);
        let target = match task_originator.or_else(|| source.map(|message| &message.actor)) {
            Some(ActorRef::Bot { id }) if source.is_none_or(|message| message.kind == MessageKind::Request) => Some(id.clone()),
            _ => None,
        };
        let kind = if target.is_some() { MessageKind::Response } else { MessageKind::Notice };
        let source_depth = source.map_or(0, |message| message.delegation_depth);
        let source_hops = source.map_or(0, |message| message.hop_count);
        let message = Message {
            message_id,
            conversation_id: conversation_id.clone(),
            actor: ActorRef::Bot { id: run.spec.bot_id.clone() },
            kind,
            parts: terminal_parts(run),
            mentions: BTreeSet::new(),
            everyone: false,
            target_bot_id: target,
            reply_to_message_id: source.map(|message| message.message_id.clone()),
            task_id: run.spec.task_id.clone(),
            origin_run_id: Some(run.spec.run_id.clone()),
            causation_id: run.spec.trigger.source_id.clone(),
            correlation_id: run.spec.task_id.clone().or_else(|| source.and_then(|message| message.correlation_id.clone())),
            delegation_depth: source_depth,
            hop_count: source_hops.saturating_add(1),
            created_at_ms: at_ms,
        };
        self.conversations.execute(ConversationWrite {
            conversation_id: conversation_id.clone(),
            expected_version: conversation.event_version,
            idempotency_key: stable_key("run_message", &[run.spec.run_id.as_str()])?,
            actor: message.actor.clone(),
            trace: TraceContext::default(),
            command: ConversationCommand::Post { message: Box::new(message), task: None, at_ms },
        })?;
        Ok(())
    }

    fn record_builder_evidence(&self, run: &BotRunState, at_ms: u64) -> Result<(), BotSystemError> {
        let builder_id =
            run.spec.trigger.source_id.as_ref().ok_or_else(|| BotSystemError::Rejected("BuilderTest source missing".into()))?;
        let builder = self.builder.get(builder_id)?;
        if builder.active_test_run_id.as_ref() != Some(&run.spec.run_id) {
            return Ok(());
        }
        let draft = builder.draft.as_ref().ok_or_else(|| BotSystemError::Rejected("BuilderTest draft missing".into()))?;
        let passed = run.status == RunStatus::Completed;
        let criteria = draft.definition.success_criteria.iter().cloned().map(|criterion| (criterion, passed)).collect();
        self.builder.execute(BuilderWrite {
            builder_session_id: builder_id.clone(),
            expected_version: builder.event_version,
            idempotency_key: stable_key("builder_evidence", &[run.spec.run_id.as_str()])?,
            actor: runtime_actor(),
            trace: TraceContext::default(),
            command: BuilderCommand::RecordTestEvidence {
                evidence: TestEvidence {
                    run_id: run.spec.run_id.clone(),
                    draft_hash: draft.content_hash.clone(),
                    passed,
                    criteria,
                    summary: terminal_text(run),
                    recorded_at_ms: at_ms,
                },
                at_ms,
            },
        })?;
        Ok(())
    }

    fn record_routine_result(&self, run: &BotRunState, at_ms: u64) -> Result<(), BotSystemError> {
        let routine_id = run.spec.trigger.source_id.as_ref().ok_or_else(|| BotSystemError::Rejected("Routine source missing".into()))?;
        let occurrence_id =
            run.spec.trigger.occurrence_id.as_ref().ok_or_else(|| BotSystemError::Rejected("Routine occurrence missing".into()))?;
        let routine = self.routines.get(routine_id)?;
        self.routines.execute(RoutineWrite {
            routine_id: routine_id.clone(),
            expected_version: routine.event_version,
            idempotency_key: stable_key("routine_result", &[run.spec.run_id.as_str()])?,
            actor: ActorRef::System { actor: SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RoutineCommand::RecordResult {
                occurrence_id: occurrence_id.clone(),
                error: (run.status != RunStatus::Completed).then(|| terminal_text(run)),
                at_ms,
            },
        })?;
        Ok(())
    }
}

fn source_message<'a>(conversation: &'a crate::bot::conversation::ConversationState, run: &BotRunState) -> Option<&'a Message> {
    let delivery_id = run.spec.trigger.source_id.as_ref()?;
    conversation.messages.iter().find(|message| {
        crate::bot::deterministic_id("bdel", &[message.message_id.as_str(), run.spec.bot_id.as_str()]).as_ref() == Ok(delivery_id)
    })
}

fn task_status(status: RunStatus) -> TaskStatus {
    match status {
        RunStatus::Queued | RunStatus::Running => TaskStatus::Working,
        RunStatus::ApprovalRequired => TaskStatus::ApprovalRequired,
        RunStatus::InputRequired => TaskStatus::InputRequired,
        RunStatus::Completed => TaskStatus::Completed,
        RunStatus::Failed => TaskStatus::Failed,
        RunStatus::Canceled => TaskStatus::Canceled,
        RunStatus::Rejected => TaskStatus::Rejected,
        RunStatus::Blocked => TaskStatus::Blocked,
    }
}

fn status_name(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Submitted => "submitted",
        TaskStatus::Working => "working",
        TaskStatus::InputRequired => "input_required",
        TaskStatus::ApprovalRequired => "approval_required",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Canceled => "canceled",
        TaskStatus::Rejected => "rejected",
        TaskStatus::Blocked => "blocked",
    }
}

fn result_parts(run: &BotRunState) -> Vec<MessagePart> {
    run.result
        .iter()
        .filter_map(convert_part)
        .chain(run.artifacts.iter().cloned().map(|artifact| MessagePart::ArtifactRef { artifact }))
        .collect()
}

fn terminal_parts(run: &BotRunState) -> Vec<MessagePart> {
    let parts = result_parts(run);
    if parts.is_empty() { vec![MessagePart::Text { text: terminal_text(run) }] } else { parts }
}

fn terminal_text(run: &BotRunState) -> String {
    run.result
        .iter()
        .find_map(|part| match part {
            ProviderNeutralPart::Text { text } => Some(text.clone()),
            _ => None,
        })
        .or_else(|| run.error_message.clone())
        .unwrap_or_else(|| format!("BotRun ended as {:?}", run.status))
}

fn convert_part(part: &ProviderNeutralPart) -> Option<MessagePart> {
    match part {
        ProviderNeutralPart::Text { text } => Some(MessagePart::Text { text: text.clone() }),
        ProviderNeutralPart::Data { schema_id, fields } => Some(MessagePart::Data { schema_id: schema_id.clone(), fields: fields.clone() }),
        ProviderNeutralPart::Artifact { artifact_id, content_hash, media_type, display_name } => Some(MessagePart::ArtifactRef {
            artifact: crate::bot::run::ArtifactRef {
                artifact_id: artifact_id.clone(),
                display_name: display_name.clone(),
                media_type: media_type.clone(),
                content_hash: content_hash.clone(),
                size_bytes: 0,
            },
        }),
        ProviderNeutralPart::ToolCall { .. } | ProviderNeutralPart::ToolResult { .. } => None,
    }
}
