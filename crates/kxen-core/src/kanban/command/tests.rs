use super::*;
use crate::kanban::model::{CardState, ColumnDef, OnEnter, OnEnterKind, Transitions};

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-command-{tag}-{}-{nanos}", std::process::id()))
}

fn open_board(workspace: &Path) -> Board {
    let mut board = Board::open(workspace, "board_t").unwrap();
    board.apply(KanbanCommand::BoardCreate { title: "测试看板".into(), columns: None }).unwrap();
    board
}

fn create_card(board: &mut Board, title: &str) -> String {
    let event = board.apply(KanbanCommand::CardCreate { column_id: None, title: title.into(), body: "详情".into() }).unwrap();
    match event.kind {
        EventKind::CardCreate(payload) => payload.card_id,
        other => panic!("expected card_create, got {other:?}"),
    }
}

/// 守卫拒绝必须零副作用：事件流长度与投影序列化在拒绝前后完全一致。
fn assert_fail_closed(board: &Board, workspace: &Path, before_events: usize, before_state: String) {
    let dir = store::board_dir(workspace, "board_t").unwrap();
    assert_eq!(store::load_events(&store::events_path(&dir)).unwrap().len(), before_events, "拒绝的命令不得落事件");
    assert_eq!(serde_json::to_string(board.state()).unwrap(), before_state, "拒绝的命令不得改投影");
}

fn reject(board: &mut Board, command: KanbanCommand) -> KanbanError {
    board.apply(command).unwrap_err()
}

fn card<'a>(board: &'a Board, card_id: &str) -> &'a CardState {
    &board.state().cards[card_id]
}

