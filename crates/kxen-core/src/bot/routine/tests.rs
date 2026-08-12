use super::*;
use crate::agent::dcp::ProviderNeutralPart;
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, SystemActor, TraceContext};
use crate::core::scheduler::{MisfirePolicy, ScheduleExpression, ScheduleSpec};

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::parse(value).unwrap()
}

fn repository(name: &str) -> RoutineRepository {
    RoutineRepository::new(std::env::temp_dir().join(format!("kxen-routine-{name}-{}", uuid::Uuid::new_v4())))
}

fn definition(misfire: MisfirePolicy) -> RoutineDefinition {
    RoutineDefinition {
        bot_id: id("bot_worker"),
        name: "Every minute".into(),
        schedule: ScheduleSpec {
            expression: ScheduleExpression::Cron { expression: "* * * * *".into() },
            timezone: "UTC".into(),
            misfire,
            max_lateness_ms: 120_000,
        },
        context_mode: ContextMode::Isolated,
        target_conversation_id: None,
        input: vec![ProviderNeutralPart::Text { text: "generate report".into() }],
        budget_override: None,
        revision_policy: RevisionPolicy::FollowCurrent,
        failure_threshold: 3,
    }
}

fn write(
    repo: &RoutineRepository,
    routine_id: &ResourceId,
    expected: u64,
    idempotency: &str,
    actor: ActorRef,
    command: RoutineCommand,
) -> RoutineState {
    repo.execute(RoutineWrite {
        routine_id: routine_id.clone(),
        expected_version: expected,
        idempotency_key: key(idempotency),
        actor,
        trace: TraceContext::default(),
        command,
    })
    .unwrap()
}

fn create(repo: &RoutineRepository, routine_id: &ResourceId, misfire: MisfirePolicy) -> RoutineState {
    write(
        repo,
        routine_id,
        0,
        "idem_create",
        ActorRef::Owner,
        RoutineCommand::Create { routine_id: routine_id.clone(), definition: definition(misfire), at_ms: 0 },
    )
}

#[test]
fn duplicate_tick_creates_one_stable_occurrence() {
    let repo = repository("tick");
    let routine_id = id("routine_tick");
    let created = create(&repo, &routine_id, MisfirePolicy::RunOnce);
    let ticked = write(
        &repo,
        &routine_id,
        created.event_version,
        "idem_tick",
        ActorRef::System { actor: SystemActor::Scheduler },
        RoutineCommand::Tick { observed_at_ms: 60_000, resolved_revision_id: Some(id("brev_current")) },
    );
    let duplicate = write(
        &repo,
        &routine_id,
        created.event_version,
        "idem_tick",
        ActorRef::System { actor: SystemActor::Scheduler },
        RoutineCommand::Tick { observed_at_ms: 60_000, resolved_revision_id: Some(id("brev_current")) },
    );
    assert_eq!(ticked, duplicate);
    assert_eq!(ticked.occurrences.len(), 1);
    assert_eq!(ticked.occurrences.values().next().unwrap().resolved_revision_id, Some(id("brev_current")));
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn skip_and_run_once_misfires_have_distinct_outcomes() {
    let skip_repo = repository("skip");
    let skip_id = id("routine_skip");
    let skip = create(&skip_repo, &skip_id, MisfirePolicy::Skip);
    let skipped = write(
        &skip_repo,
        &skip_id,
        skip.event_version,
        "idem_skip",
        ActorRef::System { actor: SystemActor::Scheduler },
        RoutineCommand::Tick { observed_at_ms: 180_000, resolved_revision_id: None },
    );
    assert_eq!(skipped.occurrences.values().next().unwrap().status, OccurrenceStatus::Skipped);

    let run_repo = repository("run-once");
    let run_id = id("routine_run_once");
    let run = create(&run_repo, &run_id, MisfirePolicy::RunOnce);
    let recorded = write(
        &run_repo,
        &run_id,
        run.event_version,
        "idem_run_once",
        ActorRef::System { actor: SystemActor::Scheduler },
        RoutineCommand::Tick { observed_at_ms: 180_000, resolved_revision_id: Some(id("brev_current")) },
    );
    let occurrence = recorded.occurrences.values().next().unwrap();
    assert_eq!(occurrence.status, OccurrenceStatus::Recorded);
    assert_eq!(occurrence.missed_before, 2);
    std::fs::remove_dir_all(skip_repo.root()).ok();
    std::fs::remove_dir_all(run_repo.root()).ok();
}

#[test]
fn consecutive_failures_pause_exactly_at_threshold() {
    let repo = repository("failures");
    let routine_id = id("routine_failures");
    let mut state = create(&repo, &routine_id, MisfirePolicy::RunOnce);
    for index in 1..=3u64 {
        state = write(
            &repo,
            &routine_id,
            state.event_version,
            &format!("idem_manual_{index}"),
            ActorRef::Owner,
            RoutineCommand::RunNow {
                occurrence_id: id(&format!("occ_manual_{index}")),
                resolved_revision_id: id("brev_current"),
                at_ms: index * 10,
            },
        );
        let occurrence_id = id(&format!("occ_manual_{index}"));
        state = write(
            &repo,
            &routine_id,
            state.event_version,
            &format!("idem_link_{index}"),
            ActorRef::System { actor: SystemActor::Runtime },
            RoutineCommand::LinkRun { occurrence_id: occurrence_id.clone(), run_id: id(&format!("brun_{index}")), at_ms: index * 10 + 1 },
        );
        state = write(
            &repo,
            &routine_id,
            state.event_version,
            &format!("idem_result_{index}"),
            ActorRef::System { actor: SystemActor::Runtime },
            RoutineCommand::RecordResult { occurrence_id, error: Some("provider failed".into()), at_ms: index * 10 + 2 },
        );
        assert_eq!(state.lifecycle, if index < 3 { RoutineLifecycle::Active } else { RoutineLifecycle::Paused });
    }
    assert_eq!(state.consecutive_failures, 3);
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn pinned_revision_rejects_a_different_resolution() {
    let repo = repository("pinned");
    let routine_id = id("routine_pinned");
    let mut pinned = definition(MisfirePolicy::RunOnce);
    pinned.revision_policy = RevisionPolicy::Pinned { revision_id: id("brev_pinned") };
    let state = write(
        &repo,
        &routine_id,
        0,
        "idem_create",
        ActorRef::Owner,
        RoutineCommand::Create { routine_id: routine_id.clone(), definition: pinned, at_ms: 0 },
    );
    let result = repo.execute(RoutineWrite {
        routine_id: routine_id.clone(),
        expected_version: state.event_version,
        idempotency_key: key("idem_tick"),
        actor: ActorRef::System { actor: SystemActor::Scheduler },
        trace: TraceContext::default(),
        command: RoutineCommand::Tick { observed_at_ms: 60_000, resolved_revision_id: Some(id("brev_other")) },
    });
    assert!(matches!(result, Err(RoutineError::Rejected(_))));
    std::fs::remove_dir_all(repo.root()).ok();
}
