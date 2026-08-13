use super::events::BuilderEvent;
use super::*;
use crate::agent::capability::CapabilityCatalog;
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, TraceContext};
use std::collections::{BTreeMap, BTreeSet};

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::parse(value).unwrap()
}

fn definition(name: &str) -> crate::bot::BotDefinition {
    let mut definition = crate::bot::BotDefinition::empty(name);
    definition.objective = "Create a verified report".into();
    definition.instructions = "Read approved input and produce a verified report.".into();
    definition.success_criteria = vec!["Report totals are verified".into()];
    definition.output_contract.description = "Verified report".into();
    definition
}

fn write(
    repo: &BuilderRepository,
    session_id: &ResourceId,
    expected: u64,
    idempotency: &str,
    actor: ActorRef,
    command: BuilderCommand,
) -> BuilderState {
    repo.execute(BuilderWrite {
        builder_session_id: session_id.clone(),
        expected_version: expected,
        idempotency_key: key(idempotency),
        actor,
        trace: TraceContext::default(),
        command,
    })
    .unwrap()
}

#[test]
fn builder_draft_without_source_message_remains_readable() {
    let definition = definition("Legacy draft");
    let draft = BuilderDraft {
        version: 1,
        source_message_id: None,
        content_hash: definition.content_hash().unwrap(),
        definition,
        updated_at_ms: 1,
    };
    let mut encoded = serde_json::to_value(draft).unwrap();
    encoded.as_object_mut().unwrap().remove("source_message_id");
    let decoded: BuilderDraft = serde_json::from_value(encoded).unwrap();
    assert!(decoded.source_message_id.is_none());
}

