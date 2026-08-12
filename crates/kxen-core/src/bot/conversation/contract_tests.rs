use super::*;
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, TraceContext};

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn write(
    repo: &ConversationRepository,
    state: &ConversationState,
    key: &str,
    command: ConversationCommand,
) -> Result<ConversationState, ConversationError> {
    repo.execute(ConversationWrite {
        conversation_id: state.conversation_id.clone(),
        expected_version: state.event_version,
        idempotency_key: IdempotencyKey::parse(key).unwrap(),
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        command,
    })
}

#[test]
fn parent_task_waits_for_every_child_terminal() {
    let root = std::env::temp_dir().join(format!("kxen-conversation-parent-{}", uuid::Uuid::new_v4()));
    let repo = ConversationRepository::new(&root);
    let conversation_id = id("bconv_parent_child");
    let mut state = repo
        .execute(ConversationWrite {
            conversation_id: conversation_id.clone(),
            expected_version: 0,
            idempotency_key: IdempotencyKey::parse("idem_parent_group").unwrap(),
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            command: ConversationCommand::Create {
                conversation_id: conversation_id.clone(),
                kind: ConversationKind::BotGroup,
                members: ["bot_parent", "bot_child"]
                    .into_iter()
                    .map(|value| BotParticipant { bot_id: id(value), joined_at_seq: 1, history_visible_from_seq: 1, active: true })
                    .collect(),
                moderator_bot_id: Some(id("bot_parent")),
                at_ms: 1,
            },
        })
        .unwrap();
    for (message_id, task_id, parent_task_id, owner) in
        [("bmsg_parent", "btask_parent", None, "bot_parent"), ("bmsg_child", "btask_child", Some(id("btask_parent")), "bot_child")]
    {
        let target = id(owner);
        let message = Message {
            message_id: id(message_id),
            conversation_id: conversation_id.clone(),
            actor: ActorRef::Owner,
            kind: MessageKind::Instruction,
            parts: vec![MessagePart::Text { text: "work".into() }],
            mentions: [target.clone()].into_iter().collect(),
            everyone: false,
            target_bot_id: None,
            reply_to_message_id: None,
            task_id: Some(id(task_id)),
            origin_run_id: None,
            causation_id: None,
            correlation_id: None,
            delegation_depth: 0,
            hop_count: 0,
            created_at_ms: 2,
        };
        state = write(
            &repo,
            &state,
            &format!("idem_{task_id}"),
            ConversationCommand::Post {
                message: Box::new(message),
                task: Some(NewTask {
                    task_id: id(task_id),
                    owner_bot_id: target,
                    title: "work".into(),
                    input: vec![MessagePart::Text { text: "work".into() }],
                    expected_output: "result".into(),
                    parent_task_id,
                    budget: Default::default(),
                }),
                at_ms: 2,
            },
        )
        .unwrap();
        state = write(
            &repo,
            &state,
            &format!("idem_start_{task_id}"),
            ConversationCommand::ChangeTask { task_id: id(task_id), status: TaskStatus::Working, result: Vec::new(), at_ms: 3 },
        )
        .unwrap();
    }
    let parent_result = vec![MessagePart::Text { text: "parent done".into() }];
    assert!(matches!(
        write(
            &repo,
            &state,
            "idem_parent_early",
            ConversationCommand::ChangeTask {
                task_id: id("btask_parent"),
                status: TaskStatus::Completed,
                result: parent_result.clone(),
                at_ms: 4,
            }
        ),
        Err(ConversationError::Rejected(_))
    ));
    state = write(
        &repo,
        &state,
        "idem_child_done",
        ConversationCommand::ChangeTask {
            task_id: id("btask_child"),
            status: TaskStatus::Completed,
            result: vec![MessagePart::Text { text: "child done".into() }],
            at_ms: 5,
        },
    )
    .unwrap();
    state = write(
        &repo,
        &state,
        "idem_parent_done",
        ConversationCommand::ChangeTask { task_id: id("btask_parent"), status: TaskStatus::Completed, result: parent_result, at_ms: 6 },
    )
    .unwrap();
    assert_eq!(state.tasks[&id("btask_parent")].status, TaskStatus::Completed);
    std::fs::remove_dir_all(root).ok();
}
