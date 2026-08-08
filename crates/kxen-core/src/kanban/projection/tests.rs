use super::*;
use crate::kanban::events::*;
use crate::kanban::model::default_template;

fn event(seq: u64, kind: EventKind) -> KanbanEvent {
    KanbanEvent { id: format!("kev_{seq}"), board_id: "board_t".into(), seq, created_at: 1_000 + seq, kind }
}

/// 覆盖全部事件类型的固定序列（时间戳固定，与挂钟无关）。
fn sample_events() -> Vec<KanbanEvent> {
    vec![
        event(1, EventKind::BoardCreate(BoardCreatePayload { title: "看板".into(), columns: default_template() })),
        event(
            2,
            EventKind::AgentDefined(AgentDefinedPayload {
                name: "qa".into(),
                role: "review".into(),
                model: "auto".into(),
                permission_profile: "readonly+test".into(),
                tools: None,
            }),
        ),
        event(
            3,
            EventKind::CardCreate(CardCreatePayload {
                card_id: "card_a".into(),
                column_id: "requirements".into(),
                title: "加登录".into(),
                body: "支持邮箱登录".into(),
            }),
        ),
        event(4, EventKind::CardComment(CardCommentPayload { card_id: "card_a".into(), author: "human".into(), body: "先做这个".into() })),
        event(
            5,
            EventKind::CardMove(CardMovePayload {
                card_id: "card_a".into(),
                from: "requirements".into(),
                to: "implementing".into(),
                outcome: Outcome::Success,
            }),
        ),
        event(
            6,
            EventKind::RunStarted(RunStartedPayload {
                run_id: "board_t:card_a:implementing:1".into(),
                card_id: "card_a".into(),
                column_id: "implementing".into(),
                attempt: 1,
            }),
        ),
        event(7, EventKind::RunFinished(RunFinishedPayload { run_id: "board_t:card_a:implementing:1".into(), outcome: Outcome::Success })),
        event(
            8,
            EventKind::RunStarted(RunStartedPayload {
                run_id: "board_t:card_a:testing:1".into(),
                card_id: "card_a".into(),
                column_id: "testing".into(),
                attempt: 1,
            }),
        ),
        event(9, EventKind::RunTimeout(RunTimeoutPayload { run_id: "board_t:card_a:testing:1".into() })),
        event(
            10,
            EventKind::ColumnAdd(ColumnAddPayload {
                column: ColumnDef {
                    id: "archive".into(),
                    title: "归档".into(),
                    on_enter: Default::default(),
                    transitions: Default::default(),
                    wip_limit: Some(10),
                    timeout_ms: None,
                },
            }),
        ),
    ]
}

#[test]
fn replay_twice_is_byte_identical() {
    let events = sample_events();
    let first = serde_json::to_string(&replay("board_t", &events).unwrap()).unwrap();
    let second = serde_json::to_string(&replay("board_t", &events).unwrap()).unwrap();
    assert_eq!(first, second, "同一事件序列重放两次必须逐字节一致");
}

#[test]
fn incremental_reduce_matches_full_replay() {
    let events = sample_events();
    let mut state = BoardState::new("board_t");
    for e in &events {
        reduce(&mut state, e).unwrap();
    }
    assert_eq!(serde_json::to_string(&state).unwrap(), serde_json::to_string(&replay("board_t", &events).unwrap()).unwrap());
}

#[test]
fn replay_applies_semantics() {
    let state = replay("board_t", &sample_events()).unwrap();
    assert_eq!(state.seq, 10);
    assert_eq!(state.title.as_deref(), Some("看板"));
    let card = &state.cards["card_a"];
    // run_timeout 后卡片停在原列 blocked，绝不留在 running
    assert_eq!(card.column_id, "testing");
    assert_eq!(card.status, CardStatus::Blocked);
    assert!(card.block_reason.as_deref().unwrap().contains("timeout"));
    assert_eq!(card.comments.len(), 1);
    assert_eq!(state.runs["board_t:card_a:testing:1"].outcome, Some(Outcome::Timeout));
    assert!(state.agents.contains_key("qa"));
    assert!(state.column("archive").is_some());
}

#[test]
fn replay_fails_closed_on_contradictory_stream() {
    // card_move 的 from 与投影不符：事件流被篡改，必须报错而非猜测
    let mut events = sample_events();
    events[4] = event(
        5,
        EventKind::CardMove(CardMovePayload {
            card_id: "card_a".into(),
            from: "done".into(),
            to: "implementing".into(),
            outcome: Outcome::Success,
        }),
    );
    assert!(matches!(replay("board_t", &events), Err(KanbanError::Projection(_))));
}

fn policy_spec(max_uses: Option<u32>) -> crate::kanban::model::PolicySpec {
    crate::kanban::model::PolicySpec { allowlist: vec!["cargo".into()], expires_at_ms: None, max_uses }
}

#[test]
fn policy_events_project_active_policy() {
    let events = vec![
        event(1, EventKind::BoardCreate(BoardCreatePayload { title: "看板".into(), columns: default_template() })),
        event(2, EventKind::PolicySet(PolicySetPayload { policy: policy_spec(Some(3)) })),
        event(3, EventKind::AutoApproved(AutoApprovedPayload { run_id: "r1".into(), command: "cargo test".into() })),
        event(4, EventKind::AutoApproved(AutoApprovedPayload { run_id: "r1".into(), command: "cargo build".into() })),
    ];
    let state = replay("board_t", &events).unwrap();
    let policy = state.policy.as_ref().expect("policy_set 后必须有生效授权");
    assert_eq!(policy.used, 2);
    assert_eq!(policy.spec.max_uses, Some(3));
    // 重设即重置计数（显式续期语义）
    let mut renewed = events.clone();
    renewed.push(event(5, EventKind::PolicySet(PolicySetPayload { policy: policy_spec(None) })));
    let state = replay("board_t", &renewed).unwrap();
    assert_eq!(state.policy.as_ref().unwrap().used, 0);
}

#[test]
fn auto_approved_beyond_max_uses_fails_closed() {
    // 锁外写入者绕过 command 守卫超放（used 已达 max_uses 仍追加放行事件）：重放必须 fail-closed 报矛盾
    let events = vec![
        event(1, EventKind::BoardCreate(BoardCreatePayload { title: "看板".into(), columns: default_template() })),
        event(2, EventKind::PolicySet(PolicySetPayload { policy: policy_spec(Some(2)) })),
        event(3, EventKind::AutoApproved(AutoApprovedPayload { run_id: "r1".into(), command: "cargo test".into() })),
        event(4, EventKind::AutoApproved(AutoApprovedPayload { run_id: "r1".into(), command: "cargo build".into() })),
        event(5, EventKind::AutoApproved(AutoApprovedPayload { run_id: "r1".into(), command: "cargo clippy".into() })),
    ];
    assert!(matches!(replay("board_t", &events), Err(KanbanError::Projection(_))));
}

#[test]
fn auto_approved_without_policy_fails_closed() {
    // 事件流被篡改（绕过守卫写入放行事件）：fail-closed 报错，不猜
    let events = vec![
        event(1, EventKind::BoardCreate(BoardCreatePayload { title: "看板".into(), columns: default_template() })),
        event(2, EventKind::AutoApproved(AutoApprovedPayload { run_id: "r1".into(), command: "cargo test".into() })),
    ];
    assert!(matches!(replay("board_t", &events), Err(KanbanError::Projection(_))));
}