#[test]
fn default_template_full_lifecycle() {
    let workspace = temp("lifecycle");
    let mut board = open_board(&workspace);
    let card_id = create_card(&mut board, "加登录");
    assert_eq!(card(&board, &card_id).column_id, "requirements");
    assert_eq!(card(&board, &card_id).status, CardStatus::WaitingHuman, "human_gate 列进卡即停车等人");

    // 需求 approve -> 实现中（agent_run 列 Ready，等 driver 拉起）
    board.apply(KanbanCommand::CardMove { card_id: card_id.clone(), outcome: Outcome::Success }).unwrap();
    assert_eq!(card(&board, &card_id).column_id, "implementing");
    assert_eq!(card(&board, &card_id).status, CardStatus::Ready);

    // 实现 run 成功 -> 自动流转测试中
    let run = board.apply(KanbanCommand::RunStarted { card_id: card_id.clone() }).unwrap();
    let EventKind::RunStarted(started) = &run.kind else { panic!("expected run_started") };
    assert_eq!(started.run_id, format!("board_t:{card_id}:implementing:1"));
    assert_eq!(card(&board, &card_id).status, CardStatus::Running);
    board.apply(KanbanCommand::RunFinished { run_id: started.run_id.clone(), outcome: Outcome::Success }).unwrap();
    assert_eq!(card(&board, &card_id).column_id, "testing");

    // 测试 run 失败 -> 按 on_failure 回流实现中
    let run = board.apply(KanbanCommand::RunStarted { card_id: card_id.clone() }).unwrap();
    let EventKind::RunStarted(started) = &run.kind else { panic!("expected run_started") };
    board.apply(KanbanCommand::RunFinished { run_id: started.run_id.clone(), outcome: Outcome::Failure }).unwrap();
    assert_eq!(card(&board, &card_id).column_id, "implementing");

    // 再走一轮成功到待验证，人工 approve -> 完成（终态）
    let run = board.apply(KanbanCommand::RunStarted { card_id: card_id.clone() }).unwrap();
    let EventKind::RunStarted(started) = &run.kind else { panic!("expected run_started") };
    assert_eq!(started.attempt, 2, "attempt 按 card+column 递增");
    board.apply(KanbanCommand::RunFinished { run_id: started.run_id.clone(), outcome: Outcome::Success }).unwrap();
    let run = board.apply(KanbanCommand::RunStarted { card_id: card_id.clone() }).unwrap();
    let EventKind::RunStarted(started) = &run.kind else { panic!("expected run_started") };
    board.apply(KanbanCommand::RunFinished { run_id: started.run_id.clone(), outcome: Outcome::Success }).unwrap();
    assert_eq!(card(&board, &card_id).column_id, "review");
    assert_eq!(card(&board, &card_id).status, CardStatus::WaitingHuman);
    board.apply(KanbanCommand::CardComment { card_id: card_id.clone(), author: "human".into(), body: "看着没问题".into() }).unwrap();
    board.apply(KanbanCommand::CardMove { card_id: card_id.clone(), outcome: Outcome::Success }).unwrap();
    assert_eq!(card(&board, &card_id).column_id, "done");

    // 重启重放：重新 open 后投影逐字节一致
    let reopened = Board::open(&workspace, "board_t").unwrap();
    assert_eq!(serde_json::to_string(reopened.state()).unwrap(), serde_json::to_string(board.state()).unwrap());
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn commands_before_board_create_are_rejected() {
    let workspace = temp("notcreated");
    let mut board = Board::open(&workspace, "board_t").unwrap();
    assert!(matches!(
        reject(&mut board, KanbanCommand::CardCreate { column_id: None, title: "x".into(), body: String::new() }),
        KanbanError::BoardNotCreated(_)
    ));
    board.apply(KanbanCommand::BoardCreate { title: "t".into(), columns: None }).unwrap();
    assert!(matches!(reject(&mut board, KanbanCommand::BoardCreate { title: "again".into(), columns: None }), KanbanError::BoardExists(_)));
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn illegal_moves_fail_closed() {
    let workspace = temp("moves");
    let mut board = open_board(&workspace);
    let card_id = create_card(&mut board, "x");
    let dir = store::board_dir(&workspace, "board_t").unwrap();
    let events = store::load_events(&store::events_path(&dir)).unwrap().len();
    let state = serde_json::to_string(board.state()).unwrap();

    // 需求列无 on_failure 出边：reject 无路可走
    assert!(matches!(
        reject(&mut board, KanbanCommand::CardMove { card_id: card_id.clone(), outcome: Outcome::Failure }),
        KanbanError::NoTransition { .. }
    ));
    // Timeout 不是合法迁移意图
    assert!(matches!(
        reject(&mut board, KanbanCommand::CardMove { card_id: card_id.clone(), outcome: Outcome::Timeout }),
        KanbanError::InvalidCommand(_)
    ));
    // 不存在的卡
    assert!(matches!(
        reject(&mut board, KanbanCommand::CardMove { card_id: "card_nope".into(), outcome: Outcome::Success }),
        KanbanError::CardNotFound(_)
    ));
    assert_fail_closed(&board, &workspace, events, state);

    // Running 中的卡不可手动迁移
    board.apply(KanbanCommand::CardMove { card_id: card_id.clone(), outcome: Outcome::Success }).unwrap();
    board.apply(KanbanCommand::RunStarted { card_id: card_id.clone() }).unwrap();
    assert!(matches!(
        reject(&mut board, KanbanCommand::CardMove { card_id: card_id.clone(), outcome: Outcome::Success }),
        KanbanError::RunInProgress(_)
    ));

    // 终态列无出边
    let run_id = card(&board, &card_id).current_run.clone().unwrap();
    board.apply(KanbanCommand::RunFinished { run_id, outcome: Outcome::Success }).unwrap();
    let run = board.apply(KanbanCommand::RunStarted { card_id: card_id.clone() }).unwrap();
    let EventKind::RunStarted(started) = &run.kind else { panic!("expected run_started") };
    board.apply(KanbanCommand::RunFinished { run_id: started.run_id.clone(), outcome: Outcome::Success }).unwrap();
    board.apply(KanbanCommand::CardMove { card_id: card_id.clone(), outcome: Outcome::Success }).unwrap();
    assert_eq!(card(&board, &card_id).column_id, "done");
    assert!(matches!(
        reject(&mut board, KanbanCommand::CardMove { card_id: card_id.clone(), outcome: Outcome::Success }),
        KanbanError::NoTransition { .. }
    ));
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn illegal_card_and_column_commands_fail_closed() {
    let workspace = temp("cards");
    let mut board = open_board(&workspace);
    let events = store::load_events(&store::events_path(&store::board_dir(&workspace, "board_t").unwrap())).unwrap().len();
    let state = serde_json::to_string(board.state()).unwrap();

    assert!(matches!(
        reject(&mut board, KanbanCommand::CardCreate { column_id: Some("nowhere".into()), title: "x".into(), body: String::new() }),
        KanbanError::ColumnNotFound(_)
    ));
    assert!(matches!(
        reject(&mut board, KanbanCommand::CardCreate { column_id: None, title: "  ".into(), body: String::new() }),
        KanbanError::InvalidCommand(_)
    ));
    assert!(matches!(
        reject(&mut board, KanbanCommand::CardComment { card_id: "card_nope".into(), author: "human".into(), body: "hi".into() }),
        KanbanError::CardNotFound(_)
    ));
    // 重复列 id
    let dup = ColumnDef {
        id: "testing".into(),
        title: "重复".into(),
        on_enter: OnEnter::default(),
        transitions: Transitions::default(),
        wip_limit: None,
        timeout_ms: None,
    };
    assert!(matches!(reject(&mut board, KanbanCommand::ColumnAdd { column: dup }), KanbanError::ColumnExists(_)));
    // transitions 指向不存在的列
    let dangling = ColumnDef {
        id: "archive".into(),
        title: "归档".into(),
        on_enter: OnEnter::default(),
        transitions: Transitions { on_success: Some("nowhere".into()), on_failure: None },
        wip_limit: None,
        timeout_ms: None,
    };
    assert!(matches!(reject(&mut board, KanbanCommand::ColumnAdd { column: dangling }), KanbanError::ColumnNotFound(_)));
    assert_fail_closed(&board, &workspace, events, state);
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn illegal_run_commands_fail_closed() {
    let workspace = temp("runs");
    let mut board = open_board(&workspace);
    let card_id = create_card(&mut board, "x");
    let events = store::load_events(&store::events_path(&store::board_dir(&workspace, "board_t").unwrap())).unwrap().len();
    let state = serde_json::to_string(board.state()).unwrap();

    // human_gate 列不允许起 run
    assert!(matches!(reject(&mut board, KanbanCommand::RunStarted { card_id: card_id.clone() }), KanbanError::InvalidCommand(_)));
    // 未知 run
    assert!(matches!(
        reject(&mut board, KanbanCommand::RunFinished { run_id: "r_nope".into(), outcome: Outcome::Success }),
        KanbanError::RunNotOpen(_)
    ));
    assert!(matches!(reject(&mut board, KanbanCommand::RunTimeout { run_id: "r_nope".into() }), KanbanError::RunNotOpen(_)));
    // run_finished 不接受 Timeout（那是 run_timeout 的语义）
    assert!(matches!(
        reject(&mut board, KanbanCommand::RunFinished { run_id: "r".into(), outcome: Outcome::Timeout }),
        KanbanError::InvalidCommand(_)
    ));
    assert_fail_closed(&board, &workspace, events, state);

    // 已关闭的 run 不可二次 finish
    board.apply(KanbanCommand::CardMove { card_id: card_id.clone(), outcome: Outcome::Success }).unwrap();
    let run = board.apply(KanbanCommand::RunStarted { card_id: card_id.clone() }).unwrap();
    let EventKind::RunStarted(started) = &run.kind else { panic!("expected run_started") };
    board.apply(KanbanCommand::RunFinished { run_id: started.run_id.clone(), outcome: Outcome::Success }).unwrap();
    assert!(matches!(
        reject(&mut board, KanbanCommand::RunFinished { run_id: started.run_id.clone(), outcome: Outcome::Success }),
        KanbanError::RunNotOpen(_)
    ));
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn wip_limit_enforced_on_create_move_and_run_finish() {
    let workspace = temp("wip");
    let mut columns = default_template();
    columns[1].wip_limit = Some(1); // implementing
    columns[2].wip_limit = Some(1); // testing
    let mut board = Board::open(&workspace, "board_t").unwrap();
    board.apply(KanbanCommand::BoardCreate { title: "t".into(), columns: Some(columns) }).unwrap();
    let card_a = create_card(&mut board, "a");
    let card_b = create_card(&mut board, "b");

    // card_move 进满列
    board.apply(KanbanCommand::CardMove { card_id: card_a.clone(), outcome: Outcome::Success }).unwrap();
    assert!(matches!(
        reject(&mut board, KanbanCommand::CardMove { card_id: card_b.clone(), outcome: Outcome::Success }),
        KanbanError::WipLimit { .. }
    ));
    // card_create 直建进满列
    assert!(matches!(
        reject(&mut board, KanbanCommand::CardCreate { column_id: Some("implementing".into()), title: "c".into(), body: String::new() }),
        KanbanError::WipLimit { .. }
    ));
    // run_finished 推导目标列满：card_b 在 testing 占坑，card_a 的实现 run 成功也进不去
    board.apply(KanbanCommand::CardMove { card_id: card_b.clone(), outcome: Outcome::Success }).unwrap_err();
    // 把 card_b 从 requirements 直接建到 testing 需要先腾位：改为先移走 card_a
    let run = board.apply(KanbanCommand::RunStarted { card_id: card_a.clone() }).unwrap();
    let EventKind::RunStarted(started) = &run.kind else { panic!("expected run_started") };
    board.apply(KanbanCommand::RunFinished { run_id: started.run_id.clone(), outcome: Outcome::Success }).unwrap();
    assert_eq!(card(&board, &card_a).column_id, "testing");
    // 现在 testing 满（wip 1）：card_b 进 implementing 后跑成功也会被 WIP 挡住
    board.apply(KanbanCommand::CardMove { card_id: card_b.clone(), outcome: Outcome::Success }).unwrap();
    let run = board.apply(KanbanCommand::RunStarted { card_id: card_b.clone() }).unwrap();
    let EventKind::RunStarted(started) = &run.kind else { panic!("expected run_started") };
    assert!(matches!(
        reject(&mut board, KanbanCommand::RunFinished { run_id: started.run_id.clone(), outcome: Outcome::Success }),
        KanbanError::WipLimit { .. }
    ));
    // on_failure 目标是 requirements（无限额）：失败可以走
    board.apply(KanbanCommand::RunFinished { run_id: started.run_id.clone(), outcome: Outcome::Failure }).unwrap();
    assert_eq!(card(&board, &card_b).column_id, "requirements");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn run_timeout_blocks_card_and_retry_recovers() {
    let workspace = temp("timeout");
    let mut board = open_board(&workspace);
    let card_id = create_card(&mut board, "x");
    board.apply(KanbanCommand::CardMove { card_id: card_id.clone(), outcome: Outcome::Success }).unwrap();
    let run = board.apply(KanbanCommand::RunStarted { card_id: card_id.clone() }).unwrap();
    let EventKind::RunStarted(started) = &run.kind else { panic!("expected run_started") };
    board.apply(KanbanCommand::RunTimeout { run_id: started.run_id.clone() }).unwrap();
    // 超时进 blocked 停车，绝不永远 running
    assert_eq!(card(&board, &card_id).status, CardStatus::Blocked);
    assert_eq!(card(&board, &card_id).column_id, "implementing");
    assert_eq!(board.state().runs[&started.run_id].outcome, Some(Outcome::Timeout));
    // blocked 可重试：attempt 递增，成功后正常流转
    let run = board.apply(KanbanCommand::RunStarted { card_id: card_id.clone() }).unwrap();
    let EventKind::RunStarted(retry) = &run.kind else { panic!("expected run_started") };
    assert_eq!(retry.attempt, 2);
    assert_eq!(card(&board, &card_id).status, CardStatus::Running);
    board.apply(KanbanCommand::RunFinished { run_id: retry.run_id.clone(), outcome: Outcome::Success }).unwrap();
    assert_eq!(card(&board, &card_id).column_id, "testing");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn agent_defined_same_name_redefines_silently() {
    // redefine 是有意语义：AI 迭代修改定义依赖静默覆盖，第二次同名定义必须成功且投影为新定义
    let workspace = temp("redefine");
    let mut board = open_board(&workspace);
    let define = |role: &str, model: &str, permission_profile: &str| KanbanCommand::AgentDefined {
        name: "exec-impl".into(),
        role: role.into(),
        model: model.into(),
        permission_profile: permission_profile.into(),
        tools: None,
    };
    board.apply(define("execution", "auto", "full")).unwrap();
    board.apply(define("execution-v2", "sonnet", "readonly")).unwrap();
    let agent = &board.state().agents["exec-impl"];
    assert_eq!((agent.role.as_str(), agent.model.as_str(), agent.permission_profile.as_str()), ("execution-v2", "sonnet", "readonly"));
    assert_eq!(board.state().agents.len(), 1);
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn agent_defined_and_column_add_happy_path() {
    let workspace = temp("agents");
    let mut board = open_board(&workspace);
    board
        .apply(KanbanCommand::AgentDefined {
            name: "qa-verifier".into(),
            role: "review".into(),
            model: "auto".into(),
            permission_profile: "readonly+test".into(),
            tools: None,
        })
        .unwrap();
    assert!(board.state().agents.contains_key("qa-verifier"));
    assert!(matches!(
        reject(
            &mut board,
            KanbanCommand::AgentDefined {
                name: "bad name".into(),
                role: "r".into(),
                model: "m".into(),
                permission_profile: "p".into(),
                tools: None
            }
        ),
        KanbanError::InvalidId(_)
    ));
    let column = ColumnDef {
        id: "archive".into(),
        title: "归档".into(),
        on_enter: OnEnter { kind: OnEnterKind::None, agent: None },
        transitions: Transitions::default(),
        wip_limit: Some(20),
        timeout_ms: None,
    };
    board.apply(KanbanCommand::ColumnAdd { column }).unwrap();
    assert!(board.state().column("archive").is_some());
    std::fs::remove_dir_all(workspace).ok();
}