#[test]
fn builder_turn_atomically_records_reply_and_target_draft() {
    let root = std::env::temp_dir().join(format!("kxen-builder-turn-{}", uuid::Uuid::new_v4()));
    let repo = BuilderRepository::new(&root);
    let session_id = id("builder_report");
    let started = write(
        &repo,
        &session_id,
        0,
        "idem_turn_start",
        ActorRef::Owner,
        BuilderCommand::Start {
            builder_session_id: session_id.clone(),
            bot_id: id("bot_report"),
            user_goal: "Build a report Bot".into(),
            at_ms: 1,
        },
    );
    let source_message_id = id("bmessage_owner");
    let owner_message = write(
        &repo,
        &session_id,
        started.event_version,
        "idem_owner_message",
        ActorRef::Owner,
        BuilderCommand::AppendMessage {
            message: BuilderMessage {
                message_id: source_message_id.clone(),
                actor: ActorRef::Owner,
                text: "Create a verified report".into(),
                created_at_ms: 2,
            },
            at_ms: 2,
        },
    );
    let wrong_bot = repo.execute(BuilderWrite {
        builder_session_id: session_id.clone(),
        expected_version: owner_message.event_version,
        idempotency_key: key("idem_wrong_bot_turn"),
        actor: ActorRef::Bot { id: id("bot_other") },
        trace: TraceContext::default(),
        command: BuilderCommand::ApplyTurn {
            source_message_id: source_message_id.clone(),
            message: BuilderMessage {
                message_id: id("bmessage_wrong_bot"),
                actor: ActorRef::Bot { id: id("bot_other") },
                text: "I should not be able to edit another Bot.".into(),
                created_at_ms: 3,
            },
            expected_draft_version: 0,
            definition: Some(Box::new(definition("Report Bot"))),
            at_ms: 3,
        },
    });
    assert!(matches!(wrong_bot, Err(BuilderError::Rejected(message)) if message.contains("target Bot self-builder")));
    let completed = write(
        &repo,
        &session_id,
        owner_message.event_version,
        "idem_builder_turn",
        ActorRef::Bot { id: id("bot_report") },
        BuilderCommand::ApplyTurn {
            source_message_id: source_message_id.clone(),
            message: BuilderMessage {
                message_id: id("bmessage_builder"),
                actor: ActorRef::Bot { id: id("bot_report") },
                text: "The Report Bot draft is ready for review.".into(),
                created_at_ms: 3,
            },
            expected_draft_version: 0,
            definition: Some(Box::new(definition("Report Bot"))),
            at_ms: 3,
        },
    );

    assert_eq!(completed.event_version, owner_message.event_version + 2);
    assert_eq!(completed.messages.len(), 2);
    assert_eq!(completed.messages[1].actor, ActorRef::Bot { id: id("bot_report") });
    let draft = completed.draft.as_ref().unwrap();
    assert_eq!(draft.source_message_id.as_ref(), Some(&source_message_id));
    assert_eq!(draft.definition.display_name, "Report Bot");

    let batches = crate::core::event_store::EventStore::<BuilderEvent>::new(
        root.join("definitions/builder-sessions").join(session_id.as_str()),
        crate::core::identity::AggregateRef { kind: crate::core::identity::AggregateKind::BuilderSession, id: session_id },
        crate::core::identity::SchemaVersion::new(1).unwrap(),
    )
    .load()
    .unwrap();
    assert_eq!(batches.last().unwrap().events.len(), 2);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn owner_cannot_append_another_message_before_builder_replies() {
    let root = std::env::temp_dir().join(format!("kxen-builder-pending-{}", uuid::Uuid::new_v4()));
    let repo = BuilderRepository::new(&root);
    let session_id = id("builder_pending");
    let started = write(
        &repo,
        &session_id,
        0,
        "idem_pending_start",
        ActorRef::Owner,
        BuilderCommand::Start {
            builder_session_id: session_id.clone(),
            bot_id: id("bot_pending"),
            user_goal: "Build a Bot".into(),
            at_ms: 1,
        },
    );
    let pending = write(
        &repo,
        &session_id,
        started.event_version,
        "idem_pending_first",
        ActorRef::Owner,
        BuilderCommand::AppendMessage {
            message: BuilderMessage {
                message_id: id("bmessage_first"),
                actor: ActorRef::Owner,
                text: "First request".into(),
                created_at_ms: 2,
            },
            at_ms: 2,
        },
    );
    let rejected = repo.execute(BuilderWrite {
        builder_session_id: session_id,
        expected_version: pending.event_version,
        idempotency_key: key("idem_pending_second"),
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        command: BuilderCommand::AppendMessage {
            message: BuilderMessage {
                message_id: id("bmessage_second"),
                actor: ActorRef::Owner,
                text: "Second request".into(),
                created_at_ms: 3,
            },
            at_ms: 3,
        },
    });
    assert!(matches!(rejected, Err(BuilderError::Rejected(message)) if message.contains("awaiting a Builder reply")));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn deterministic_validation_requires_exact_grant_and_test_evidence() {
    let definition = definition("Reporter");
    let draft_hash = definition.content_hash().unwrap();
    let permission_hash = permission_hash(&definition).unwrap();
    let catalog = CapabilityCatalog::default();
    let roles = [id("execution")].into_iter().collect::<BTreeSet<_>>();
    let grant = PermissionGrant {
        grant_id: id("grant_one"),
        draft_hash: draft_hash.clone(),
        permission_hash,
        reason: "Reviewed exact scope".into(),
        granted_at_ms: 1,
    };
    let without_test = validate(
        id("report_one"),
        &definition,
        ValidationContext { catalog: &catalog, mrm_roles: &roles, connectors: &Default::default(), grant: Some(&grant), tests: &[] },
        2,
    )
    .unwrap();
    assert!(!without_test.publish_eligible);
    assert!(without_test.findings.iter().any(|finding| finding.status == ValidationStatus::Unknown));
    let evidence = TestEvidence {
        run_id: id("brun_test"),
        draft_hash,
        passed: true,
        criteria: BTreeMap::from([("Report totals are verified".into(), true)]),
        summary: "Output contract verified".into(),
        recorded_at_ms: 3,
    };
    let eligible = validate(
        id("report_two"),
        &definition,
        ValidationContext {
            catalog: &catalog,
            mrm_roles: &roles,
            connectors: &Default::default(),
            grant: Some(&grant),
            tests: &[evidence],
        },
        4,
    )
    .unwrap();
    assert!(eligible.publish_eligible);
    assert!(eligible.findings.iter().all(|finding| finding.status == ValidationStatus::Pass));
}

#[test]
fn deterministic_validation_rejects_unconfigured_connectors() {
    let mut definition = definition("Connector Reporter");
    definition.resources.connectors.insert(id("missing_connector"));
    let report = validate(
        id("report_missing_connector"),
        &definition,
        ValidationContext {
            catalog: &CapabilityCatalog::default(),
            mrm_roles: &[id("execution")].into_iter().collect(),
            connectors: &Default::default(),
            grant: None,
            tests: &[],
        },
        1,
    )
    .unwrap();
    assert!(report.findings.iter().any(|finding| finding.code == "connectors" && finding.status == ValidationStatus::Fail));
    assert!(!report.publish_eligible);
}

#[test]
fn builder_cannot_grant_and_draft_change_invalidates_review_state() {
    let root = std::env::temp_dir().join(format!("kxen-builder-{}", uuid::Uuid::new_v4()));
    let repo = BuilderRepository::new(&root);
    let session_id = id("builder_session");
    let bot_id = id("bot_builder_target");
    let started = write(
        &repo,
        &session_id,
        0,
        "idem_start",
        ActorRef::Owner,
        BuilderCommand::Start {
            builder_session_id: session_id.clone(),
            bot_id,
            user_goal: "Build a recurring report Bot".into(),
            at_ms: 1,
        },
    );
    let drafted = write(
        &repo,
        &session_id,
        started.event_version,
        "idem_draft",
        ActorRef::Bot { id: id("bot_builder_target") },
        BuilderCommand::ReplaceDraft {
            expected_draft_version: 0,
            source_message_id: None,
            definition: Box::new(definition("Reporter")),
            at_ms: 2,
        },
    );
    let draft = drafted.draft.as_ref().unwrap();
    let denied_test = repo.execute(BuilderWrite {
        builder_session_id: session_id.clone(),
        expected_version: drafted.event_version,
        idempotency_key: key("idem_builder_test"),
        actor: ActorRef::Bot { id: id("bot_builder_target") },
        trace: TraceContext::default(),
        command: BuilderCommand::LinkTestRun { run_id: id("brun_builder_denied"), draft_hash: draft.content_hash.clone(), at_ms: 3 },
    });
    assert!(matches!(denied_test, Err(BuilderError::Rejected(_))));
    let grant = PermissionGrant {
        grant_id: id("grant_one"),
        draft_hash: draft.content_hash.clone(),
        permission_hash: permission_hash(&draft.definition).unwrap(),
        reason: "reviewed".into(),
        granted_at_ms: 3,
    };
    let denied = repo.execute(BuilderWrite {
        builder_session_id: session_id.clone(),
        expected_version: drafted.event_version,
        idempotency_key: key("idem_builder_grant"),
        actor: ActorRef::Bot { id: id("bot_builder_target") },
        trace: TraceContext::default(),
        command: BuilderCommand::RecordGrant { grant: grant.clone(), at_ms: 3 },
    });
    assert!(matches!(denied, Err(BuilderError::Rejected(_))));
    let granted = write(
        &repo,
        &session_id,
        drafted.event_version,
        "idem_owner_grant",
        ActorRef::Owner,
        BuilderCommand::RecordGrant { grant, at_ms: 3 },
    );
    let changed = write(
        &repo,
        &session_id,
        granted.event_version,
        "idem_changed",
        ActorRef::Bot { id: id("bot_builder_target") },
        BuilderCommand::ReplaceDraft {
            expected_draft_version: 1,
            source_message_id: None,
            definition: Box::new(definition("Reporter changed")),
            at_ms: 4,
        },
    );
    assert!(changed.current_report().is_none());
    assert_ne!(changed.draft.as_ref().unwrap().content_hash, changed.grants[0].draft_hash);
    assert_eq!(BUILDER_MRM_ROLE, "bot_builder");
    assert!(!BUILDER_CAPABILITIES.contains(&"bot_publish"));
    std::fs::remove_dir_all(root).ok();
}
