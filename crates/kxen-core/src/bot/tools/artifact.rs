use std::collections::BTreeSet;

use crate::bot::run::{ArtifactRef, RunCommand, RunWrite};
use crate::core::artifact::CommitArtifact;
use crate::core::identity::{ActorRef, AggregateKind, AggregateRef, ResourceId, TraceContext};

use super::helpers;

pub(super) fn execute(system: &crate::bot::system::BotSystem, run_id: &ResourceId, args: &serde_json::Value) -> Result<String, String> {
    let run = helpers::run(system, run_id)?;
    if helpers::required(args, "action")? != "commit" {
        return Err("unknown bot_artifact action".into());
    }
    let content = helpers::required(args, "content")?.as_bytes();
    let display_name = helpers::required(args, "display_name")?;
    let media_type = helpers::required(args, "media_type")?;
    let artifact_id = helpers::stable_id("bartifact", run_id, args)?;
    let shared_with_conversations = if args.get("share_with_conversation").and_then(serde_json::Value::as_bool).unwrap_or(false) {
        [run.spec.conversation_id.clone().ok_or("cannot share an Artifact without a Conversation-bound Run")?]
            .into_iter()
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };
    let manifest = system
        .artifacts()
        .commit(CommitArtifact {
            artifact_id: &artifact_id,
            owner: AggregateRef { kind: AggregateKind::Bot, id: run.spec.bot_id.clone() },
            display_name,
            media_type,
            content,
            shared_with_conversations,
            created_at_ms: run.created_at_ms,
        })
        .map_err(|error| error.to_string())?;
    let artifact = ArtifactRef {
        artifact_id,
        display_name: manifest.display_name,
        media_type: manifest.media_type,
        content_hash: manifest.content_hash,
        size_bytes: manifest.size_bytes,
    };
    let current = system.runs().get(run_id).map_err(|error| error.to_string())?;
    let state = system
        .runs()
        .execute(RunWrite {
            run_id: run_id.clone(),
            expected_version: current.event_version,
            idempotency_key: helpers::stable_key("bot_artifact", run_id, args)?,
            actor: ActorRef::Bot { id: run.spec.bot_id },
            trace: TraceContext::default(),
            command: RunCommand::CommitArtifact { artifact: artifact.clone(), at_ms: run.created_at_ms },
        })
        .map_err(|error| error.to_string())?;
    serde_json::to_string(&serde_json::json!({ "artifact": artifact, "run_version": state.event_version }))
        .map_err(|error| error.to_string())
}
