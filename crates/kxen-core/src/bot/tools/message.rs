use std::collections::BTreeSet;

use crate::bot::conversation::{Message, MessageKind, MessagePart};
use crate::bot::system::PostConversation;
use crate::core::identity::{ActorRef, AggregateKind, ResourceId, TraceContext};

use super::helpers;

pub(super) fn execute(system: &crate::bot::system::BotSystem, run_id: &ResourceId, args: &serde_json::Value) -> Result<String, String> {
    let run = helpers::run(system, run_id)?;
    let conversation = helpers::conversation(system, &run)?;
    let action = helpers::required(args, "action")?;
    let (base_depth, base_hops) = helpers::lineage(&conversation, &run);
    let (kind, target, parts, depth, hops) = match action {
        "send_request" => {
            let target = ResourceId::parse(helpers::required(args, "target_bot_id")?)?;
            let parts = message_parts(args)?;
            let depth = base_depth.saturating_add(1);
            let hops = base_hops.saturating_add(1);
            if let Err(reason) = helpers::check_lineage(&run, depth, hops, helpers::outbound_request_count(&conversation, &run)) {
                helpers::record_limit_rejected(system, &run, &conversation, args, &reason)?;
                return Err(reason);
            }
            (MessageKind::Request, Some(target), parts, depth, hops)
        }
        "send_response" => {
            let target = response_target(args, &conversation, &run)?;
            (MessageKind::Response, target, message_parts(args)?, base_depth, base_hops.saturating_add(1))
        }
        "post_notice" => (
            MessageKind::Notice,
            None,
            vec![MessagePart::Text { text: helpers::required(args, "text")?.to_string() }],
            base_depth,
            base_hops,
        ),
        "post_artifact" => {
            let artifact_id = ResourceId::parse(helpers::required(args, "artifact_id")?)?;
            let manifest = system.artifacts().load(&artifact_id).map_err(|error| error.to_string())?;
            if manifest.owner != (crate::core::identity::AggregateRef { kind: AggregateKind::Bot, id: run.spec.bot_id.clone() }) {
                return Err("Artifact is not owned by this Bot".into());
            }
            if !manifest.shared_with_conversations.contains(&conversation.conversation_id) {
                return Err("Artifact is not shared with this Conversation".into());
            }
            let artifact = crate::bot::run::ArtifactRef {
                artifact_id,
                display_name: manifest.display_name,
                media_type: manifest.media_type,
                content_hash: manifest.content_hash,
                size_bytes: manifest.size_bytes,
            };
            (MessageKind::Artifact, None, vec![MessagePart::ArtifactRef { artifact }], base_depth, base_hops)
        }
        _ => return Err(format!("unknown bot_message action: {action}")),
    };
    helpers::check_lineage(&run, depth, hops, 0)?;
    let message_id = helpers::stable_id("bmsg", run_id, args)?;
    let message = Message {
        message_id: message_id.clone(),
        conversation_id: conversation.conversation_id.clone(),
        actor: ActorRef::Bot { id: run.spec.bot_id.clone() },
        kind,
        parts,
        mentions: BTreeSet::new(),
        everyone: false,
        target_bot_id: target,
        reply_to_message_id: helpers::source_message(&conversation, &run).map(|message| message.message_id.clone()),
        task_id: helpers::optional_id(args, "task_id")?.or(run.spec.task_id.clone()),
        origin_run_id: Some(run.spec.run_id.clone()),
        causation_id: run.spec.trigger.source_id.clone(),
        correlation_id: run.spec.task_id.clone(),
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
            task: None,
            trace: TraceContext::default(),
            idempotency_key: helpers::stable_key("bot_message", run_id, args)?,
            at_ms: crate::core::shared::now_ms(),
        })
        .map_err(|error| error.to_string())?;
    serde_json::to_string(&serde_json::json!({ "message_id": message_id, "conversation_version": state.event_version }))
        .map_err(|error| error.to_string())
}

fn message_parts(args: &serde_json::Value) -> Result<Vec<MessagePart>, String> {
    match (args.get("text").and_then(serde_json::Value::as_str), args.get("fields")) {
        (Some(text), None) if !text.trim().is_empty() => Ok(vec![MessagePart::Text { text: text.into() }]),
        (None, Some(fields)) => {
            let schema_id = ResourceId::parse(helpers::required(args, "schema_id")?)?;
            let fields =
                serde_json::from_value(fields.clone()).map_err(|error| format!("invalid structured Bot message fields: {error}"))?;
            Ok(vec![MessagePart::Data { schema_id, fields }])
        }
        _ => Err("Bot request or response requires exactly one of text or structured fields".into()),
    }
}

fn response_target(
    args: &serde_json::Value,
    conversation: &crate::bot::conversation::ConversationState,
    run: &crate::bot::run::BotRunState,
) -> Result<Option<ResourceId>, String> {
    if let Some(target) = helpers::optional_id(args, "target_bot_id")? {
        return Ok(Some(target));
    }
    let actor = run
        .spec
        .task_id
        .as_ref()
        .and_then(|task_id| conversation.tasks.get(task_id))
        .map(|task| &task.originator)
        .or_else(|| helpers::source_message(conversation, run).map(|message| &message.actor));
    Ok(match actor {
        Some(ActorRef::Bot { id }) => Some(id.clone()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_peer_message_requires_schema_and_string_fields() {
        let parts = message_parts(&serde_json::json!({
            "schema_id": "bot_contract_input",
            "fields": { "topic": "weekly report" }
        }))
        .unwrap();
        assert!(matches!(&parts[0], MessagePart::Data { fields, .. } if fields["topic"] == "weekly report"));
        assert!(message_parts(&serde_json::json!({ "text": "x", "fields": { "topic": "x" }, "schema_id": "s" })).is_err());
    }
}
