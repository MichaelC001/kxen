use super::*;
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, TraceContext};

fn repository(name: &str) -> BotRepository {
    BotRepository::new(std::env::temp_dir().join(format!("kxen-bot-{name}-{}", uuid::Uuid::new_v4())))
}

fn definition(name: &str) -> BotDefinition {
    let mut definition = BotDefinition::empty(name);
    definition.description = "Processes recurring work".into();
    definition.objective = "Produce a checked result".into();
    definition.instructions = "Read the input, execute the approved work, and return evidence.".into();
    definition.success_criteria = vec!["Output satisfies the declared contract".into()];
    definition.output_contract.description = "A result with verification evidence".into();
    definition
}

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::parse(value).unwrap()
}

fn create(repo: &BotRepository, bot_id: &ResourceId, definition: &BotDefinition) -> BotState {
    repo.create(CreateBot {
        bot_id,
        definition,
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        idempotency_key: key("idem_create"),
        at_ms: 10,
    })
    .unwrap()
}

fn publish(repo: &BotRepository, state: &BotState, idempotency_key: &str) -> BotState {
    let draft = state.draft.as_ref().unwrap();
    repo.publish(PublishBot {
        bot_id: &state.bot_id,
        expected_event_version: state.event_version,
        expected_draft_version: draft.version,
        expected_content_hash: &draft.content_hash,
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        idempotency_key: key(idempotency_key),
        at_ms: 20,
    })
    .unwrap()
}

#[test]
fn draft_publish_and_replay_preserve_immutable_revisions() {
    let repo = repository("revision");
    let bot_id = id("bot_reporter");
    let first = create(&repo, &bot_id, &definition("Reporter"));
    let published = publish(&repo, &first, "idem_publish_one");
    assert_eq!(published.lifecycle, BotLifecycle::Active);
    assert!(published.draft.is_none());
    assert_eq!(published.revisions.len(), 1);

    let mut changed = definition("Reporter 2");
    changed.objective = "Produce a second checked result".into();
    let drafted = repo
        .replace_draft(ReplaceDraft {
            bot_id: &bot_id,
            expected_event_version: published.event_version,
            expected_draft_version: 0,
            definition: &changed,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key("idem_draft_two"),
            at_ms: 30,
        })
        .unwrap();
    let second = publish(&repo, &drafted, "idem_publish_two");
    assert_eq!(second.revisions.len(), 2);
    assert_eq!(second.revisions[&1].definition.display_name, "Reporter");
    assert_eq!(second.revisions[&2].definition.display_name, "Reporter 2");
    assert_eq!(repo.get(&bot_id).unwrap(), second);
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn publish_rejects_invalid_definition_and_stale_draft() {
    let repo = repository("validation");
    let bot_id = id("bot_validation");
    let invalid = BotDefinition::empty("Incomplete");
    let state = create(&repo, &bot_id, &invalid);
    let draft = state.draft.as_ref().unwrap();
    let result = repo.publish(PublishBot {
        bot_id: &bot_id,
        expected_event_version: state.event_version,
        expected_draft_version: draft.version,
        expected_content_hash: &draft.content_hash,
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        idempotency_key: key("idem_invalid_publish"),
        at_ms: 20,
    });
    assert!(matches!(result, Err(BotError::InvalidDefinition(_))));

    let valid = definition("Complete");
    let replaced = repo
        .replace_draft(ReplaceDraft {
            bot_id: &bot_id,
            expected_event_version: state.event_version,
            expected_draft_version: 1,
            definition: &valid,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key("idem_replace"),
            at_ms: 30,
        })
        .unwrap();
    let stale = repo.replace_draft(ReplaceDraft {
        bot_id: &bot_id,
        expected_event_version: state.event_version,
        expected_draft_version: 1,
        definition: &valid,
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        idempotency_key: key("idem_stale"),
        at_ms: 40,
    });
    assert!(matches!(stale, Err(BotError::VersionConflict(_))));
    assert_eq!(replaced.draft.unwrap().version, 2);
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn lifecycle_is_explicit_and_restore_is_paused() {
    let repo = repository("lifecycle");
    let bot_id = id("bot_lifecycle");
    let state = publish(&repo, &create(&repo, &bot_id, &definition("Lifecycle")), "idem_publish");
    let apply = |state: &BotState, change, idempotency_key, at_ms| {
        repo.change_lifecycle(ChangeLifecycle {
            bot_id: &bot_id,
            expected_event_version: state.event_version,
            change,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key(idempotency_key),
            at_ms,
        })
    };
    let paused = apply(&state, LifecycleChange::Pause, "idem_pause", 30).unwrap();
    let resumed = apply(&paused, LifecycleChange::Resume, "idem_resume", 40).unwrap();
    let trashed = apply(&resumed, LifecycleChange::Trash, "idem_trash", 50).unwrap();
    assert!(matches!(apply(&trashed, LifecycleChange::Resume, "idem_bad", 60), Err(BotError::LifecycleRejected(_))));
    let restored = apply(&trashed, LifecycleChange::Restore, "idem_restore", 70).unwrap();
    assert_eq!(restored.lifecycle, BotLifecycle::Paused);
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn create_is_idempotent_and_listing_excludes_trashed_by_default() {
    let repo = repository("list");
    let bot_id = id("bot_list");
    let definition = definition("Listed");
    let first = create(&repo, &bot_id, &definition);
    let duplicate = create(&repo, &bot_id, &definition);
    assert_eq!(first, duplicate);
    assert_eq!(std::fs::read_to_string(repo.root().join("definitions/bot_list/events.jsonl")).unwrap().lines().count(), 1);
    let trashed = repo
        .change_lifecycle(ChangeLifecycle {
            bot_id: &bot_id,
            expected_event_version: first.event_version,
            change: LifecycleChange::Trash,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key("idem_trash"),
            at_ms: 20,
        })
        .unwrap();
    assert_eq!(trashed.lifecycle, BotLifecycle::Trashed);
    assert!(repo.list(false).unwrap().is_empty());
    assert_eq!(repo.list(true).unwrap().len(), 1);
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn exact_create_retry_returns_latest_state_after_later_events() {
    let repo = repository("advanced-retry");
    let bot_id = id("bot_advanced_retry");
    let definition = definition("Retry");
    let created = create(&repo, &bot_id, &definition);
    let published = publish(&repo, &created, "idem_publish");

    let retried = create(&repo, &bot_id, &definition);

    assert_eq!(retried, published);
    assert_eq!(std::fs::read_to_string(repo.root().join("definitions/bot_advanced_retry/events.jsonl")).unwrap().lines().count(), 2);
    std::fs::remove_dir_all(repo.root()).ok();
}
