//! Bot product RPC composition. Domain modules keep each handler bounded and
//! all writes retain version plus idempotency protection.

#[path = "ops_bot/builder.rs"]
mod builder;
#[path = "ops_bot/collaboration.rs"]
mod collaboration;
#[path = "ops_bot/definition.rs"]
mod definition;
#[path = "ops_bot/operations.rs"]
mod operations;

use serde_json::Value;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use crate::AppState;

pub(super) const METHODS: &[&str] = &[
    "bot.list",
    "bot.get",
    "bot.create",
    "bot.duplicate",
    "bot.draft.get",
    "bot.draft.patch",
    "bot.validate",
    "bot.publish",
    "bot.pause",
    "bot.resume",
    "bot.archive",
    "bot.trash",
    "bot.restore",
    "bot.builder.start",
    "bot.builder.message",
    "bot.builder.get",
    "bot.builder.grant",
    "bot.builder.test",
    "bot.builder.cancel",
    "bot.run.start",
    "bot.run.get",
    "bot.run.list",
    "bot.run.cancel",
    "bot.run.input",
    "bot.run.approval",
    "bot.routine.list",
    "bot.routine.create",
    "bot.routine.update",
    "bot.routine.pause",
    "bot.routine.resume",
    "bot.routine.run_now",
    "bot.routine.trash",
    "bot.routine.history",
    "bot.conversation.list",
    "bot.conversation.get",
    "bot.conversation.post",
    "bot.conversation.pause",
    "bot.conversation.resume",
    "bot.conversation.archive",
    "bot.direct.open",
    "bot.group.create",
    "bot.group.add_member",
    "bot.group.remove_member",
    "bot.group.set_moderator",
    "bot.group.stop",
    "bot.task.list",
    "bot.task.get",
    "bot.task.cancel",
    "bot.task.reassign",
    "bot.memory.list",
    "bot.memory.create",
    "bot.memory.revise",
    "bot.memory.remove",
    "bot.recovery.inspect",
    "bot.recovery.repair",
    "bot.recovery.clear",
    "bot.artifact.get",
    "bot.artifact.restore",
    "bot.artifact.trash",
];

pub(super) async fn handle(method: &str, params: &Value, state: &Arc<AppState>) -> Result<Value, String> {
    let result = if method.starts_with("bot.builder.") {
        builder::handle(method, params, state).await
    } else if method.starts_with("bot.conversation.")
        || method.starts_with("bot.direct.")
        || method.starts_with("bot.group.")
        || method.starts_with("bot.task.")
    {
        collaboration::handle(method, params, state)
    } else if method.starts_with("bot.run.")
        || method.starts_with("bot.routine.")
        || method.starts_with("bot.memory.")
        || method.starts_with("bot.recovery.")
        || method.starts_with("bot.artifact.")
    {
        operations::handle(method, params, state)
    } else {
        definition::handle(method, params, state).await
    };
    match result {
        Ok(value) => {
            publish_invalidations(method, params, &value, state);
            Ok(value)
        }
        Err(error) => Err(error.to_string()),
    }
}

fn publish_invalidations(method: &str, params: &Value, value: &Value, state: &AppState) {
    if !is_mutation(method) {
        return;
    }
    let seq = value.get("event_version").and_then(Value::as_u64).unwrap_or(0);
    let target = if method.starts_with("bot.builder.") {
        id_from(value, params, "builder_session_id").map(|id| (format!("bot-builder:{id}"), id))
    } else if method.starts_with("bot.run.") {
        value.pointer("/spec/run_id").and_then(Value::as_str).map(|id| (format!("bot-run:{id}"), id.to_string()))
    } else if method.starts_with("bot.routine.") {
        id_from(value, params, "routine_id").map(|id| (format!("bot-routine:{id}"), id))
    } else if method.starts_with("bot.conversation.")
        || method.starts_with("bot.direct.")
        || method.starts_with("bot.group.")
        || method.starts_with("bot.task.")
    {
        id_from(value, params, "conversation_id").map(|id| (format!("bot-conversation:{id}"), id))
    } else if method.starts_with("bot.recovery.") {
        param_id(params, "aggregate_id").map(|id| {
            let prefix = if params.get("kind").and_then(Value::as_str) == Some("bot_run") { "bot-run" } else { "bot" };
            (format!("{prefix}:{id}"), id)
        })
    } else if method.starts_with("bot.artifact.") {
        id_from(value, params, "artifact_id").map(|id| (format!("bot-artifact:{id}"), id))
    } else {
        id_from(value, params, "bot_id").or_else(|| param_id(params, "aggregate_id")).map(|id| (format!("bot:{id}"), id))
    };
    if let Some((topic, aggregate_id)) = target {
        state.bus.publish(crate::core::event::Event::BotUpdate { topic, aggregate_id, seq });
    }
    if method == "bot.builder.test"
        && let Some(run_id) = value.pointer("/spec/run_id").and_then(Value::as_str)
    {
        state.bus.publish(crate::core::event::Event::BotUpdate {
            topic: format!("bot-run:{run_id}"),
            aggregate_id: run_id.to_string(),
            seq,
        });
    }
}

