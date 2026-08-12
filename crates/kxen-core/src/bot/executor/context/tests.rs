use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::bot::conversation::{BotParticipant, ConversationCommand, ConversationKind, Message, MessageKind, MessagePart, NewTask};
use crate::bot::memory::{MemoryCommand, MemoryItem, MemoryKind, MemoryWrite};
use crate::bot::run::{ArtifactRef, RunTrigger, RunTriggerKind};
use crate::bot::system::{ConversationMutation, PostConversation, QueueRun};
use crate::bot::{BotDefinition, CreateBot, PublishBot};
use crate::core::identity::{ActorRef, AggregateKind, AggregateRef, ContentHash, IdempotencyKey, ResourceId, TraceContext};

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::parse(value).unwrap()
}

fn publish(system: &BotSystem, bot_id: &ResourceId, suffix: &str) {
    let mut definition = BotDefinition::empty(format!("Bot {suffix}"));
    definition.objective = "Produce evidence".into();
    definition.instructions = "Use scoped context".into();
    definition.success_criteria = vec!["Evidence exists".into()];
    definition.output_contract.description = "Evidence".into();
    definition.communication.allow_groups = true;
    let created = system
        .definitions()
        .create(CreateBot {
            bot_id,
            definition: &definition,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key(&format!("idem_context_create_{suffix}")),
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
            idempotency_key: key(&format!("idem_context_publish_{suffix}")),
            at_ms: 2,
        })
        .unwrap();
}

#[test]
fn compose_scopes_memory_conversation_task_and_input() {
    let root = std::env::temp_dir().join(format!("kxen-bot-context-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let worker = id("bot_context_worker");
    let moderator = id("bot_context_moderator");
    publish(&system, &worker, "worker");
    publish(&system, &moderator, "moderator");
    system
        .memory()
        .execute(MemoryWrite {
            bot_id: worker.clone(),
            expected_version: 0,
            idempotency_key: key("idem_context_memory"),
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            command: MemoryCommand::Create {
                item: MemoryItem {
                    item_id: id("memory_context_rule"),
                    kind: MemoryKind::Procedure,
                    content: "Verify every total".into(),
                    provenance: AggregateRef { kind: AggregateKind::Bot, id: worker.clone() },
                    version: 1,
                    created_at_ms: 3,
                    updated_at_ms: 3,
                },
            },
        })
        .unwrap();

    let conversation_id = id("bconv_context_group");
    let created = system
        .mutate_conversation(ConversationMutation {
            conversation_id: conversation_id.clone(),
            expected_version: 0,
            actor: ActorRef::Owner,
            command: ConversationCommand::Create {
                conversation_id: conversation_id.clone(),
                kind: ConversationKind::BotGroup,
                members: vec![
                    BotParticipant { bot_id: moderator.clone(), joined_at_seq: 0, history_visible_from_seq: 0, active: true },
                    BotParticipant { bot_id: worker.clone(), joined_at_seq: 0, history_visible_from_seq: 0, active: true },
                ],
                moderator_bot_id: Some(moderator),
                at_ms: 4,
            },
            trace: TraceContext::default(),
            idempotency_key: key("idem_context_group"),
        })
        .unwrap();
    let task_id = id("btask_context_report");
    let artifact = ArtifactRef {
        artifact_id: id("artifact_context"),
        display_name: "evidence.txt".into(),
        media_type: "text/plain".into(),
        content_hash: ContentHash::from_bytes(b"evidence"),
        size_bytes: 8,
    };
    let message = Message {
        message_id: id("bmsg_context_instruction"),
        conversation_id: conversation_id.clone(),
        actor: ActorRef::Owner,
        kind: MessageKind::Instruction,
        parts: vec![
            MessagePart::Text { text: "Prepare report".into() },
            MessagePart::Data { schema_id: id("report_input"), fields: BTreeMap::from([("period".into(), "weekly".into())]) },
            MessagePart::ArtifactRef { artifact },
        ],
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
        created_at_ms: 5,
    };
    let posted = system
        .post_conversation(PostConversation {
            conversation_id: conversation_id.clone(),
            expected_version: created.event_version,
            actor: ActorRef::Owner,
            message,
            task: Some(NewTask {
                task_id: task_id.clone(),
                owner_bot_id: worker.clone(),
                title: "Weekly report".into(),
                input: vec![MessagePart::Text { text: "Prepare report".into() }],
                expected_output: "Verified evidence".into(),
                parent_task_id: None,
                budget: Default::default(),
            }),
            trace: TraceContext::default(),
            idempotency_key: key("idem_context_post"),
            at_ms: 5,
        })
        .unwrap();
    assert_eq!(posted.messages.len(), 1);

    let run = system
        .queue_run(QueueRun {
            run_id: id("brun_context_scoped"),
            bot_id: worker,
            revision_id: None,
            trigger: RunTrigger { kind: RunTriggerKind::Manual, source_id: None, occurrence_id: None },
            input: vec![ProviderNeutralPart::Text { text: "Current request".into() }],
            conversation_id: Some(conversation_id),
            task_id: Some(task_id),
            budget_override: None,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key("idem_context_run"),
            at_ms: 6,
        })
        .unwrap();
    let frame = compose(&system, &run).unwrap();
    let layers = frame.segments.iter().map(|segment| segment.layer).collect::<Vec<_>>();
    assert!(layers.contains(&ContextLayer::Definition));
    assert!(layers.contains(&ContextLayer::Memory));
    assert!(layers.contains(&ContextLayer::Conversation));
    assert!(layers.contains(&ContextLayer::CollaborationTask));
    assert!(layers.contains(&ContextLayer::NewInput));

    let frame_json = serde_json::to_string(&frame).unwrap();
    let mut recorded_run = run;
    recorded_run.turns.push(crate::agent::dcp::TurnRecord {
        record_id: id("turn_context_frame"),
        kind: crate::agent::dcp::TurnRecordKind::Request,
        parts: vec![ProviderNeutralPart::Data {
            schema_id: id("dcp_context_frame"),
            fields: BTreeMap::from([("frame_json".into(), frame_json)]),
        }],
        created_at_ms: 7,
    });
    assert_eq!(recorded(&recorded_run).unwrap(), Some(frame));
    std::fs::remove_dir_all(root).ok();
}
