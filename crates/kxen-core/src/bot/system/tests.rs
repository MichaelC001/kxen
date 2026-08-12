use super::*;
use crate::bot::conversation::{BotParticipant, Message, MessageKind, MessagePart, NewTask, TaskStatus};
use crate::bot::routine::{ContextMode, RevisionPolicy, RoutineCommand, RoutineDefinition, RoutineLifecycle, RoutineWrite};
use crate::bot::{BotDefinition, ChangeLifecycle, CreateBot, LifecycleChange, PublishBot, WorkspaceGrantSpec};
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, TraceContext};
use crate::core::scheduler::{MisfirePolicy, ScheduleExpression, ScheduleSpec};
use std::collections::BTreeSet;

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::parse(value).unwrap()
}

fn definition(name: &str) -> BotDefinition {
    let mut definition = BotDefinition::empty(name);
    definition.objective = "Complete assigned work".into();
    definition.instructions = "Use only approved capabilities and return evidence.".into();
    definition.success_criteria = vec!["Evidence is returned".into()];
    definition.output_contract.description = "Verified evidence".into();
    definition.communication.allow_groups = true;
    definition
}

fn publish(system: &BotSystem, bot_id: &ResourceId, definition: &BotDefinition, suffix: &str) {
    let created = system
        .definitions()
        .create(CreateBot {
            bot_id,
            definition,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key(&format!("idem_create_{suffix}")),
            at_ms: 1,
        })
        .unwrap();
    let draft = created.draft.as_ref().unwrap();
    system
        .definitions()
        .publish(PublishBot {
            bot_id,
            expected_event_version: created.event_version,
            expected_draft_version: draft.version,
            expected_content_hash: &draft.content_hash,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key(&format!("idem_publish_{suffix}")),
            at_ms: 2,
        })
        .unwrap();
}

fn participant(bot_id: &ResourceId) -> BotParticipant {
    BotParticipant { bot_id: bot_id.clone(), joined_at_seq: 1, history_visible_from_seq: 1, active: true }
}

fn routine_definition(bot_id: &ResourceId) -> RoutineDefinition {
    RoutineDefinition {
        bot_id: bot_id.clone(),
        name: "Validated schedule".into(),
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
    }
}

