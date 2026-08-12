use super::*;
use crate::bot::builder::{BuilderCommand, BuilderWrite, PermissionGrant, permission_hash};
use crate::bot::run::{RunCommand, RunWrite, UsageSummary};

fn start_builder_with_draft(system: &BotSystem, builder_id: &ResourceId, bot_id: &ResourceId) -> crate::bot::builder::BuilderState {
    let created = system
        .definitions()
        .create(CreateBot {
            bot_id,
            definition: &definition("Builder target"),
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key("idem_builder_target_create"),
            at_ms: 1,
        })
        .unwrap();
    let started = system
        .builder()
        .execute(BuilderWrite {
            builder_session_id: builder_id.clone(),
            expected_version: 0,
            idempotency_key: key("idem_builder_flow_start"),
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            command: BuilderCommand::Start {
                builder_session_id: builder_id.clone(),
                bot_id: bot_id.clone(),
                user_goal: "Build a verified recurring report".into(),
                at_ms: 2,
            },
        })
        .unwrap();
    let drafted = system
        .builder()
        .execute(BuilderWrite {
            builder_session_id: builder_id.clone(),
            expected_version: started.event_version,
            idempotency_key: key("idem_builder_flow_draft"),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Builder },
            trace: TraceContext::default(),
            command: BuilderCommand::ReplaceDraft {
                expected_draft_version: 0,
                source_message_id: None,
                definition: Box::new(definition("Builder draft")),
                at_ms: 3,
            },
        })
        .unwrap();
    assert_eq!(created.lifecycle, crate::bot::BotLifecycle::Draft);
    drafted
}

#[test]
fn binds_test_validation_and_publish_to_one_draft_hash() {
    let root = std::env::temp_dir().join(format!("kxen-bot-system-builder-flow-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let builder_id = id("builder_flow");
    let bot_id = id("bot_builder_flow");
    let drafted = start_builder_with_draft(&system, &builder_id, &bot_id);
    let draft = drafted.draft.as_ref().unwrap();
    let grant = PermissionGrant {
        grant_id: id("grant_builder_flow"),
        draft_hash: draft.content_hash.clone(),
        permission_hash: permission_hash(&draft.definition).unwrap(),
        reason: "Owner reviewed exact permissions".into(),
        granted_at_ms: 4,
    };
    let granted = system
        .builder()
        .execute(BuilderWrite {
            builder_session_id: builder_id.clone(),
            expected_version: drafted.event_version,
            idempotency_key: key("idem_builder_flow_grant"),
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            command: BuilderCommand::RecordGrant { grant, at_ms: 4 },
        })
        .unwrap();
    let synced = system.sync_builder_draft(&builder_id, key("idem_builder_flow_sync"), 5).unwrap();
    assert_eq!(synced.draft.as_ref().unwrap().content_hash, granted.draft.as_ref().unwrap().content_hash);

    let run_id = id("brun_builder_flow");
    let queued = system
        .queue_builder_test(
            &builder_id,
            run_id.clone(),
            vec![crate::agent::dcp::ProviderNeutralPart::Text { text: "verify report".into() }],
            key("idem_builder_flow_test"),
            6,
        )
        .unwrap();
    let running = system
        .runs()
        .execute(RunWrite {
            run_id: run_id.clone(),
            expected_version: queued.event_version,
            idempotency_key: key("idem_builder_flow_test_start"),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RunCommand::Start { at_ms: 7 },
        })
        .unwrap();
    let criterion = granted.draft.as_ref().unwrap().definition.success_criteria[0].clone();
    let evidence = serde_json::json!({
        "criteria": { criterion: true },
        "summary": "Verified exact success criteria"
    })
    .to_string();
    let completed = system
        .runs()
        .execute(RunWrite {
            run_id,
            expected_version: running.event_version,
            idempotency_key: key("idem_builder_flow_test_complete"),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RunCommand::Complete {
                result: vec![crate::agent::dcp::ProviderNeutralPart::Text { text: evidence }],
                usage: UsageSummary::default(),
                at_ms: 8,
            },
        })
        .unwrap();
    system.settle_run(&completed, 9).unwrap();

    let roles = [id("execution")].into_iter().collect();
    let validated = system.validate_builder(&builder_id, &roles, &Default::default(), key("idem_builder_flow_validate"), 10).unwrap();
    let report = validated.current_report().unwrap();
    assert!(report.publish_eligible);
    let published = system.publish_validated_builder(&builder_id, &report.draft_hash, key("idem_builder_flow_publish"), 11).unwrap();
    assert_eq!(published.lifecycle, crate::bot::BotLifecycle::Active);
    assert_eq!(published.current_revision().unwrap().content_hash, report.draft_hash);
    std::fs::remove_dir_all(root).ok();
}
