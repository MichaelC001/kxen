use super::*;
use crate::bot::conversation::{BotParticipant, Message, MessageKind, MessagePart};
use crate::bot::{BotDefinition, CreateBot, PublishBot};
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, TraceContext};
use std::collections::BTreeSet;

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::parse(value).unwrap()
}

fn definition(name: &str) -> BotDefinition {
    let mut definition = BotDefinition::empty(name);
    definition.objective = "Complete assigned work".into();
    definition.instructions = "Use only approved capabilities and return evidence.".into();
    definition.success_criteria = vec!["Evidence is returned".into()];
    definition.output_contract.description = "Verified evidence".into();
    definition.communication.allow_groups = true;
    definition
}

fn publish(system: &BotSystem, bot_id: &ResourceId, definition: &BotDefinition, suffix: &str) {
    let created = system
        .definitions()
        .create(CreateBot {
            bot_id,
            definition,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key(&format!("idem_create_{suffix}")),
            at_ms: 1,
        })
        .unwrap();
    let draft = created.draft.as_ref().unwrap();
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

fn participant(bot_id: &ResourceId) -> BotParticipant {
    BotParticipant { bot_id: bot_id.clone(), joined_at_seq: 1, history_visible_from_seq: 1, active: true }
}

#[test]
fn group_instruction_dispatches_one_durable_moderator_run_without_permission_union() {
    let root = std::env::temp_dir().join(format!("kxen-bot-system-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let bot_a = id("bot_a");
    let bot_b = id("bot_b");
    let mut definition_a = definition("A");
    definition_a.resources.connectors.insert(id("connector_a"));
    let mut definition_b = definition("B");
    definition_b.resources.connectors.insert(id("connector_b"));
    publish(&system, &bot_a, &definition_a, "a");
    publish(&system, &bot_b, &definition_b, "b");

    let conversation_id = id("bconv_system");
    let group = system
        .mutate_conversation(ConversationMutation {
            conversation_id: conversation_id.clone(),
            expected_version: 0,
            actor: ActorRef::Owner,
            command: ConversationCommand::Create {
                conversation_id: conversation_id.clone(),
                kind: ConversationKind::BotGroup,
                members: vec![participant(&bot_a), participant(&bot_b)],
                moderator_bot_id: Some(bot_a.clone()),
                at_ms: 3,
            },
            trace: TraceContext::default(),
            idempotency_key: key("idem_group"),
        })
        .unwrap();
    let message = Message {
        message_id: id("bmsg_instruction"),
        conversation_id: conversation_id.clone(),
        actor: ActorRef::Owner,
        kind: MessageKind::Instruction,
        parts: vec![MessagePart::Text { text: "prepare evidence".into() }],
        mentions: BTreeSet::new(),
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
    };
    system
        .post_conversation(PostConversation {
            conversation_id: conversation_id.clone(),
            expected_version: group.event_version,
            actor: ActorRef::Owner,
            message,
            task: None,
            trace: TraceContext::default(),
            idempotency_key: key("idem_post"),
            at_ms: 4,
        })
        .unwrap();
    let dispatched = system.dispatch_next_delivery(5).unwrap().unwrap();
    assert_eq!(dispatched.run.spec.bot_id, bot_a);
    assert!(dispatched.run.spec.permission.resources.connectors.contains(&id("connector_a")));
    assert!(!dispatched.run.spec.permission.resources.connectors.contains(&id("connector_b")));
    assert!(system.conversations().get(&conversation_id).unwrap().deliveries.in_flight.is_none());
    assert!(system.dispatch_next_delivery(6).unwrap().is_none());
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn direct_open_policy_requires_explicit_peer_allowlist() {
    let root = std::env::temp_dir().join(format!("kxen-bot-system-direct-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let bot_a = id("bot_direct_a");
    let bot_b = id("bot_direct_b");
    let mut a = definition("A");
    a.communication.allow_direct = true;
    let mut b = definition("B");
    b.communication.allow_direct = true;
    publish(&system, &bot_a, &a, "direct_a");
    publish(&system, &bot_b, &b, "direct_b");
    let conversation_id = crate::bot::conversation::direct_conversation_id(&bot_a, &bot_b).unwrap();
    let open = system.mutate_conversation(ConversationMutation {
        conversation_id: conversation_id.clone(),
        expected_version: 0,
        actor: ActorRef::Owner,
        command: ConversationCommand::Create {
            conversation_id: conversation_id.clone(),
            kind: ConversationKind::BotDirect,
            members: vec![participant(&bot_a), participant(&bot_b)],
            moderator_bot_id: None,
            at_ms: 3,
        },
        trace: TraceContext::default(),
        idempotency_key: key("idem_direct"),
    });
    assert!(matches!(open, Err(BotSystemError::Rejected(_))));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn direct_request_response_settles_without_response_ping_pong() {
    use crate::agent::dcp::ProviderNeutralPart;
    use crate::bot::run::{RunCommand, RunWrite, UsageSummary};

    let root = std::env::temp_dir().join(format!("kxen-bot-system-direct-loop-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let bot_a = id("bot_direct_loop_a");
    let bot_b = id("bot_direct_loop_b");
    let mut a = definition("A");
    a.communication.allow_direct = true;
    a.communication.allowed_peers.insert(bot_b.clone());
    let mut b = definition("B");
    b.communication.allow_direct = true;
    b.communication.allowed_peers.insert(bot_a.clone());
    publish(&system, &bot_a, &a, "loop_a");
    publish(&system, &bot_b, &b, "loop_b");
    let conversation_id = crate::bot::conversation::direct_conversation_id(&bot_a, &bot_b).unwrap();
    let conversation = system
        .mutate_conversation(ConversationMutation {
            conversation_id: conversation_id.clone(),
            expected_version: 0,
            actor: ActorRef::Owner,
            command: ConversationCommand::Create {
                conversation_id: conversation_id.clone(),
                kind: ConversationKind::BotDirect,
                members: vec![participant(&bot_a), participant(&bot_b)],
                moderator_bot_id: None,
                at_ms: 3,
            },
            trace: TraceContext::default(),
            idempotency_key: key("idem_direct_loop"),
        })
        .unwrap();
    let request = Message {
        message_id: id("bmsg_direct_loop"),
        conversation_id: conversation_id.clone(),
        actor: ActorRef::Bot { id: bot_a.clone() },
        kind: MessageKind::Request,
        parts: vec![MessagePart::Text { text: "verify".into() }],
        mentions: BTreeSet::new(),
        everyone: false,
        target_bot_id: Some(bot_b.clone()),
        reply_to_message_id: None,
        task_id: None,
        origin_run_id: Some(id("brun_origin")),
        causation_id: None,
        correlation_id: None,
        delegation_depth: 1,
        hop_count: 1,
        created_at_ms: 4,
    };
    system
        .post_conversation(PostConversation {
            conversation_id: conversation_id.clone(),
            expected_version: conversation.event_version,
            actor: request.actor.clone(),
            message: request,
            task: None,
            trace: TraceContext::default(),
            idempotency_key: key("idem_direct_request"),
            at_ms: 4,
        })
        .unwrap();

    let first = system.dispatch_next_delivery(5).unwrap().unwrap().run;
    let running = system
        .runs()
        .execute(RunWrite {
            run_id: first.spec.run_id.clone(),
            expected_version: first.event_version,
            idempotency_key: key("idem_direct_start_b"),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RunCommand::Start { at_ms: 6 },
        })
        .unwrap();
    let completed = system
        .runs()
        .execute(RunWrite {
            run_id: running.spec.run_id.clone(),
            expected_version: running.event_version,
            idempotency_key: key("idem_direct_complete_b"),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RunCommand::Complete {
                result: vec![ProviderNeutralPart::Text { text: "verified".into() }],
                usage: UsageSummary::default(),
                at_ms: 7,
            },
        })
        .unwrap();
    system.settle_run(&completed, 8).unwrap();

    let second = system.dispatch_next_delivery(9).unwrap().unwrap().run;
    assert_eq!(second.spec.bot_id, bot_a);
    let running = system
        .runs()
        .execute(RunWrite {
            run_id: second.spec.run_id.clone(),
            expected_version: second.event_version,
            idempotency_key: key("idem_direct_start_a"),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RunCommand::Start { at_ms: 10 },
        })
        .unwrap();
    let completed = system
        .runs()
        .execute(RunWrite {
            run_id: running.spec.run_id.clone(),
            expected_version: running.event_version,
            idempotency_key: key("idem_direct_complete_a"),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RunCommand::Complete {
                result: vec![ProviderNeutralPart::Text { text: "received".into() }],
                usage: UsageSummary::default(),
                at_ms: 11,
            },
        })
        .unwrap();
    system.settle_run(&completed, 12).unwrap();

    let final_state = system.conversations().get(&conversation_id).unwrap();
    assert_eq!(final_state.messages.last().unwrap().kind, MessageKind::Notice);
    assert!(system.dispatch_next_delivery(13).unwrap().is_none());
    std::fs::remove_dir_all(root).ok();
}
