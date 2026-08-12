use serde_json::Value;
use std::sync::Arc;

use crate::AppState;
use crate::bot::CreateBot;
use crate::bot::builder::{BuilderCommand, BuilderMessage, BuilderWrite};
use crate::core::identity::{ActorRef, SystemActor};

use super::{RpcResult, idempotency, now, owner, resource_id, trace, value};

pub(super) async fn handle(method: &str, params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    match method {
        "bot.builder.start" => start(params, state),
        "bot.builder.message" => message(params, state).await,
        "bot.builder.get" => value(state.bots.builder().get(&resource_id(params, "builder_session_id")?)?),
        "bot.builder.grant" => grant(params, state),
        "bot.builder.test" => test(params, state),
        "bot.builder.cancel" => {
            let id = resource_id(params, "builder_session_id")?;
            let current = state.bots.builder().get(&id)?;
            value(state.bots.builder().execute(BuilderWrite {
                builder_session_id: id,
                expected_version: current.event_version,
                idempotency_key: idempotency(params)?,
                actor: owner(),
                trace: trace(),
                command: BuilderCommand::Cancel { at_ms: now() },
            })?)
        }
        _ => Err(format!("unknown Bot Builder method: {method}").into()),
    }
}

fn grant(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let builder_id = resource_id(params, "builder_session_id")?;
    let builder = state.bots.builder().get(&builder_id)?;
    let draft = builder.draft.as_ref().ok_or("Builder draft is missing")?;
    let supplied =
        crate::core::identity::ContentHash::parse(params.get("draft_hash").and_then(Value::as_str).ok_or("missing draft_hash")?)?;
    if supplied != draft.content_hash {
        return Err("permission grant draft hash is stale".into());
    }
    let permission_hash = crate::bot::builder::permission_hash(&draft.definition)?;
    let grant_id = crate::bot::deterministic_id("grant", &[builder_id.as_str(), draft.content_hash.as_str()])?;
    value(state.bots.builder().execute(BuilderWrite {
        builder_session_id: builder_id,
        expected_version: builder.event_version,
        idempotency_key: idempotency(params)?,
        actor: owner(),
        trace: trace(),
        command: BuilderCommand::RecordGrant {
            grant: crate::bot::builder::PermissionGrant {
                grant_id,
                draft_hash: draft.content_hash.clone(),
                permission_hash,
                reason: params.get("reason").and_then(Value::as_str).ok_or("missing reason")?.into(),
                granted_at_ms: now(),
            },
            at_ms: now(),
        },
    })?)
}

fn start(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let bot_id = resource_id(params, "bot_id")?;
    if state.bots.definitions().get(&bot_id).is_err() {
        let definition = crate::bot::BotDefinition::empty(params.get("display_name").and_then(Value::as_str).unwrap_or("New Bot"));
        let create_key = crate::bot::deterministic_id("idem", &[idempotency(params)?.as_str(), "bot"])?;
        state.bots.definitions().create(CreateBot {
            bot_id: &bot_id,
            definition: &definition,
            actor: owner(),
            trace: trace(),
            idempotency_key: crate::core::identity::IdempotencyKey::parse(create_key.to_string())?,
            at_ms: now(),
        })?;
    }
    let builder_session_id = resource_id(params, "builder_session_id")?;
    value(state.bots.builder().execute(BuilderWrite {
        builder_session_id: builder_session_id.clone(),
        expected_version: 0,
        idempotency_key: idempotency(params)?,
        actor: owner(),
        trace: trace(),
        command: BuilderCommand::Start {
            builder_session_id,
            bot_id,
            user_goal: params.get("user_goal").and_then(Value::as_str).ok_or("missing user_goal")?.into(),
            at_ms: now(),
        },
    })?)
}

async fn message(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let builder_id = resource_id(params, "builder_session_id")?;
    let current = state.bots.builder().get(&builder_id)?;
    let text = params.get("text").and_then(Value::as_str).ok_or("missing text")?;
    let appended = state.bots.builder().execute(BuilderWrite {
        builder_session_id: builder_id.clone(),
        expected_version: current.event_version,
        idempotency_key: idempotency(params)?,
        actor: owner(),
        trace: trace(),
        command: BuilderCommand::AppendMessage {
            message: BuilderMessage {
                message_id: resource_id(params, "message_id")?,
                actor: owner(),
                text: text.into(),
                created_at_ms: now(),
            },
            at_ms: now(),
        },
    })?;
    let runtime = state.ready_active_runtime().await?;
    let store = crate::core::shared::lock(&state.auth_store).clone();
    let definition = crate::bot::builder::agent::generate_draft(
        &runtime.mrm(),
        &store,
        &appended.user_goal,
        &appended.messages,
        appended.draft.as_ref().map(|draft| &draft.definition),
        state.bots.capabilities(),
    )
    .await?;
    let draft_key = crate::bot::deterministic_id("idem", &[idempotency(params)?.as_str(), "draft"])?;
    let drafted = state.bots.builder().execute(BuilderWrite {
        builder_session_id: builder_id.clone(),
        expected_version: appended.event_version,
        idempotency_key: crate::core::identity::IdempotencyKey::parse(draft_key.to_string())?,
        actor: ActorRef::System { actor: SystemActor::Builder },
        trace: trace(),
        command: BuilderCommand::ReplaceDraft {
            expected_draft_version: appended.draft.as_ref().map_or(0, |draft| draft.version),
            definition: Box::new(definition),
            at_ms: now(),
        },
    })?;
    let sync_key = crate::bot::deterministic_id("idem", &[idempotency(params)?.as_str(), "sync"])?;
    state.bots.sync_builder_draft(&builder_id, crate::core::identity::IdempotencyKey::parse(sync_key.to_string())?, now())?;
    value(drafted)
}

fn test(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let builder_id = resource_id(params, "builder_session_id")?;
    let builder = state.bots.builder().get(&builder_id)?;
    let draft = builder.draft.as_ref().ok_or("Builder draft is missing")?;
    let run_id = resource_id(params, "run_id")?;
    let instruction = format!(
        "Execute a controlled validation scenario for this draft. Demonstrate each success criterion and produce a concise evidence report. Criteria:\n- {}",
        draft.definition.success_criteria.join("\n- ")
    );
    value(state.bots.queue_builder_test(
        &builder_id,
        run_id,
        vec![crate::agent::dcp::ProviderNeutralPart::Text { text: instruction }],
        idempotency(params)?,
        now(),
    )?)
}
