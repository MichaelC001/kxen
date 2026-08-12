use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::Arc;

use crate::AppState;
use crate::bot::conversation::{
    BotParticipant, ConversationCommand, ConversationKind, ConversationLifecycle, Message, MessageKind, MessagePart, TaskStatus,
};
use crate::bot::system::{ConversationMutation, PostConversation};

use super::{RpcResult, decode, expected, idempotency, now, owner, resource_id, trace, value};

pub(super) fn handle(method: &str, params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    match method {
        "bot.conversation.list" => list(params, state),
        "bot.conversation.get" => value(state.bots.conversations().get(&resource_id(params, "conversation_id")?)?),
        "bot.conversation.post" => post(params, state),
        "bot.conversation.pause" => mutate_lifecycle(params, state, ConversationCommand::Pause { at_ms: now() }),
        "bot.conversation.resume" => mutate_lifecycle(params, state, ConversationCommand::Resume { at_ms: now() }),
        "bot.conversation.archive" => mutate_lifecycle(params, state, ConversationCommand::Archive { at_ms: now() }),
        "bot.direct.open" => direct_open(params, state),
        "bot.group.create" => group_create(params, state),
        "bot.group.add_member" => add_member(params, state),
        "bot.group.remove_member" => group_mutation(params, state, |bot_id| ConversationCommand::RemoveMember { bot_id, at_ms: now() }),
        "bot.group.set_moderator" => group_mutation(params, state, |bot_id| ConversationCommand::SetModerator { bot_id, at_ms: now() }),
        "bot.group.stop" => stop_group(params, state),
        "bot.task.list" => task_list(params, state),
        "bot.task.get" => task_get(params, state),
        "bot.task.cancel" => task_cancel(params, state),
        "bot.task.reassign" => task_reassign(params, state),
        _ => Err(format!("unknown Bot collaboration method: {method}").into()),
    }
}

fn list(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let mut conversations = state.bots.conversations().list()?;
    if let Some(kind) = params.get("kind").and_then(Value::as_str) {
        conversations.retain(|conversation| format!("{:?}", conversation.kind).eq_ignore_ascii_case(kind));
    }
    if !params.get("include_archived").and_then(Value::as_bool).unwrap_or(false) {
        conversations.retain(|conversation| conversation.lifecycle != ConversationLifecycle::Archived);
    }
    value(conversations)
}

fn post(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let conversation_id = resource_id(params, "conversation_id")?;
    let message = Message {
        message_id: resource_id(params, "message_id")?,
        conversation_id: conversation_id.clone(),
        actor: owner(),
        kind: MessageKind::Instruction,
        parts: decode::<Vec<MessagePart>>(params, "parts")?,
        mentions: params
            .get("mentions")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| error.to_string())?
            .unwrap_or_default(),
        everyone: params.get("everyone").and_then(Value::as_bool).unwrap_or(false),
        target_bot_id: None,
        reply_to_message_id: optional_id(params, "reply_to_message_id")?,
        task_id: optional_id(params, "task_id")?,
        origin_run_id: None,
        causation_id: None,
        correlation_id: optional_id(params, "correlation_id")?,
        delegation_depth: 0,
        hop_count: 0,
        created_at_ms: now(),
    };
    value(state.bots.post_conversation(PostConversation {
        conversation_id,
        expected_version: expected(params)?,
        actor: owner(),
        message,
        task: params.get("task").cloned().map(serde_json::from_value).transpose().map_err(|error| error.to_string())?,
        trace: trace(),
        idempotency_key: idempotency(params)?,
        at_ms: now(),
    })?)
}

fn direct_open(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let left = resource_id(params, "left_bot_id")?;
    let right = resource_id(params, "right_bot_id")?;
    let conversation_id = crate::bot::conversation::direct_conversation_id(&left, &right)?;
    if params.get("conversation_id").and_then(Value::as_str).is_some_and(|supplied| supplied != conversation_id.as_str()) {
        return Err("direct Conversation id must be derived from the two Bot ids".into());
    }
    if let Some(existing) = state.bots.conversations().list()?.into_iter().find(|conversation| {
        conversation.kind == ConversationKind::BotDirect
            && conversation.active_members().cloned().collect::<BTreeSet<_>>() == [left.clone(), right.clone()].into_iter().collect()
            && conversation.lifecycle != ConversationLifecycle::Archived
    }) {
        return value(existing);
    }
    create_conversation(params, state, conversation_id, ConversationKind::BotDirect, vec![left, right], None)
}

fn group_create(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    create_conversation(
        params,
        state,
        resource_id(params, "conversation_id")?,
        ConversationKind::BotGroup,
        decode(params, "bot_ids")?,
        Some(resource_id(params, "moderator_bot_id")?),
    )
}

fn create_conversation(
    params: &Value,
    state: &Arc<AppState>,
    conversation_id: crate::core::identity::ResourceId,
    kind: ConversationKind,
    bot_ids: Vec<crate::core::identity::ResourceId>,
    moderator_bot_id: Option<crate::core::identity::ResourceId>,
) -> RpcResult<Value> {
    let members =
        bot_ids.into_iter().map(|bot_id| BotParticipant { bot_id, joined_at_seq: 0, history_visible_from_seq: 0, active: true }).collect();
    value(state.bots.mutate_conversation(ConversationMutation {
        conversation_id: conversation_id.clone(),
        expected_version: 0,
        actor: owner(),
        command: ConversationCommand::Create { conversation_id, kind, members, moderator_bot_id, at_ms: now() },
        trace: trace(),
        idempotency_key: idempotency(params)?,
    })?)
}

