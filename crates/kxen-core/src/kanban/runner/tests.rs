use super::*;
use crate::kanban::driver::tests as driver_tests;
use crate::kanban::events::*;
use crate::kanban::model::{CardStatus, OnEnterKind};
use crate::kanban::{Board, agents, store};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-runner-{tag}-{}-{nanos}", std::process::id()))
}

fn columns() -> Vec<crate::kanban::ColumnDef> {
    vec![
        driver_tests::column("implementing", OnEnterKind::AgentRun, Some("exec-impl"), Some("done"), Some("implementing"), None),
        driver_tests::column("done", OnEnterKind::None, None, None, None, None),
    ]
}

/// 手写事件流（append 指派 seq）：模拟「进程死亡遗留」的历史，时间戳早于任何 Runner。
fn write_legacy_events(workspace: &Path) {
    let path = store::events_path(&store::board_dir(workspace, "board_t").unwrap());
    let event =
        |id: &str, created_at: u64, kind: EventKind| KanbanEvent { id: id.into(), board_id: "board_t".into(), seq: 0, created_at, kind };
    let mut events = vec![
        event("kev_1", 1, EventKind::BoardCreate(BoardCreatePayload { title: "t".into(), columns: columns() })),
        event(
            "kev_2",
            2,
            EventKind::CardCreate(CardCreatePayload {
                card_id: "card_a".into(),
                column_id: "implementing".into(),
                title: "Add login".into(),
                body: String::new(),
            }),
        ),
        event(
            "kev_3",
            3,
            EventKind::RunStarted(RunStartedPayload {
                run_id: "board_t:card_a:implementing:1".into(),
                card_id: "card_a".into(),
                column_id: "implementing".into(),
                attempt: 1,
            }),
        ),
    ];
    for event in &mut events {
        store::append_event(&path, event).unwrap();
    }
}

#[tokio::test]
async fn orphaned_run_recovered_as_unknown_without_redispatch() {
    let workspace = temp("orphan");
    write_legacy_events(&workspace);
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let stream: crate::llm::StreamFn = Arc::new(move |_, _, _, _| {
        seen.fetch_add(1, Ordering::SeqCst);
        Box::pin(futures::stream::pending())
    });
    let launched = Runner::new().scan_once(&workspace, &driver_tests::deps(&workspace, stream)).await.unwrap();
    assert_eq!(launched, 0, "遗留 run 不得拉起执行");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "遗留 run 绝不自动重发（不猜结果）");
    let board = Board::open(&workspace, "board_t").unwrap();
    let run = &board.state().runs["board_t:card_a:implementing:1"];
    assert_eq!(run.outcome, Some(Outcome::Timeout), "orphan 按 Unknown 处置：run_timeout");
    let card = &board.state().cards["card_a"];
    assert_eq!(card.status, CardStatus::Blocked);
    assert!(card.comments.iter().any(|c| c.body.contains("UNKNOWN")), "恢复必须留审计评论");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn explicit_retry_claim_is_adopted_and_executed() {
    let workspace = temp("adopt");
    agents::save(&workspace, &driver_tests::agent_def()).unwrap();
    // Runner 先于 claim 创建（生产语义：runner 随进程启动，显式 claim 由之后的 RPC/工具发起）
    let runner = Runner::new();
    let mut board = Board::open(&workspace, "board_t").unwrap();
    board.apply(KanbanCommand::BoardCreate { title: "t".into(), columns: Some(columns()) }).unwrap();
    let card_id = driver_tests::create_card(&mut board, "implementing");
    // 显式重试的语义入口：外部 Command claim（started_at 不早于 boot，Runner 收养而非当 orphan）
    let event = board.apply(KanbanCommand::RunStarted { card_id: card_id.clone() }).unwrap();
    let EventKind::RunStarted(started) = event.kind else { panic!("expected run_started") };
    drop(board);
    let launched =
        runner.scan_once(&workspace, &driver_tests::deps(&workspace, driver_tests::text_stream("done\nVERDICT: success"))).await.unwrap();
    assert_eq!(launched, 1, "显式 claim 必须被收养执行");
    for _ in 0..100 {
        let board = Board::open(&workspace, "board_t").unwrap();
        if board.state().runs[&started.run_id].outcome.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let board = Board::open(&workspace, "board_t").unwrap();
    assert_eq!(board.state().runs[&started.run_id].outcome, Some(Outcome::Success));
    assert_eq!(board.state().cards[&card_id].column_id, "done");
    // 执行完成后不再重复拉起（handled 集）
    let launched =
        runner.scan_once(&workspace, &driver_tests::deps(&workspace, driver_tests::text_stream("again\nVERDICT: success"))).await.unwrap();
    assert_eq!(launched, 0, "已完结的 run 不得重跑");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn ready_card_runs_once_with_in_flight_dedup() {
    let workspace = temp("dedup");
    agents::save(&workspace, &driver_tests::agent_def()).unwrap();
    let mut board = Board::open(&workspace, "board_t").unwrap();
    board.apply(KanbanCommand::BoardCreate { title: "t".into(), columns: Some(columns()) }).unwrap();
    let _card_id = driver_tests::create_card(&mut board, "implementing");
    drop(board);
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    // 永不完成的流：第一个执行一直在飞，第二轮扫描不得重复拉起
    let stream: crate::llm::StreamFn = Arc::new(move |_, _, _, _| {
        seen.fetch_add(1, Ordering::SeqCst);
        Box::pin(futures::stream::pending())
    });
    let runner = Runner::new();
    let deps = driver_tests::deps(&workspace, stream);
    let first = runner.scan_once(&workspace, &deps).await.unwrap();
    let second = runner.scan_once(&workspace, &deps).await.unwrap();
    assert_eq!((first, second), (1, 0), "同一卡片同时只能有一个活跃 run");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1, "LLM 流只能发起一次");
    std::fs::remove_dir_all(workspace).ok();
}
