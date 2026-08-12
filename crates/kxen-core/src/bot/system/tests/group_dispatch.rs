use super::*;

#[test]
fn group_instruction_dispatches_one_durable_moderator_run_without_permission_union() {
    let root = std::env::temp_dir().join(format!("kxen-bot-system-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let system = BotSystem::new(&root).unwrap();
    let bot_a = id("bot_a");
    let bot_b = id("bot_b");
    let mut definition_a = definition("A");
    definition_a.resources.connectors.insert(id("connector_a"));
    definition_a
        .resources
        .workspaces
        .push(WorkspaceGrantSpec { workspace_id: crate::bot::executor::workspace_id(&root).unwrap(), paths: Vec::new() });
    let mut definition_b = definition("B");
    definition_b.resources.connectors.insert(id("connector_b"));
    definition_b
        .resources
        .workspaces
        .push(WorkspaceGrantSpec { workspace_id: crate::bot::executor::workspace_id(&root).unwrap(), paths: Vec::new() });
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