fn add_member(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let expected_version = expected(params)?;
    let participant = BotParticipant {
        bot_id: resource_id(params, "bot_id")?,
        joined_at_seq: expected_version.saturating_add(1),
        history_visible_from_seq: params
            .get("history_visible_from_seq")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| expected_version.saturating_add(1)),
        active: true,
    };
    mutate(params, state, ConversationCommand::AddMember { participant, at_ms: now() })
}

fn group_mutation(
    params: &Value,
    state: &Arc<AppState>,
    command: impl FnOnce(crate::core::identity::ResourceId) -> ConversationCommand,
) -> RpcResult<Value> {
    mutate(params, state, command(resource_id(params, "bot_id")?))
}

fn mutate_lifecycle(params: &Value, state: &Arc<AppState>, command: ConversationCommand) -> RpcResult<Value> {
    mutate(params, state, command)
}

fn mutate(params: &Value, state: &Arc<AppState>, command: ConversationCommand) -> RpcResult<Value> {
    value(state.bots.mutate_conversation(ConversationMutation {
        conversation_id: resource_id(params, "conversation_id")?,
        expected_version: expected(params)?,
        actor: owner(),
        command,
        trace: trace(),
        idempotency_key: idempotency(params)?,
    })?)
}

fn stop_group(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let conversation_id = resource_id(params, "conversation_id")?;
    let mut conversation = state.bots.mutate_conversation(ConversationMutation {
        conversation_id: conversation_id.clone(),
        expected_version: expected(params)?,
        actor: owner(),
        command: ConversationCommand::Pause { at_ms: now() },
        trace: trace(),
        idempotency_key: idempotency(params)?,
    })?;
    let pending = conversation.deliveries.records.keys().cloned().collect::<Vec<_>>();
    for delivery_id in pending {
        let generation = conversation
            .deliveries
            .in_flight
            .as_ref()
            .filter(|token| token.delivery_ids.contains(&delivery_id))
            .map(|token| token.generation.clone());
        conversation = state.bots.mutate_conversation(ConversationMutation {
            conversation_id: conversation_id.clone(),
            expected_version: conversation.event_version,
            actor: crate::core::identity::ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            command: ConversationCommand::RejectDelivery {
                delivery_id: delivery_id.clone(),
                generation,
                reason: "Bot Group stopped by owner".into(),
                at_ms: now(),
            },
            trace: trace(),
            idempotency_key: crate::bot::system::stable_idempotency("group_stop_delivery", &[delivery_id.as_str()])?,
        })?;
    }
    for run in state
        .bots
        .runs()
        .list()?
        .into_iter()
        .filter(|run| run.spec.conversation_id.as_ref() == Some(&conversation_id) && !run.status.is_terminal())
    {
        state.bot_executor.cancel(&run.spec.run_id);
        state.bots.runs().execute(crate::bot::run::RunWrite {
            run_id: run.spec.run_id.clone(),
            expected_version: run.event_version,
            idempotency_key: crate::bot::system::stable_idempotency("group_stop_run", &[run.spec.run_id.as_str()])?,
            actor: owner(),
            trace: trace(),
            command: crate::bot::run::RunCommand::Cancel { reason: "Bot Group stopped by owner".into(), usage: run.usage, at_ms: now() },
        })?;
    }
    value(conversation)
}

fn task_list(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let mut tasks =
        state.bots.conversations().list()?.into_iter().flat_map(|conversation| conversation.tasks.into_values()).collect::<Vec<_>>();
    if let Some(conversation_id) = params.get("conversation_id").and_then(Value::as_str) {
        tasks.retain(|task| task.conversation_id.as_str() == conversation_id);
    }
    if let Some(owner_bot_id) = params.get("owner_bot_id").and_then(Value::as_str) {
        tasks.retain(|task| task.owner_bot_id.as_str() == owner_bot_id);
    }
    value(tasks)
}

fn task_get(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let task_id = resource_id(params, "task_id")?;
    state
        .bots
        .conversations()
        .list()?
        .into_iter()
        .find_map(|conversation| conversation.tasks.get(&task_id).cloned())
        .map(value)
        .unwrap_or_else(|| Err(format!("task not found: {task_id}").into()))
}

fn task_cancel(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    mutate(
        params,
        state,
        ConversationCommand::ChangeTask {
            task_id: resource_id(params, "task_id")?,
            status: TaskStatus::Canceled,
            result: Vec::new(),
            at_ms: now(),
        },
    )
}

fn task_reassign(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    mutate(
        params,
        state,
        ConversationCommand::ReassignTask {
            task_id: resource_id(params, "task_id")?,
            owner_bot_id: resource_id(params, "bot_id")?,
            at_ms: now(),
        },
    )
}

fn optional_id(params: &Value, field: &str) -> RpcResult<Option<crate::core::identity::ResourceId>> {
    Ok(params.get(field).and_then(Value::as_str).map(crate::core::identity::ResourceId::parse).transpose()?)
}
