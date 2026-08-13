use super::*;

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
fn direct_reopen_revalidates_current_peer_policy() {
    let root = std::env::temp_dir().join(format!("kxen-bot-system-direct-reopen-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let bot_a = id("bot_direct_reopen_a");
    let bot_b = id("bot_direct_reopen_b");
    let mut a = definition("A");
    a.communication.allow_direct = true;
    a.communication.allowed_peers.insert(bot_b.clone());
    let mut b = definition("B");
    b.communication.allow_direct = true;
    b.communication.allowed_peers.insert(bot_a.clone());
    publish(&system, &bot_a, &a, "direct_reopen_a");
    publish(&system, &bot_b, &b, "direct_reopen_b");
    let conversation_id = crate::bot::conversation::direct_conversation_id(&bot_a, &bot_b).unwrap();
    let created = system
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
            idempotency_key: key("idem_direct_reopen_create"),
        })
        .unwrap();
    let archived = system
        .mutate_conversation(ConversationMutation {
            conversation_id: conversation_id.clone(),
            expected_version: created.event_version,
            actor: ActorRef::Owner,
            command: ConversationCommand::Archive { at_ms: 4 },
            trace: TraceContext::default(),
            idempotency_key: key("idem_direct_reopen_archive"),
        })
        .unwrap();
    let reopened = system
        .mutate_conversation(ConversationMutation {
            conversation_id,
            expected_version: archived.event_version,
            actor: ActorRef::Owner,
            command: ConversationCommand::Reopen { at_ms: 5 },
            trace: TraceContext::default(),
            idempotency_key: key("idem_direct_reopen_apply"),
        })
        .unwrap();

    assert_eq!(reopened.lifecycle, crate::bot::conversation::ConversationLifecycle::Active);
    let archived_again = system
        .mutate_conversation(ConversationMutation {
            conversation_id: reopened.conversation_id.clone(),
            expected_version: reopened.event_version,
            actor: ActorRef::Owner,
            command: ConversationCommand::Archive { at_ms: 6 },
            trace: TraceContext::default(),
            idempotency_key: key("idem_direct_reopen_archive_again"),
        })
        .unwrap();
    let current_b = system.definitions().get(&bot_b).unwrap();
    system
        .definitions()
        .change_lifecycle(ChangeLifecycle {
            bot_id: &bot_b,
            expected_event_version: current_b.event_version,
            change: LifecycleChange::Pause,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key("idem_direct_reopen_pause_b"),
            at_ms: 7,
        })
        .unwrap();
    let rejected = system.mutate_conversation(ConversationMutation {
        conversation_id: archived_again.conversation_id,
        expected_version: archived_again.event_version,
        actor: ActorRef::Owner,
        command: ConversationCommand::Reopen { at_ms: 8 },
        trace: TraceContext::default(),
        idempotency_key: key("idem_direct_reopen_inactive"),
    });
    assert!(matches!(rejected, Err(BotSystemError::Rejected(_))));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn conversation_bound_run_requires_active_membership() {
    let root = std::env::temp_dir().join(format!("kxen-bot-system-membership-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let bot_a = id("bot_member_a");
    let bot_b = id("bot_member_b");
    publish(&system, &bot_a, &definition("A"), "member_a");
    publish(&system, &bot_b, &definition("B"), "member_b");
    let conversation_id = id("bconv_membership");
    system
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
            idempotency_key: key("idem_membership_group"),
        })
        .unwrap();
    let outsider = id("bot_member_outsider");
    publish(&system, &outsider, &definition("Outsider"), "member_outsider");
    let result = system.queue_run(QueueRun {
        run_id: id("brun_membership_denied"),
        bot_id: outsider,
        revision_id: None,
        trigger: crate::bot::run::RunTrigger { kind: crate::bot::run::RunTriggerKind::Manual, source_id: None, occurrence_id: None },
        input: vec![crate::agent::dcp::ProviderNeutralPart::Text { text: "read group".into() }],
        conversation_id: Some(conversation_id),
        task_id: None,
        budget_override: None,
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        idempotency_key: key("idem_membership_denied"),
        at_ms: 4,
    });
    assert!(matches!(result, Err(BotSystemError::Rejected(_))));
    std::fs::remove_dir_all(root).ok();
}
