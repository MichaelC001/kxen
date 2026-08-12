use super::Kind;

pub(super) fn required(method: &str) -> Option<&'static [(&'static str, Kind)]> {
    use Kind::{Array as A, Bool as B, Object as O, String as S, StringArray as SA, U64 as U};
    Some(match method {
        "bot.get" | "bot.draft.get" | "bot.memory.list" => &[("bot_id", S)],
        "bot.create" => &[("bot_id", S), ("idempotency_key", S)],
        "bot.duplicate" => &[("source_bot_id", S), ("bot_id", S), ("idempotency_key", S)],
        "bot.draft.patch" => {
            &[("bot_id", S), ("expected_version", U), ("expected_draft_version", U), ("definition", O), ("idempotency_key", S)]
        }
        "bot.validate" => &[("builder_session_id", S), ("idempotency_key", S)],
        "bot.publish" => &[("builder_session_id", S), ("review_hash", S), ("idempotency_key", S)],
        "bot.pause" | "bot.resume" | "bot.archive" | "bot.trash" | "bot.restore" => {
            &[("bot_id", S), ("expected_version", U), ("idempotency_key", S)]
        }
        "bot.builder.start" => &[("bot_id", S), ("builder_session_id", S), ("user_goal", S), ("idempotency_key", S)],
        "bot.builder.message" => &[("builder_session_id", S), ("message_id", S), ("text", S), ("idempotency_key", S)],
        "bot.builder.get" => &[("builder_session_id", S)],
        "bot.builder.grant" => &[("builder_session_id", S), ("draft_hash", S), ("reason", S), ("idempotency_key", S)],
        "bot.builder.test" => &[("builder_session_id", S), ("run_id", S), ("idempotency_key", S)],
        "bot.builder.cancel" => &[("builder_session_id", S), ("idempotency_key", S)],
        "bot.run.start" => &[("run_id", S), ("bot_id", S), ("input", A), ("idempotency_key", S)],
        "bot.run.get" => &[("run_id", S)],
        "bot.run.cancel" => &[("run_id", S), ("expected_version", U), ("idempotency_key", S)],
        "bot.run.input" => &[("run_id", S), ("request_id", S), ("input", A), ("expected_version", U), ("idempotency_key", S)],
        "bot.run.approval" => &[("run_id", S), ("approval_id", S), ("allow", B), ("expected_version", U), ("idempotency_key", S)],
        "bot.routine.create" => &[("routine_id", S), ("definition", O), ("idempotency_key", S)],
        "bot.routine.update" => &[("routine_id", S), ("definition", O), ("expected_version", U), ("idempotency_key", S)],
        "bot.routine.pause" | "bot.routine.resume" | "bot.routine.trash" => {
            &[("routine_id", S), ("expected_version", U), ("idempotency_key", S)]
        }
        "bot.routine.run_now" => &[("routine_id", S), ("occurrence_id", S), ("expected_version", U), ("idempotency_key", S)],
        "bot.routine.history" => &[("routine_id", S)],
        "bot.conversation.get" => &[("conversation_id", S)],
        "bot.conversation.post" => {
            &[("conversation_id", S), ("message_id", S), ("parts", A), ("expected_version", U), ("idempotency_key", S)]
        }
        "bot.conversation.pause" | "bot.conversation.resume" | "bot.conversation.archive" | "bot.group.stop" => {
            &[("conversation_id", S), ("expected_version", U), ("idempotency_key", S)]
        }
        "bot.direct.open" => &[("left_bot_id", S), ("right_bot_id", S), ("idempotency_key", S)],
        "bot.group.create" => &[("conversation_id", S), ("bot_ids", SA), ("moderator_bot_id", S), ("idempotency_key", S)],
        "bot.group.add_member" | "bot.group.remove_member" | "bot.group.set_moderator" => {
            &[("conversation_id", S), ("bot_id", S), ("expected_version", U), ("idempotency_key", S)]
        }
        "bot.task.get" => &[("task_id", S)],
        "bot.task.cancel" => &[("conversation_id", S), ("task_id", S), ("expected_version", U), ("idempotency_key", S)],
        "bot.task.reassign" => &[("conversation_id", S), ("task_id", S), ("bot_id", S), ("expected_version", U), ("idempotency_key", S)],
        "bot.memory.create" => {
            &[("bot_id", S), ("item_id", S), ("kind", S), ("content", S), ("expected_version", U), ("idempotency_key", S)]
        }
        "bot.memory.revise" => {
            &[("bot_id", S), ("item_id", S), ("expected_item_version", U), ("content", S), ("expected_version", U), ("idempotency_key", S)]
        }
        "bot.memory.remove" => {
            &[("bot_id", S), ("item_id", S), ("expected_item_version", U), ("expected_version", U), ("idempotency_key", S)]
        }
        "bot.recovery.repair" | "bot.recovery.clear" => {
            &[("kind", S), ("aggregate_id", S), ("expected_version", U), ("idempotency_key", S)]
        }
        "bot.artifact.get" | "bot.artifact.restore" | "bot.artifact.trash" => &[("artifact_id", S)],
        _ if method.starts_with("bot.") => &[],
        _ => return None,
    })
}

pub(super) fn optional(method: &str) -> Option<&'static [(&'static str, Kind)]> {
    use Kind::{Bool as B, Object as O, String as S, StringArray as SA, U64 as U};
    Some(match method {
        "bot.list" => &[("include_trashed", B)],
        "bot.create" => &[("display_name", S), ("definition", O)],
        "bot.duplicate" => &[("revision_id", S), ("display_name", S)],
        "bot.builder.start" => &[("display_name", S)],
        "bot.run.start" => &[("revision_id", S), ("conversation_id", S), ("budget", O)],
        "bot.run.list" => &[("bot_id", S), ("conversation_id", S), ("status", S)],
        "bot.run.cancel" | "bot.routine.pause" => &[("reason", S)],
        "bot.routine.list" => &[("bot_id", S)],
        "bot.conversation.list" => &[("kind", S), ("include_archived", B)],
        "bot.direct.open" => &[("conversation_id", S)],
        "bot.conversation.post" => {
            &[("mentions", SA), ("everyone", B), ("reply_to_message_id", S), ("task_id", S), ("correlation_id", S), ("task", O)]
        }
        "bot.group.add_member" => &[("history_visible_from_seq", U)],
        "bot.task.list" => &[("conversation_id", S), ("owner_bot_id", S)],
        _ if method.starts_with("bot.") => &[],
        _ => return None,
    })
}
