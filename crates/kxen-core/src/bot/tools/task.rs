use crate::bot::conversation::{ConversationCommand, ConversationWrite, Message, MessageKind, MessagePart, NewTask, TaskStatus};
use crate::bot::run::{ApprovalRequest, InputRequest, RunCommand, RunWrite};
use crate::bot::system::PostConversation;
use crate::core::identity::{ActorRef, ResourceId, TraceContext};

use super::helpers;

pub(super) fn execute(system: &crate::bot::system::BotSystem, run_id: &ResourceId, args: &serde_json::Value) -> Result<String, String> {
    let run = helpers::run(system, run_id)?;
    let conversation = helpers::conversation(system, &run)?;
    let action = helpers::required(args, "action")?;
    if action == "create" {
        return create(system, &run, conversation, args);
    }
    let task_id = helpers::optional_id(args, "task_id")?
        .or(run.spec.task_id.clone())
        .ok_or("bot_task action requires task_id or a task-bound Run")?;
    let task = conversation.tasks.get(&task_id).ok_or_else(|| format!("CollaborationTask not found: {task_id}"))?;
    if task.owner_bot_id != run.spec.bot_id {
        return Err("only the current task owner Bot can update it".into());
    }
    let (status, result) = match action {
        "start" if task.status == TaskStatus::Working => return serde_json::to_string(task).map_err(|error| error.to_string()),
        "start" => (TaskStatus::Working, Vec::new()),
        "need_input" => (TaskStatus::InputRequired, Vec::new()),
        "need_approval" => (TaskStatus::ApprovalRequired, Vec::new()),
        "complete" => (TaskStatus::Completed, vec![MessagePart::Text { text: helpers::required(args, "result")?.into() }]),
        "fail" => (TaskStatus::Failed, Vec::new()),
        "reject" => (TaskStatus::Rejected, Vec::new()),
        "cancel" => (TaskStatus::Canceled, Vec::new()),
        _ => return Err(format!("unknown bot_task action: {action}")),
    };
    let updated = system
        .conversations()
        .execute(ConversationWrite {
            conversation_id: conversation.conversation_id,
            expected_version: conversation.event_version,
            idempotency_key: helpers::stable_key("bot_task", run_id, args)?,
            actor: ActorRef::Bot { id: run.spec.bot_id.clone() },
            trace: TraceContext::default(),
            command: ConversationCommand::ChangeTask { task_id: task_id.clone(), status, result, at_ms: crate::core::shared::now_ms() },
        })
        .map_err(|error| error.to_string())?;
    if matches!(action, "need_input" | "need_approval" | "fail" | "reject" | "cancel") {
        post_status_note(system, &run, &updated, &task_id, action, args)?;
    }
    if matches!(action, "need_input" | "need_approval") {
        pause_run(system, &run, action, args)?;
    }
    serde_json::to_string(&updated.tasks[&task_id]).map_err(|error| error.to_string())
}

