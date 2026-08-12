use super::*;
use crate::agent::capability::CapabilitySet;
use crate::agent::dcp::ProviderNeutralPart;
use crate::bot::conversation::{
    BotParticipant, ConversationCommand, ConversationKind, Message, MessageKind, MessagePart, NewTask, TaskStatus,
};
use crate::bot::run::{RunCommand, RunTrigger, RunTriggerKind, RunWrite};
use crate::bot::system::{BotSystem, ConversationMutation, PostConversation, QueueRun};
use crate::bot::{BotDefinition, CreateBot, PublishBot};
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, TraceContext};
use std::collections::BTreeSet;

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::parse(value).unwrap()
}

fn publish_bot(system: &BotSystem, bot_id: &ResourceId, name: &str, capabilities: CapabilitySet, suffix: &str) {
    let mut definition = BotDefinition::empty(name);
    definition.objective = "Complete assigned work".into();
    definition.instructions = "Use durable Bot tools".into();
    definition.success_criteria = vec!["Work is settled".into()];
    definition.output_contract.description = "Settled work".into();
    definition.capabilities = capabilities;
    definition.communication.allow_groups = true;
    let created = system
        .definitions()
        .create(CreateBot {
            bot_id,
            definition: &definition,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key(&format!("idem_tool_create_{suffix}")),
            at_ms: 1,
        })
        .unwrap();
    let draft = created.draft.unwrap();
    system
        .definitions()
        .publish(PublishBot {
            bot_id,
            expected_event_version: created.event_version,
            expected_draft_version: draft.version,
            expected_content_hash: &draft.content_hash,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key(&format!("idem_tool_publish_{suffix}")),
            at_ms: 2,
        })
        .unwrap();
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

#[test]
fn workflow_approval_pauses_run_without_fake_operation_and_denial_settles_task() {
    let root = std::env::temp_dir().join(format!("kxen-bot-task-approval-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let worker = id("bot_task_approval_worker");
    let moderator = id("bot_task_approval_moderator");
    publish_bot(&system, &worker, "Worker", CapabilitySet::new([id("bot_task")]), "task_worker");
    publish_bot(&system, &moderator, "Moderator", CapabilitySet::default(), "task_moderator");

    let conversation_id = id("bconv_task_approval");
    let created = system
        .mutate_conversation(ConversationMutation {
            conversation_id: conversation_id.clone(),
            expected_version: 0,
            actor: ActorRef::Owner,
            command: ConversationCommand::Create {
                conversation_id: conversation_id.clone(),
                kind: ConversationKind::BotGroup,
                members: vec![
                    BotParticipant { bot_id: worker.clone(), joined_at_seq: 0, history_visible_from_seq: 0, active: true },
                    BotParticipant { bot_id: moderator.clone(), joined_at_seq: 0, history_visible_from_seq: 0, active: true },
                ],
                moderator_bot_id: Some(moderator),
                at_ms: 3,
            },
            trace: TraceContext::default(),
            idempotency_key: key("idem_task_approval_group"),
        })
        .unwrap();
    let task_id = id("btask_workflow_approval");
    let message = Message {
        message_id: id("bmsg_task_approval"),
        conversation_id: conversation_id.clone(),
        actor: ActorRef::Owner,
        kind: MessageKind::Instruction,
        parts: vec![MessagePart::Text { text: "prepare a controlled report".into() }],
        mentions: BTreeSet::from([worker.clone()]),
        everyone: false,
        target_bot_id: None,
        reply_to_message_id: None,
        task_id: Some(task_id.clone()),
        origin_run_id: None,
        causation_id: None,
        correlation_id: None,
        delegation_depth: 0,
        hop_count: 0,
        created_at_ms: 4,
    };
    system
        .post_conversation(PostConversation {
            conversation_id: conversation_id.clone(),
            expected_version: created.event_version,
            actor: ActorRef::Owner,
            message,
            task: Some(NewTask {
                task_id: task_id.clone(),
                owner_bot_id: worker.clone(),
                title: "Controlled report".into(),
                input: vec![MessagePart::Text { text: "input".into() }],
                expected_output: "approved report".into(),
                parent_task_id: None,
                budget: Default::default(),
            }),
            trace: TraceContext::default(),
            idempotency_key: key("idem_task_approval_post"),
            at_ms: 4,
        })
        .unwrap();
    let run_id = id("brun_workflow_approval");
    let queued = system
        .queue_run(QueueRun {
            run_id: run_id.clone(),
            bot_id: worker.clone(),
            revision_id: None,
            trigger: RunTrigger { kind: RunTriggerKind::Manual, source_id: None, occurrence_id: None },
            input: vec![ProviderNeutralPart::Text { text: "work".into() }],
            conversation_id: Some(conversation_id.clone()),
            task_id: Some(task_id.clone()),
            budget_override: None,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key("idem_task_approval_run"),
            at_ms: 5,
        })
        .unwrap();
    let running = system
        .runs()
        .execute(RunWrite {
            run_id: run_id.clone(),
            expected_version: queued.event_version,
            idempotency_key: key("idem_task_approval_start"),
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            command: RunCommand::Start { at_ms: 6 },
        })
        .unwrap();
    system.settle_run(&running, 6).unwrap();

    task::execute(&system, &run_id, &serde_json::json!({ "action": "need_approval", "reason": "approve report release" })).unwrap();
    let paused = system.runs().get(&run_id).unwrap();
    assert_eq!(paused.status, crate::bot::run::RunStatus::ApprovalRequired);
    assert_eq!(paused.approval.as_ref().unwrap().operation_id, None);
    assert_eq!(system.conversations().get(&conversation_id).unwrap().tasks[&task_id].status, TaskStatus::ApprovalRequired);

    let rejected = system
        .runs()
        .execute(RunWrite {
            run_id: run_id.clone(),
            expected_version: paused.event_version,
            idempotency_key: key("idem_task_approval_deny"),
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            command: RunCommand::ResolveApproval {
                approval_id: paused.approval.unwrap().approval_id,
                decision: crate::bot::run::ApprovalDecision::Denied,
                at_ms: 7,
            },
        })
        .unwrap();
    system.settle_run(&rejected, 8).unwrap();
    assert_eq!(system.conversations().get(&conversation_id).unwrap().tasks[&task_id].status, TaskStatus::Rejected);
    std::fs::remove_dir_all(root).ok();
}