fn id_from(value: &Value, params: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_string).or_else(|| param_id(params, field))
}

fn param_id(params: &Value, field: &str) -> Option<String> {
    params.get(field).and_then(Value::as_str).map(str::to_string)
}

fn is_mutation(method: &str) -> bool {
    !matches!(
        method,
        "bot.list"
            | "bot.get"
            | "bot.draft.get"
            | "bot.builder.get"
            | "bot.run.get"
            | "bot.run.list"
            | "bot.routine.list"
            | "bot.routine.history"
            | "bot.conversation.list"
            | "bot.conversation.get"
            | "bot.task.list"
            | "bot.task.get"
            | "bot.memory.list"
            | "bot.recovery.inspect"
            | "bot.artifact.get"
    )
}

#[derive(Debug)]
pub(super) struct RpcError(String);

impl Display for RpcError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RpcError {}

macro_rules! rpc_error_from {
    ($($error:ty),+ $(,)?) => {
        $(impl From<$error> for RpcError {
            fn from(error: $error) -> Self {
                Self(error.to_string())
            }
        })+
    };
}

rpc_error_from!(
    String,
    &str,
    serde_json::Error,
    crate::bot::BotError,
    crate::bot::builder::BuilderError,
    crate::bot::conversation::ConversationError,
    crate::bot::memory::MemoryError,
    crate::bot::routine::RoutineError,
    crate::bot::run::RunError,
    crate::bot::system::BotSystemError,
    crate::core::artifact::ArtifactError,
    crate::core::recovery::RecoveryError,
);

pub(super) type RpcResult<T> = Result<T, RpcError>;

pub(super) fn resource_id(params: &Value, field: &str) -> RpcResult<kxen_core::core::identity::ResourceId> {
    let value = params.get(field).and_then(Value::as_str).ok_or_else(|| format!("missing {field}"))?;
    Ok(kxen_core::core::identity::ResourceId::parse(value)?)
}

pub(super) fn idempotency(params: &Value) -> RpcResult<kxen_core::core::identity::IdempotencyKey> {
    let value = params.get("idempotency_key").and_then(Value::as_str).ok_or("missing idempotency_key")?;
    Ok(kxen_core::core::identity::IdempotencyKey::parse(value)?)
}

pub(super) fn expected(params: &Value) -> RpcResult<u64> {
    Ok(params.get("expected_version").and_then(Value::as_u64).ok_or("missing expected_version")?)
}

pub(super) fn now() -> u64 {
    kxen_core::core::shared::now_ms()
}

pub(super) fn owner() -> kxen_core::core::identity::ActorRef {
    kxen_core::core::identity::ActorRef::Owner
}

pub(super) fn trace() -> kxen_core::core::identity::TraceContext {
    kxen_core::core::identity::TraceContext::default()
}

pub(super) fn value<T: serde::Serialize>(value: T) -> RpcResult<Value> {
    Ok(serde_json::to_value(value)?)
}

pub(super) fn decode<T: serde::de::DeserializeOwned>(params: &Value, field: &str) -> RpcResult<T> {
    Ok(serde_json::from_value(params.get(field).cloned().ok_or_else(|| format!("missing {field}"))?)?)
}
