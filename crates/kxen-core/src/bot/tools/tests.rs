use super::*;
use crate::agent::capability::CapabilitySet;
use crate::agent::dcp::ProviderNeutralPart;
use crate::bot::run::{RunCommand, RunTrigger, RunTriggerKind, RunWrite};
use crate::bot::system::{BotSystem, QueueRun};
use crate::bot::{BotDefinition, CreateBot, PublishBot};
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, TraceContext};

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::parse(value).unwrap()
}

#[test]
fn artifact_tool_commits_immutable_owned_content_and_links_run() {
    let root = std::env::temp_dir().join(format!("kxen-bot-artifact-tool-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let bot_id = id("bot_artifact_owner");
    let mut definition = BotDefinition::empty("Artifact Bot");
    definition.objective = "Create evidence".into();
    definition.instructions = "Commit the final evidence".into();
    definition.success_criteria = vec!["Artifact exists".into()];
    definition.output_contract.description = "Artifact".into();
    definition.capabilities = CapabilitySet::new([id("bot_artifact")]);
    let created = system
        .definitions()
        .create(CreateBot {
            bot_id: &bot_id,
            definition: &definition,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key("idem_artifact_bot"),
            at_ms: 1,
        })
        .unwrap();
    let draft = created.draft.unwrap();
    system
        .definitions()
        .publish(PublishBot {
            bot_id: &bot_id,
            expected_event_version: created.event_version,
            expected_draft_version: draft.version,
            expected_content_hash: &draft.content_hash,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key("idem_artifact_publish"),
            at_ms: 2,
        })
        .unwrap();
    let run_id = id("brun_artifact");
    let queued = system
        .queue_run(QueueRun {
            run_id: run_id.clone(),
            bot_id: bot_id.clone(),
            revision_id: None,
            trigger: RunTrigger { kind: RunTriggerKind::Manual, source_id: None, occurrence_id: None },
            input: vec![ProviderNeutralPart::Text { text: "create".into() }],
            conversation_id: None,
            task_id: None,
            budget_override: None,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key("idem_artifact_run"),
            at_ms: 3,
        })
        .unwrap();
    system
        .runs()
        .execute(RunWrite {
            run_id: run_id.clone(),
            expected_version: queued.event_version,
            idempotency_key: key("idem_artifact_start"),
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            command: RunCommand::Start { at_ms: 4 },
        })
        .unwrap();
    let args = serde_json::json!({
        "action": "commit",
        "display_name": "evidence.txt",
        "media_type": "text/plain",
        "content": "verified"
    });
    let first = artifact::execute(&system, &run_id, &args).unwrap();
    let second = artifact::execute(&system, &run_id, &args).unwrap();
    assert_eq!(first, second);
    let run = system.runs().get(&run_id).unwrap();
    assert_eq!(run.artifacts.len(), 1);
    let content = system
        .artifacts()
        .read_verified(
            &run.artifacts[0].artifact_id,
            &crate::core::artifact::ArtifactAccess { actor: ActorRef::Bot { id: bot_id }, conversation_id: None },
        )
        .unwrap();
    assert_eq!(content, b"verified");
    std::fs::remove_dir_all(root).ok();
}
