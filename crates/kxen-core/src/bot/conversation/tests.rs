use super::*;
use crate::bot::run::{PermissionSnapshot, RunCommand, RunRepository, RunSpec, RunTrigger, RunTriggerKind, RunWrite};
use crate::core::delivery::DeliveryStatus;
use crate::core::identity::{ActorRef, ContentHash, IdempotencyKey, ResourceId, TraceContext};
use std::collections::{BTreeMap, BTreeSet};

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::parse(value).unwrap()
}

fn repository(name: &str) -> ConversationRepository {
    ConversationRepository::new(std::env::temp_dir().join(format!("kxen-conversation-{name}-{}", uuid::Uuid::new_v4())))
}

fn participant(bot_id: &str) -> BotParticipant {
    BotParticipant { bot_id: id(bot_id), joined_at_seq: 1, history_visible_from_seq: 1, active: true }
}

fn write(
    repo: &ConversationRepository,
    conversation_id: &ResourceId,
    expected: u64,
    idempotency: &str,
    actor: ActorRef,
    command: ConversationCommand,
) -> ConversationState {
    repo.execute(ConversationWrite {
        conversation_id: conversation_id.clone(),
        expected_version: expected,
        idempotency_key: key(idempotency),
        actor,
        trace: TraceContext::default(),
        command,
    })
    .unwrap()
}

fn create_group(repo: &ConversationRepository) -> ConversationState {
    let conversation_id = id("bconv_group");
    write(
        repo,
        &conversation_id,
        0,
        "idem_create_group",
        ActorRef::Owner,
        ConversationCommand::Create {
            conversation_id: conversation_id.clone(),
            kind: ConversationKind::BotGroup,
            members: vec![participant("bot_a"), participant("bot_b"), participant("bot_c")],
            moderator_bot_id: Some(id("bot_a")),
            at_ms: 10,
        },
    )
}

fn message(conversation_id: &ResourceId, message_id: &str, actor: ActorRef, kind: MessageKind) -> Message {
    Message {
        message_id: id(message_id),
        conversation_id: conversation_id.clone(),
        actor,
        kind,
        parts: vec![MessagePart::Text { text: "work item".into() }],
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
        created_at_ms: 20,
    }
}

