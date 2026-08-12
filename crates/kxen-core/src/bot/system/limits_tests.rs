use super::*;
use crate::bot::conversation::{BotParticipant, Message, MessageKind, MessagePart};
use crate::bot::run::{RunCommand, RunWrite};
use crate::bot::{BotDefinition, CreateBot, PublishBot};
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, TraceContext};

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::parse(value).unwrap()
}

fn publish(system: &BotSystem, bot_id: &ResourceId, suffix: &str) {
    let mut definition = BotDefinition::empty(suffix);
    definition.objective = "Complete work".into();
    definition.instructions = "Return evidence".into();
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
            idempotency_key: key(&format!("idem_create_{suffix}")),
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
            idempotency_key: key(&format!("idem_publish_{suffix}")),
            at_ms: 2,
        })
        .unwrap();
}

fn instruction(conversation_id: &ResourceId, message_id: &str) -> Message {
    Message {
        message_id: id(message_id),
        conversation_id: conversation_id.clone(),
        actor: ActorRef::Owner,
        kind: MessageKind::Instruction,
        parts: vec![MessagePart::Text { text: "work".into() }],
        mentions: Default::default(),
        everyone: false,
        target_bot_id: None,
        reply_to_message_id: None,
        task_id: None,
        origin_run_id: None,
        causation_id: None,
        correlation_id: None,
        delegation_depth: 0,
        hop_count: 0,
        created_at_ms: 4,
    }
}

#[test]
fn platform_limits_and_one_active_run_per_bot_conversation_are_enforced() {
    let root = std::env::temp_dir().join(format!("kxen-bot-system-limits-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let bot_a = id("bot_serial_a");
    let bot_b = id("bot_serial_b");
    publish(&system, &bot_a, "serial_a");
    publish(&system, &bot_b, "serial_b");
    let conversation_id = id("bconv_serial");
    let mut conversation = system
        .mutate_conversation(ConversationMutation {
            conversation_id: conversation_id.clone(),
            expected_version: 0,
            actor: ActorRef::Owner,
            command: ConversationCommand::Create {
                conversation_id: conversation_id.clone(),
                kind: ConversationKind::BotGroup,
                members: [&bot_a, &bot_b]
                    .into_iter()
                    .map(|bot_id| BotParticipant { bot_id: bot_id.clone(), joined_at_seq: 1, history_visible_from_seq: 1, active: true })
                    .collect(),
                moderator_bot_id: Some(bot_a.clone()),
                at_ms: 3,
            },
            trace: TraceContext::default(),
            idempotency_key: key("idem_serial_group"),
        })
        .unwrap();
    for (message_id, idempotency) in [("bmsg_serial_one", "idem_serial_one"), ("bmsg_serial_two", "idem_serial_two")] {
        conversation = system
            .post_conversation(PostConversation {
                conversation_id: conversation_id.clone(),
                expected_version: conversation.event_version,
                actor: ActorRef::Owner,
                message: instruction(&conversation_id, message_id),
                task: None,
                trace: TraceContext::default(),
                idempotency_key: key(idempotency),
                at_ms: 4,
            })
            .unwrap();
    }
    let first = system.dispatch_next_delivery(5).unwrap().unwrap().run;
    assert_eq!(first.spec.permission.budget.max_child_tasks, Some(32));
    assert_eq!(first.spec.permission.budget.max_delegation_depth, Some(8));
    assert_eq!(first.spec.permission.budget.max_message_hops, Some(32));
    assert!(system.dispatch_next_delivery(6).unwrap().is_none());
    assert_eq!(system.runs().list().unwrap().len(), 1);
    system
        .runs()
        .execute(RunWrite {
            run_id: first.spec.run_id.clone(),
            expected_version: first.event_version,
            idempotency_key: key("idem_cancel_serial"),
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            command: RunCommand::Cancel { reason: "test complete".into(), usage: Default::default(), at_ms: 7 },
        })
        .unwrap();
    let second = system.dispatch_next_delivery(8).unwrap().unwrap().run;
    assert_eq!(second.spec.bot_id, bot_a);
    assert_eq!(system.runs().list().unwrap().len(), 2);
    std::fs::remove_dir_all(root).ok();
}
