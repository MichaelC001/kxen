use super::*;

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