fn post_status_note(
    system: &crate::bot::system::BotSystem,
    run: &crate::bot::run::BotRunState,
    conversation: &crate::bot::conversation::ConversationState,
    task_id: &ResourceId,
    action: &str,
    args: &serde_json::Value,
) -> Result<(), String> {
    let field = if action == "need_input" { "prompt" } else { "reason" };
    let text = helpers::required(args, field)?.to_string();
    let message = Message {
        message_id: helpers::stable_id("bmsg_task_status", &run.spec.run_id, args)?,
        conversation_id: conversation.conversation_id.clone(),
        actor: ActorRef::Bot { id: run.spec.bot_id.clone() },
        kind: MessageKind::Status,
        parts: vec![MessagePart::Text { text: format!("Task {action}: {text}") }],
        mentions: Default::default(),
        everyone: false,
        target_bot_id: None,
        reply_to_message_id: helpers::source_message(conversation, run).map(|message| message.message_id.clone()),
        task_id: Some(task_id.clone()),
        origin_run_id: Some(run.spec.run_id.clone()),
        causation_id: run.spec.trigger.source_id.clone(),
        correlation_id: Some(task_id.clone()),
        delegation_depth: helpers::lineage(conversation, run).0,
        hop_count: helpers::lineage(conversation, run).1,
        created_at_ms: crate::core::shared::now_ms(),
    };
    system
        .post_conversation(PostConversation {
            conversation_id: conversation.conversation_id.clone(),
            expected_version: conversation.event_version,
            actor: message.actor.clone(),
            message,
            task: None,
            trace: TraceContext::default(),
            idempotency_key: helpers::stable_key("bot_task_status", &run.spec.run_id, args)?,
            at_ms: crate::core::shared::now_ms(),
        })
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn pause_run(
    system: &crate::bot::system::BotSystem,
    run: &crate::bot::run::BotRunState,
    action: &str,
    args: &serde_json::Value,
) -> Result<(), String> {
    let current = system.runs().get(&run.spec.run_id).map_err(|error| error.to_string())?;
    let command = if action == "need_input" {
        RunCommand::RequireInput {
            request: InputRequest {
                request_id: helpers::stable_id("input", &run.spec.run_id, args)?,
                prompt: helpers::required(args, "prompt")?.into(),
            },
            at_ms: crate::core::shared::now_ms(),
        }
    } else {
        let operation_id = helpers::stable_id("approval_op", &run.spec.run_id, args)?;
        RunCommand::RequestApproval {
            request: ApprovalRequest {
                approval_id: helpers::stable_id("approval", &run.spec.run_id, args)?,
                operation_id,
                summary: helpers::required(args, "reason")?.into(),
            },
            at_ms: crate::core::shared::now_ms(),
        }
    };
    system
        .runs()
        .execute(RunWrite {
            run_id: run.spec.run_id.clone(),
            expected_version: current.event_version,
            idempotency_key: helpers::stable_key("bot_task_pause", &run.spec.run_id, args)?,
            actor: ActorRef::Bot { id: run.spec.bot_id.clone() },
            trace: TraceContext::default(),
            command,
        })
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn create(
    system: &crate::bot::system::BotSystem,
    run: &crate::bot::run::BotRunState,
    conversation: crate::bot::conversation::ConversationState,
    args: &serde_json::Value,
) -> Result<String, String> {
    let target = ResourceId::parse(helpers::required(args, "target_bot_id")?)?;
    let title = helpers::required(args, "title")?.to_string();
    let input = helpers::required(args, "input")?.to_string();
    let expected_output = helpers::required(args, "expected_output")?.to_string();
    let (base_depth, base_hops) = helpers::lineage(&conversation, run);
    let depth = base_depth.saturating_add(1);
    let hops = base_hops.saturating_add(1);
    if let Err(reason) = helpers::check_lineage(run, depth, hops, helpers::outbound_request_count(&conversation, run)) {
        helpers::record_limit_rejected(system, run, &conversation, args, &reason)?;
        return Err(reason);
    }
    let task_id = helpers::stable_id("btask", &run.spec.run_id, args)?;
    let message_id = helpers::stable_id("bmsg", &run.spec.run_id, args)?;
    let parts = vec![MessagePart::Text { text: input.clone() }];
    let message = Message {
        message_id,
        conversation_id: conversation.conversation_id.clone(),
        actor: ActorRef::Bot { id: run.spec.bot_id.clone() },
        kind: MessageKind::Request,
        parts: parts.clone(),
        mentions: Default::default(),
        everyone: false,
        target_bot_id: Some(target.clone()),
        reply_to_message_id: helpers::source_message(&conversation, run).map(|message| message.message_id.clone()),
        task_id: Some(task_id.clone()),
        origin_run_id: Some(run.spec.run_id.clone()),
        causation_id: run.spec.trigger.source_id.clone(),
        correlation_id: run.spec.task_id.clone().or(Some(task_id.clone())),
        delegation_depth: depth,
        hop_count: hops,
        created_at_ms: crate::core::shared::now_ms(),
    };
    let state = system
        .post_conversation(PostConversation {
            conversation_id: conversation.conversation_id,
            expected_version: conversation.event_version,
            actor: message.actor.clone(),
            message,
            task: Some(NewTask {
                task_id: task_id.clone(),
                owner_bot_id: target,
                title,
                input: parts,
                expected_output,
                parent_task_id: run.spec.task_id.clone(),
                budget: run.spec.permission.budget.clone(),
            }),
            trace: TraceContext::default(),
            idempotency_key: helpers::stable_key("bot_task_create", &run.spec.run_id, args)?,
            at_ms: crate::core::shared::now_ms(),
        })
        .map_err(|error| error.to_string())?;
    serde_json::to_string(&state.tasks[&task_id]).map_err(|error| error.to_string())
}
