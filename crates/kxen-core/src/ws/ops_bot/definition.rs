use serde_json::{Value, json};
use std::sync::Arc;

use crate::AppState;
use crate::bot::{ChangeLifecycle, CreateBot, LifecycleChange, ReplaceDraft};

use super::{RpcResult, decode, expected, idempotency, now, owner, resource_id, trace, value};

pub(super) async fn handle(method: &str, params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    match method {
        "bot.list" => value(state.bots.definitions().list(params.get("include_trashed").and_then(Value::as_bool).unwrap_or(false))?),
        "bot.get" => value(state.bots.definitions().get(&resource_id(params, "bot_id")?)?),
        "bot.create" => {
            let bot_id = resource_id(params, "bot_id")?;
            let definition = params
                .get("definition")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|error| error.to_string())?
                .unwrap_or_else(|| {
                    crate::bot::BotDefinition::empty(params.get("display_name").and_then(Value::as_str).unwrap_or("New Bot"))
                });
            value(state.bots.definitions().create(CreateBot {
                bot_id: &bot_id,
                definition: &definition,
                actor: owner(),
                trace: trace(),
                idempotency_key: idempotency(params)?,
                at_ms: now(),
            })?)
        }
        "bot.duplicate" => {
            let source = state.bots.definitions().get(&resource_id(params, "source_bot_id")?)?;
            let revision = match params.get("revision_id").and_then(Value::as_str) {
                Some(id) => source.revisions.values().find(|revision| revision.revision_id.as_str() == id).ok_or("revision not found")?,
                None => source.current_revision().ok_or("source Bot has no published revision")?,
            };
            let bot_id = resource_id(params, "bot_id")?;
            let mut definition = revision.definition.clone();
            if let Some(name) = params.get("display_name").and_then(Value::as_str) {
                definition.display_name = name.into();
            }
            value(state.bots.definitions().create(CreateBot {
                bot_id: &bot_id,
                definition: &definition,
                actor: owner(),
                trace: trace(),
                idempotency_key: idempotency(params)?,
                at_ms: now(),
            })?)
        }
        "bot.draft.get" => {
            let bot = state.bots.definitions().get(&resource_id(params, "bot_id")?)?;
            Ok(json!({ "bot_id": bot.bot_id, "event_version": bot.event_version, "draft": bot.draft }))
        }
        "bot.draft.patch" => {
            let bot_id = resource_id(params, "bot_id")?;
            let definition = decode(params, "definition")?;
            let bot = state.bots.definitions().get(&bot_id)?;
            value(
                state
                    .bots
                    .definitions()
                    .replace_draft(ReplaceDraft {
                        bot_id: &bot_id,
                        expected_event_version: expected(params)?,
                        expected_draft_version: params
                            .get("expected_draft_version")
                            .and_then(Value::as_u64)
                            .ok_or("missing expected_draft_version")?,
                        definition: &definition,
                        actor: owner(),
                        trace: trace(),
                        idempotency_key: idempotency(params)?,
                        at_ms: now(),
                    })
                    .inspect(|result| debug_assert!(result.event_version >= bot.event_version))?,
            )
        }
        "bot.validate" => {
            let builder_id = resource_id(params, "builder_session_id")?;
            let builder = state.bots.builder().get(&builder_id)?;
            let draft = builder.draft.as_ref().ok_or("Builder draft is missing")?;
            let runtime = state.ready_active_runtime().await?;
            let mrm = runtime.mrm();
            let mut roles = std::collections::BTreeSet::new();
            if mrm.role(draft.definition.mrm_role.as_str()).is_some() {
                roles.insert(draft.definition.mrm_role.clone());
            }
            let connectors = runtime
                .mcp()
                .status()
                .into_iter()
                .filter_map(|status| crate::core::identity::ResourceId::parse(status.name).ok())
                .collect();
            value(state.bots.validate_builder(&builder_id, &roles, &connectors, idempotency(params)?, now())?)
        }
        "bot.publish" => {
            let builder_id = resource_id(params, "builder_session_id")?;
            let hash =
                crate::core::identity::ContentHash::parse(params.get("review_hash").and_then(Value::as_str).ok_or("missing review_hash")?)?;
            value(state.bots.publish_validated_builder(&builder_id, &hash, idempotency(params)?, now())?)
        }
        "bot.pause" => lifecycle(params, state, LifecycleChange::Pause),
        "bot.resume" => lifecycle(params, state, LifecycleChange::Resume),
        "bot.archive" => lifecycle(params, state, LifecycleChange::Archive),
        "bot.trash" => lifecycle(params, state, LifecycleChange::Trash),
        "bot.restore" => lifecycle(params, state, LifecycleChange::Restore),
        _ => Err(format!("unknown Bot definition method: {method}").into()),
    }
}

fn lifecycle(params: &Value, state: &Arc<AppState>, change: LifecycleChange<'_>) -> RpcResult<Value> {
    let bot_id = resource_id(params, "bot_id")?;
    value(state.bots.definitions().change_lifecycle(ChangeLifecycle {
        bot_id: &bot_id,
        expected_event_version: expected(params)?,
        change,
        actor: owner(),
        trace: trace(),
        idempotency_key: idempotency(params)?,
        at_ms: now(),
    })?)
}
