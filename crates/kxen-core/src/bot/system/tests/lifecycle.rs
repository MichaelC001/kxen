use super::*;

#[test]
fn inactive_bot_reconciliation_rejects_pending_work_and_pauses_routine() {
    let root = std::env::temp_dir().join(format!("kxen-bot-system-inactive-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let moderator = id("bot_inactive_moderator");
    let worker = id("bot_inactive_worker");
    publish(&system, &moderator, &definition("Moderator"), "inactive_moderator");
    publish(&system, &worker, &definition("Worker"), "inactive_worker");

    let routine_id = id("routine_inactive_worker");
    system
        .routines()
        .execute(RoutineWrite {
            routine_id: routine_id.clone(),
            expected_version: 0,
            idempotency_key: key("idem_inactive_routine"),
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            command: RoutineCommand::Create {
                routine_id: routine_id.clone(),
                definition: RoutineDefinition {
                    bot_id: worker.clone(),
                    name: "Inactive worker schedule".into(),
                    schedule: ScheduleSpec {
                        expression: ScheduleExpression::Once { at_ms: 60_000 },
                        timezone: "UTC".into(),
                        misfire: MisfirePolicy::RunOnce,
                        max_lateness_ms: 1_000,
                    },
                    context_mode: ContextMode::Isolated,
                    target_conversation_id: None,
                    input: vec![crate::agent::dcp::ProviderNeutralPart::Text { text: "work".into() }],
                    budget_override: None,
                    revision_policy: RevisionPolicy::FollowCurrent,
                    failure_threshold: 3,
                },
                at_ms: 3,
            },
        })
        .unwrap();

    let conversation_id = id("bconv_inactive_worker");
    let created = system
        .mutate_conversation(ConversationMutation {
            conversation_id: conversation_id.clone(),
            expected_version: 0,
            actor: ActorRef::Owner,
            command: ConversationCommand::Create {
                conversation_id: conversation_id.clone(),
                kind: ConversationKind::BotGroup,
                members: vec![participant(&moderator), participant(&worker)],
                moderator_bot_id: Some(moderator),
                at_ms: 4,
            },
            trace: TraceContext::default(),
            idempotency_key: key("idem_inactive_group"),
        })
        .unwrap();
    let task_id = id("btask_inactive_worker");
    let mut message = Message {
        message_id: id("bmsg_inactive_worker"),
        conversation_id: conversation_id.clone(),
        actor: ActorRef::Owner,
        kind: MessageKind::Instruction,
        parts: vec![MessagePart::Text { text: "perform pending work".into() }],
        mentions: BTreeSet::new(),
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
    message.mentions.insert(worker.clone());
    system
        .post_conversation(PostConversation {
            conversation_id: conversation_id.clone(),
            expected_version: created.event_version,
            actor: ActorRef::Owner,
            message,
            task: Some(NewTask {
                task_id: task_id.clone(),
                owner_bot_id: worker.clone(),
                title: "Pending work".into(),
                input: vec![MessagePart::Text { text: "input".into() }],
                expected_output: "evidence".into(),
                parent_task_id: None,
                budget: Default::default(),
            }),
            trace: TraceContext::default(),
            idempotency_key: key("idem_inactive_post"),
            at_ms: 5,
        })
        .unwrap();

    let current = system.definitions().get(&worker).unwrap();
    system
        .definitions()
        .change_lifecycle(ChangeLifecycle {
            bot_id: &worker,
            expected_event_version: current.event_version,
            change: LifecycleChange::Pause,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key("idem_inactive_pause"),
            at_ms: 6,
        })
        .unwrap();
    assert_eq!(system.reconcile_inactive_bot_work(7).unwrap(), 2);

    let conversation = system.conversations().get(&conversation_id).unwrap();
    assert!(conversation.deliveries.records.is_empty());
    assert_eq!(conversation.deliveries.tombstones.back().unwrap().status, crate::core::delivery::DeliveryStatus::Rejected);
    assert_eq!(conversation.tasks[&task_id].status, TaskStatus::Rejected);
    assert_eq!(system.routines().get(&routine_id).unwrap().lifecycle, RoutineLifecycle::Paused);
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