fn active_delivery_recipients(state: &ConversationState) -> BTreeSet<ResourceId> {
    state
        .deliveries
        .records
        .values()
        .filter(|record| record.status != DeliveryStatus::Acked)
        .filter_map(|record| match &record.envelope.recipient {
            ActorRef::Bot { id } => Some(id.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn group_owner_routing_matrix_is_deterministic() {
    let repo = repository("routing");
    let mut state = create_group(&repo);
    let conversation_id = state.conversation_id.clone();

    state = write(
        &repo,
        &conversation_id,
        state.event_version,
        "idem_no_mention",
        ActorRef::Owner,
        ConversationCommand::Post {
            message: Box::new(message(&conversation_id, "bmsg_one", ActorRef::Owner, MessageKind::Instruction)),
            task: None,
            at_ms: 20,
        },
    );
    assert_eq!(active_delivery_recipients(&state), [id("bot_a")].into_iter().collect());
    assert_eq!(state.message_sequences[&id("bmsg_one")], 2);

    let mut mentioned = message(&conversation_id, "bmsg_two", ActorRef::Owner, MessageKind::Instruction);
    mentioned.mentions = [id("bot_b"), id("bot_c")].into_iter().collect();
    state = write(
        &repo,
        &conversation_id,
        state.event_version,
        "idem_mentions",
        ActorRef::Owner,
        ConversationCommand::Post { message: Box::new(mentioned), task: None, at_ms: 30 },
    );
    assert_eq!(active_delivery_recipients(&state), [id("bot_a"), id("bot_b"), id("bot_c")].into_iter().collect());
    assert_eq!(state.message_sequences[&id("bmsg_two")], 4);

    let mut everyone = message(&conversation_id, "bmsg_three", ActorRef::Owner, MessageKind::Instruction);
    everyone.everyone = true;
    state = write(
        &repo,
        &conversation_id,
        state.event_version,
        "idem_everyone",
        ActorRef::Owner,
        ConversationCommand::Post { message: Box::new(everyone), task: None, at_ms: 40 },
    );
    assert_eq!(state.deliveries.records.len(), 6);
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn bot_notice_is_timeline_only_and_everyone_is_rejected() {
    let repo = repository("bot-routing");
    let state = create_group(&repo);
    let conversation_id = state.conversation_id.clone();
    let notice = message(&conversation_id, "bmsg_notice", ActorRef::Bot { id: id("bot_b") }, MessageKind::Notice);
    let posted = write(
        &repo,
        &conversation_id,
        state.event_version,
        "idem_notice",
        ActorRef::Bot { id: id("bot_b") },
        ConversationCommand::Post { message: Box::new(notice), task: None, at_ms: 20 },
    );
    assert!(posted.deliveries.records.is_empty());
    let mut invalid = message(&conversation_id, "bmsg_invalid", ActorRef::Bot { id: id("bot_b") }, MessageKind::Request);
    invalid.everyone = true;
    invalid.target_bot_id = Some(id("bot_c"));
    let result = repo.execute(ConversationWrite {
        conversation_id: conversation_id.clone(),
        expected_version: posted.event_version,
        idempotency_key: key("idem_invalid"),
        actor: ActorRef::Bot { id: id("bot_b") },
        trace: TraceContext::default(),
        command: ConversationCommand::Post { message: Box::new(invalid), task: None, at_ms: 30 },
    });
    assert!(matches!(result, Err(ConversationError::Rejected(_))));
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn direct_request_delivery_run_task_and_response_complete_async_loop() {
    let repo = repository("direct-loop");
    let conversation_id = direct_conversation_id(&id("bot_a"), &id("bot_b")).unwrap();
    let mut state = write(
        &repo,
        &conversation_id,
        0,
        "idem_direct",
        ActorRef::Owner,
        ConversationCommand::Create {
            conversation_id: conversation_id.clone(),
            kind: ConversationKind::BotDirect,
            members: vec![participant("bot_a"), participant("bot_b")],
            moderator_bot_id: None,
            at_ms: 10,
        },
    );
    let mut request = message(&conversation_id, "bmsg_request", ActorRef::Bot { id: id("bot_a") }, MessageKind::Request);
    request.target_bot_id = Some(id("bot_b"));
    request.task_id = Some(id("btask_one"));
    request.origin_run_id = Some(id("brun_origin"));
    request.delegation_depth = 1;
    request.hop_count = 1;
    state = write(
        &repo,
        &conversation_id,
        state.event_version,
        "idem_request",
        ActorRef::Bot { id: id("bot_a") },
        ConversationCommand::Post {
            message: Box::new(request),
            task: Some(NewTask {
                task_id: id("btask_one"),
                owner_bot_id: id("bot_b"),
                title: "verify".into(),
                input: vec![MessagePart::Text { text: "facts".into() }],
                expected_output: "verified facts".into(),
                parent_task_id: None,
                budget: Default::default(),
            }),
            at_ms: 20,
        },
    );
    let claimed = write(
        &repo,
        &conversation_id,
        state.event_version,
        "idem_claim",
        ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
        ConversationCommand::ClaimDelivery { generation: id("claim_one"), at_ms: 30 },
    );
    let token = claimed.deliveries.in_flight.clone().unwrap();
    let delivery_id = token.delivery_ids[0].clone();

    let runs = RunRepository::new(repo.root());
    let run_id = id("brun_recipient");
    let run_state = runs
        .execute(RunWrite {
            run_id: run_id.clone(),
            expected_version: 0,
            idempotency_key: key("idem_run_queue"),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RunCommand::Queue {
                spec: Box::new(RunSpec {
                    run_id: run_id.clone(),
                    bot_id: id("bot_b"),
                    revision_id: id("brev_b"),
                    revision_hash: ContentHash::from_bytes(b"b"),
                    mrm_role: id("execution"),
                    trigger: RunTrigger { kind: RunTriggerKind::BotRequest, source_id: Some(delivery_id.clone()), occurrence_id: None },
                    input: vec![crate::agent::dcp::ProviderNeutralPart::Text { text: "facts".into() }],
                    conversation_id: Some(conversation_id.clone()),
                    task_id: Some(id("btask_one")),
                    permission: PermissionSnapshot {
                        capabilities: Default::default(),
                        resources: Default::default(),
                        approval: crate::bot::ApprovalPolicy::ManualWhenRequired,
                        budget: Default::default(),
                    },
                }),
                at_ms: 40,
            },
        })
        .unwrap();
    assert_eq!(run_state.status, crate::bot::run::RunStatus::Queued);
    state = write(
        &repo,
        &conversation_id,
        claimed.event_version,
        "idem_ack",
        ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
        ConversationCommand::AcknowledgeDelivery { token, run_ids: BTreeMap::from([(delivery_id, run_id)]), at_ms: 50 },
    );
    assert!(state.deliveries.in_flight.is_none());
    assert_eq!(state.tasks[&id("btask_one")].status, TaskStatus::Submitted);
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn moderator_cannot_be_removed_and_history_cursor_is_preserved() {
    let repo = repository("members");
    let state = create_group(&repo);
    let result = repo.execute(ConversationWrite {
        conversation_id: state.conversation_id.clone(),
        expected_version: state.event_version,
        idempotency_key: key("idem_remove_mod"),
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        command: ConversationCommand::RemoveMember { bot_id: id("bot_a"), at_ms: 20 },
    });
    assert!(matches!(result, Err(ConversationError::Rejected(_))));
    assert_eq!(state.members[&id("bot_b")].history_visible_from_seq, 1);
    std::fs::remove_dir_all(repo.root()).ok();
}
