use crate::bot::conversation::{ConversationState, Message};
use crate::bot::run::BotRunState;
use crate::core::identity::{IdempotencyKey, ResourceId};

pub(super) fn run(system: &crate::bot::system::BotSystem, run_id: &ResourceId) -> Result<BotRunState, String> {
    let run = system.runs().get(run_id).map_err(|error| error.to_string())?;
    if run.status != crate::bot::run::RunStatus::Running {
        return Err(format!("Bot domain tool requires a running BotRun, actual {:?}", run.status));
    }
    Ok(run)
}

pub(super) fn conversation(system: &crate::bot::system::BotSystem, run: &BotRunState) -> Result<ConversationState, String> {
    let id = run.spec.conversation_id.as_ref().ok_or("Bot domain tool requires a Conversation binding")?;
    system.conversations().get(id).map_err(|error| error.to_string())
}

pub(super) fn source_message<'a>(conversation: &'a ConversationState, run: &BotRunState) -> Option<&'a Message> {
    let delivery_id = run.spec.trigger.source_id.as_ref()?;
    conversation.messages.iter().find(|message| {
        crate::bot::deterministic_id("bdel", &[message.message_id.as_str(), run.spec.bot_id.as_str()])
            .is_ok_and(|candidate| &candidate == delivery_id)
    })
}

pub(super) fn lineage(conversation: &ConversationState, run: &BotRunState) -> (u16, u16) {
    if let Some(task) = run.spec.task_id.as_ref().and_then(|task_id| conversation.tasks.get(task_id)) {
        (task.delegation_depth, task.hop_count)
    } else {
        source_message(conversation, run).map_or((0, 0), |message| (message.delegation_depth, message.hop_count))
    }
}

pub(super) fn stable_key(prefix: &str, run_id: &ResourceId, args: &serde_json::Value) -> Result<IdempotencyKey, String> {
    let args = serde_json::to_string(args).map_err(|error| error.to_string())?;
    crate::bot::system::stable_idempotency(prefix, &[run_id.as_str(), &args])
}

pub(super) fn stable_id(prefix: &str, run_id: &ResourceId, args: &serde_json::Value) -> Result<ResourceId, String> {
    let args = serde_json::to_string(args).map_err(|error| error.to_string())?;
    crate::bot::deterministic_id(prefix, &[run_id.as_str(), &args])
}

pub(super) fn required<'a>(args: &'a serde_json::Value, field: &str) -> Result<&'a str, String> {
    args.get(field).and_then(serde_json::Value::as_str).filter(|value| !value.trim().is_empty()).ok_or_else(|| format!("missing {field}"))
}

pub(super) fn optional_id(args: &serde_json::Value, field: &str) -> Result<Option<ResourceId>, String> {
    args.get(field).and_then(serde_json::Value::as_str).map(ResourceId::parse).transpose()
}

pub(super) fn check_lineage(run: &BotRunState, depth: u16, hops: u16, child_tasks: u32) -> Result<(), String> {
    let usage = crate::agent::runtime::BudgetUsage { delegation_depth: depth, message_hops: hops, child_tasks, ..Default::default() };
    run.spec
        .permission
        .budget
        .exceeded(usage)
        .map(|exceeded| Err(format!("Bot collaboration budget exceeded: {exceeded:?}")))
        .unwrap_or(Ok(()))
}

pub(super) fn outbound_request_count(conversation: &ConversationState, run: &BotRunState) -> u32 {
    u32::try_from(
        conversation
            .messages
            .iter()
            .filter(|message| {
                message.origin_run_id.as_ref() == Some(&run.spec.run_id) && message.kind == crate::bot::conversation::MessageKind::Request
            })
            .count()
            .saturating_add(1),
    )
    .unwrap_or(u32::MAX)
}

pub(super) fn record_limit_rejected(
    system: &crate::bot::system::BotSystem,
    run: &BotRunState,
    conversation: &ConversationState,
    args: &serde_json::Value,
    reason: &str,
) -> Result<(), String> {
    let message = crate::bot::conversation::Message {
        message_id: stable_id("bmsg_limit_rejected", &run.spec.run_id, args)?,
        conversation_id: conversation.conversation_id.clone(),
        actor: crate::core::identity::ActorRef::Bot { id: run.spec.bot_id.clone() },
        kind: crate::bot::conversation::MessageKind::Status,
        parts: vec![crate::bot::conversation::MessagePart::Text { text: format!("limit_rejected: {reason}") }],
        mentions: Default::default(),
        everyone: false,
        target_bot_id: None,
        reply_to_message_id: source_message(conversation, run).map(|message| message.message_id.clone()),
        task_id: run.spec.task_id.clone(),
        origin_run_id: Some(run.spec.run_id.clone()),
        causation_id: run.spec.trigger.source_id.clone(),
        correlation_id: run.spec.task_id.clone(),
        delegation_depth: lineage(conversation, run).0,
        hop_count: lineage(conversation, run).1,
        created_at_ms: crate::core::shared::now_ms(),
    };
    system
        .post_conversation(crate::bot::system::PostConversation {
            conversation_id: conversation.conversation_id.clone(),
            expected_version: conversation.event_version,
            actor: message.actor.clone(),
            message,
            task: None,
            trace: crate::core::identity::TraceContext::default(),
            idempotency_key: stable_key("bot_limit_rejected", &run.spec.run_id, args)?,
            at_ms: crate::core::shared::now_ms(),
        })
        .map(|_| ())
        .map_err(|error| error.to_string())
}