#[test]
fn routine_admission_resolves_pinned_revision_and_closes_conversation_scope() {
    let root = std::env::temp_dir().join(format!("kxen-bot-system-routine-admission-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let bot_id = id("bot_routine_admission");
    publish(&system, &bot_id, &definition("Routine Bot"), "routine_admission");
    let bot = system.definitions().get(&bot_id).unwrap();
    let revision_id = bot.current_revision().unwrap().revision_id.clone();

    let mut routine = routine_definition(&bot_id);
    routine.revision_policy = RevisionPolicy::Pinned { revision_id: revision_id.clone() };
    assert_eq!(system.validate_routine_definition(&routine).unwrap(), revision_id);

    routine.target_conversation_id = Some(id("bconv_forbidden_isolated"));
    assert!(matches!(system.validate_routine_definition(&routine), Err(BotSystemError::Rejected(_))));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn routine_admission_requires_active_conversation_membership() {
    let root = std::env::temp_dir().join(format!("kxen-bot-system-routine-membership-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let member = id("bot_routine_member");
    let outsider = id("bot_routine_outsider");
    publish(&system, &member, &definition("Member"), "routine_member");
    publish(&system, &outsider, &definition("Outsider"), "routine_outsider");
    let conversation_id = id("bconv_routine_membership");
    system
        .mutate_conversation(ConversationMutation {
            conversation_id: conversation_id.clone(),
            expected_version: 0,
            actor: ActorRef::Owner,
            command: ConversationCommand::Create {
                conversation_id: conversation_id.clone(),
                kind: ConversationKind::HumanBot,
                members: vec![participant(&member)],
                moderator_bot_id: None,
                at_ms: 3,
            },
            trace: TraceContext::default(),
            idempotency_key: key("idem_routine_membership"),
        })
        .unwrap();
    let mut routine = routine_definition(&outsider);
    routine.context_mode = ContextMode::ContinueConversation;
    routine.target_conversation_id = Some(conversation_id);
    assert!(matches!(system.validate_routine_definition(&routine), Err(BotSystemError::Rejected(_))));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn bot_catalog_keeps_cross_scope_lsp_unavailable() {
    let root = std::env::temp_dir().join(format!("kxen-bot-system-catalog-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    assert_eq!(system.capabilities().get(&id("lsp")).unwrap().availability, crate::agent::capability::CapabilityAvailability::Unavailable);
    std::fs::remove_dir_all(root).ok();
}

fn fail_manual_run(system: &BotSystem, bot_id: &ResourceId, run_id: &str, at_ms: u64) {
    use crate::bot::run::{RunCommand, RunTrigger, RunTriggerKind, RunWrite, UsageSummary};

    let queued = system
        .queue_run(QueueRun {
            run_id: id(run_id),
            bot_id: bot_id.clone(),
            revision_id: None,
            trigger: RunTrigger { kind: RunTriggerKind::Manual, source_id: None, occurrence_id: None },
            input: vec![crate::agent::dcp::ProviderNeutralPart::Text { text: "fail deterministically".into() }],
            conversation_id: None,
            task_id: None,
            budget_override: None,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key(&format!("idem_{run_id}_queue")),
            at_ms,
        })
        .unwrap();
    let running = system
        .runs()
        .execute(RunWrite {
            run_id: queued.spec.run_id.clone(),
            expected_version: queued.event_version,
            idempotency_key: key(&format!("idem_{run_id}_start")),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RunCommand::Start { at_ms: at_ms + 1 },
        })
        .unwrap();
    let failed = system
        .runs()
        .execute(RunWrite {
            run_id: running.spec.run_id.clone(),
            expected_version: running.event_version,
            idempotency_key: key(&format!("idem_{run_id}_fail")),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RunCommand::Fail {
                code: "test_failure".into(),
                message: "controlled failure".into(),
                usage: UsageSummary::default(),
                at_ms: at_ms + 2,
            },
        })
        .unwrap();
    system.settle_run(&failed, at_ms + 3).unwrap();
}

#[test]
fn bot_failure_policy_pauses_after_exact_consecutive_threshold() {
    let root = std::env::temp_dir().join(format!("kxen-bot-system-failure-policy-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let bot_id = id("bot_failure_policy");
    let mut bot_definition = definition("Failure policy");
    bot_definition.failure.auto_pause_after_failures = 2;
    publish(&system, &bot_id, &bot_definition, "failure_policy");

    fail_manual_run(&system, &bot_id, "brun_failure_one", 10);
    assert_eq!(system.definitions().get(&bot_id).unwrap().lifecycle, crate::bot::BotLifecycle::Active);
    fail_manual_run(&system, &bot_id, "brun_failure_two", 20);
    assert_eq!(system.definitions().get(&bot_id).unwrap().lifecycle, crate::bot::BotLifecycle::Paused);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn routine_terminal_settlement_is_repeatable() {
    use crate::bot::run::{RunCommand, RunWrite, UsageSummary};

    let root = std::env::temp_dir().join(format!("kxen-bot-system-routine-settlement-{}", uuid::Uuid::new_v4()));
    let system = BotSystem::new(&root).unwrap();
    let bot_id = id("bot_routine_settlement");
    publish(&system, &bot_id, &definition("Routine settlement"), "routine_settlement");
    let routine_id = id("routine_repeatable_settlement");
    system
        .routines()
        .execute(RoutineWrite {
            routine_id: routine_id.clone(),
            expected_version: 0,
            idempotency_key: key("idem_routine_repeatable_create"),
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            command: RoutineCommand::Create { routine_id: routine_id.clone(), definition: routine_definition(&bot_id), at_ms: 1 },
        })
        .unwrap();
    let report = system.tick_routines(60_000);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let run_id = report.queued_run_ids[0].clone();
    let queued = system.runs().get(&run_id).unwrap();
    let running = system
        .runs()
        .execute(RunWrite {
            run_id: run_id.clone(),
            expected_version: queued.event_version,
            idempotency_key: key("idem_routine_repeatable_start"),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RunCommand::Start { at_ms: 60_001 },
        })
        .unwrap();
    let completed = system
        .runs()
        .execute(RunWrite {
            run_id: run_id.clone(),
            expected_version: running.event_version,
            idempotency_key: key("idem_routine_repeatable_complete"),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RunCommand::Complete {
                result: vec![crate::agent::dcp::ProviderNeutralPart::Text { text: "done".into() }],
                usage: UsageSummary::default(),
                at_ms: 60_002,
            },
        })
        .unwrap();
    system.settle_run(&completed, 60_003).unwrap();
    system.settle_run(&completed, 60_004).unwrap();
    let occurrence = system.routines().get(&routine_id).unwrap().occurrences.into_values().next().unwrap();
    assert_eq!(occurrence.status, crate::bot::routine::OccurrenceStatus::Completed);
    std::fs::remove_dir_all(root).ok();
}

mod builder_flow;
mod group_dispatch;

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

mod lifecycle;
