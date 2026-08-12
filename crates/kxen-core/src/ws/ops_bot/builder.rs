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
    match state.bots.definitions().get(&bot_id) {
        Ok(_) => {}
        Err(crate::bot::BotError::NotFound(_)) => {
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
        Err(error) => return Err(error.into()),
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
    let message_id = resource_id(params, "message_id")?;
    let existing_index = current.messages.iter().position(|message| message.message_id == message_id);
    let appended = if let Some(index) = existing_index {
        let existing = &current.messages[index];
        if existing.actor != owner() || existing.text != text {
            return Err(format!("Builder message id collision: {message_id}").into());
        }
        if index + 1 < current.messages.len() {
            return value(current);
        }
        current
    } else {
        state.bots.builder().execute(BuilderWrite {
            builder_session_id: builder_id.clone(),
            expected_version: current.event_version,
            idempotency_key: idempotency(params)?,
            actor: owner(),
            trace: trace(),
            command: BuilderCommand::AppendMessage {
                message: BuilderMessage { message_id: message_id.clone(), actor: owner(), text: text.into(), created_at_ms: now() },
                at_ms: now(),
            },
        })?
    };
    if appended.draft.as_ref().and_then(|draft| draft.source_message_id.as_ref()) == Some(&message_id) {
        sync_message_draft(state, &builder_id, &message_id, &appended)?;
        return value(appended);
    }
    let runtime = state.ready_active_runtime().await?;
    let workspace_id = crate::bot::executor::workspace_id(runtime.root())?;
    let connectors = runtime.mcp().status().into_iter().map(|status| status.name).collect::<Vec<_>>();
    let mrm = runtime.mrm();
    let store = crate::core::shared::lock(&state.auth_store).clone();
    let definition = crate::bot::builder::agent::generate_draft(crate::bot::builder::agent::DraftGenerationInput {
        mrm: &mrm,
        store: &store,
        user_goal: &appended.user_goal,
        conversation: &appended.messages,
        current: appended.draft.as_ref().map(|draft| &draft.definition),
        capability_catalog: state.bots.capabilities(),
        workspace_id: &workspace_id,
        connectors: &connectors,
    })
    .await?;
    let draft_key = crate::bot::deterministic_id("idem", &["builder_message_draft", builder_id.as_str(), message_id.as_str()])?;
    let drafted = state.bots.builder().execute(BuilderWrite {
        builder_session_id: builder_id.clone(),
        expected_version: appended.event_version,
        idempotency_key: crate::core::identity::IdempotencyKey::parse(draft_key.to_string())?,
        actor: ActorRef::System { actor: SystemActor::Builder },
        trace: trace(),
        command: BuilderCommand::ReplaceDraft {
            expected_draft_version: appended.draft.as_ref().map_or(0, |draft| draft.version),
            source_message_id: Some(message_id.clone()),
            definition: Box::new(definition),
            at_ms: now(),
        },
    })?;
    sync_message_draft(state, &builder_id, &message_id, &drafted)?;
    value(drafted)
}

fn sync_message_draft(
    state: &Arc<AppState>,
    builder_id: &crate::core::identity::ResourceId,
    message_id: &crate::core::identity::ResourceId,
    builder: &crate::bot::builder::BuilderState,
) -> RpcResult<()> {
    let draft = builder.draft.as_ref().ok_or("Builder draft is missing")?;
    let bot = state.bots.definitions().get(&builder.bot_id)?;
    if bot.draft.as_ref().is_some_and(|bot_draft| bot_draft.content_hash == draft.content_hash) {
        return Ok(());
    }
    let sync_key = crate::bot::deterministic_id(
        "idem",
        &["builder_message_sync", builder_id.as_str(), message_id.as_str(), draft.content_hash.as_str()],
    )?;
    state.bots.sync_builder_draft(builder_id, crate::core::identity::IdempotencyKey::parse(sync_key.to_string())?, now())?;
    Ok(())
}

fn test(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let builder_id = resource_id(params, "builder_session_id")?;
    let builder = state.bots.builder().get(&builder_id)?;
    let draft = builder.draft.as_ref().ok_or("Builder draft is missing")?;
    let run_id = resource_id(params, "run_id")?;
    let report_shape = serde_json::json!({
        "criteria": draft
            .definition
            .success_criteria
            .iter()
            .cloned()
            .map(|criterion| (criterion, false))
            .collect::<std::collections::BTreeMap<_, _>>(),
        "summary": "Concise evidence summary"
    });
    let instruction = format!(
        "Execute a controlled validation scenario for this draft. Demonstrate each success criterion. Return exactly one JSON object with the exact criterion strings as keys, no markdown and no additional keys. Set a criterion to true only when the run produced concrete evidence for it. Required shape:\n{}",
        serde_json::to_string_pretty(&report_shape)?
    );
    value(state.bots.queue_builder_test(
        &builder_id,
        run_id,
        vec![crate::agent::dcp::ProviderNeutralPart::Text { text: instruction }],
        idempotency(params)?,
        now(),
    )?)
}
