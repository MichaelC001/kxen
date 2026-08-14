//! P6 端到端（design.md 验证节）：默认模板全链路自动流转 + 两卡 worktree 并发隔离。
//! 挂 kanban 根而非 runner/tests.rs：后者聚焦调度分支且守 350 行门禁，这里跨 driver/runner/store 全链路。

use super::driver::tests as driver_tests;
use super::events::*;
use super::model::{CardStatus, OnEnterKind};
use super::runner::Runner;
use super::{Board, agents, store};
use futures::StreamExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-e2e-{tag}-{}-{nanos}", std::process::id()))
}

/// 轮询投影直到条件成立：列执行在 tokio::spawn 里异步落地，定长 sleep 在并行负载下不可靠。
async fn wait_until(what: &str, mut check: impl FnMut() -> bool) {
    for _ in 0..1500 {
        if check() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

/// 全链路：默认模板建卡 -> approve 放行 -> 两段 agent_run 自动流转到待验证 -> 停车 -> 人工 approve -> 完成。
#[tokio::test]
async fn default_template_full_chain_parks_at_human_gates() {
    let workspace = temp("chain");
    // 默认模板 implementing/testing 各引一个列 Agent 定义；role 保持 execution（测试 config 只配这个路由键）
    for name in ["execution", "qa"] {
        let mut def = driver_tests::agent_def();
        def.name = name.into();
        agents::save(&workspace, &def).unwrap();
    }
    let mut board = Board::open(&workspace, "board_t").unwrap();
    board.apply(KanbanCommand::BoardCreate { title: "链".into(), columns: None }).unwrap();
    let card_id = driver_tests::create_card(&mut board, "requirements");
    assert_eq!(board.state().cards[&card_id].status, CardStatus::WaitingHuman, "human_gate 列进卡即停车");
    // 人工 approve（kanban.card_move RPC 的核心命令）-> implementing（Ready 等 runner 派发）
    board.apply(KanbanCommand::CardMove { card_id: card_id.clone(), outcome: Outcome::Success }).unwrap();
    drop(board);

    let deps = driver_tests::deps(&workspace, driver_tests::text_stream("done\nVERDICT: success"));
    let runner = Runner::new();
    assert_eq!(runner.scan_once(&workspace, &deps).await.unwrap(), 1, "implementing 列应派发一个 run");
    wait_until("card reaches testing", || Board::open(&workspace, "board_t").unwrap().state().cards[&card_id].column_id == "testing").await;
    assert_eq!(runner.scan_once(&workspace, &deps).await.unwrap(), 1, "testing 列应派发一个 run");
    wait_until("card reaches review", || Board::open(&workspace, "board_t").unwrap().state().cards[&card_id].column_id == "review").await;
    // 停车断言：human_gate 列绝不自动放行，再扫无新 run
    assert_eq!(runner.scan_once(&workspace, &deps).await.unwrap(), 0, "review 人工门必须停车");
    let board = Board::open(&workspace, "board_t").unwrap();
    assert_eq!(board.state().cards[&card_id].status, CardStatus::WaitingHuman);
    assert_eq!(board.state().runs.len(), 2, "停车不得产生第三个 run");
    drop(board);

    // 人工 approve -> done（终态无出边）
    let mut board = Board::open(&workspace, "board_t").unwrap();
    board.apply(KanbanCommand::CardMove { card_id: card_id.clone(), outcome: Outcome::Success }).unwrap();
    assert_eq!(board.state().cards[&card_id].column_id, "done");
    // run_id 派生：board:card:column:attempt
    let impl_run = format!("board_t:{card_id}:implementing:1");
    let test_run = format!("board_t:{card_id}:testing:1");
    assert_eq!(board.state().runs[&impl_run].outcome, Some(Outcome::Success));
    assert_eq!(board.state().runs[&test_run].outcome, Some(Outcome::Success));
    let landed = serde_json::to_string(board.state()).unwrap();
    drop(board);

    // 事件流可回放：快照（纯缓存）删除后两次从事件流重建，序列化逐字节一致
    let dir = store::board_dir(&workspace, "board_t").unwrap();
    std::fs::remove_file(store::snapshot_path(&dir)).unwrap();
    let first = serde_json::to_string(Board::open(&workspace, "board_t").unwrap().state()).unwrap();
    let second = serde_json::to_string(Board::open(&workspace, "board_t").unwrap().state()).unwrap();
    assert_eq!(first, landed, "事件流重建必须等于落地态");
    assert_eq!(second, landed, "重复重建必须确定");
    // 事件序列：seq 从 1 连续；每个 run 的 claim 先于 outcome；两次人工 approve 都入流
    let events = store::load_events(&store::events_path(&dir)).unwrap();
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event.seq, index as u64 + 1, "事件 seq 必须连续");
    }
    for run_id in [&impl_run, &test_run] {
        let started = events.iter().position(|e| matches!(&e.kind, EventKind::RunStarted(p) if &p.run_id == run_id)).unwrap();
        let finished = events.iter().position(|e| matches!(&e.kind, EventKind::RunFinished(p) if &p.run_id == run_id)).unwrap();
        assert!(started < finished, "两阶段落点：{run_id} 的 claim 必须先于 outcome");
    }
    let moves = events.iter().filter(|e| matches!(&e.kind, EventKind::CardMove(p) if p.card_id == card_id)).count();
    assert_eq!(moves, 2, "requirements/review 两次人工 approve 都必须以 card_move 入流");
    std::fs::remove_dir_all(workspace).ok();
}

fn git(workspace: &Path, args: &[&str]) {
    let out = std::process::Command::new("git").args(args).current_dir(workspace).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
}

/// git temp 仓库：init + 初始 commit（worktree add 需要已 born 的 HEAD）。
fn git_repo(tag: &str) -> PathBuf {
    let workspace = temp(tag);
    std::fs::create_dir_all(&workspace).unwrap();
    git(&workspace, &["init"]);
    // commit.gpgsign=false：宿主全局 gitconfig 可能开签名（如 1Password op-ssh-sign），测试环境签名必挂
    git(&workspace, &["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "--allow-empty", "-m", "init"]);
    workspace
}

fn create_card_titled(board: &mut Board, title: &str) -> String {
    let event = board
        .apply(KanbanCommand::CardCreate { column_id: Some("implementing".into()), title: title.into(), body: String::new() })
        .unwrap();
    let EventKind::CardCreate(payload) = event.kind else { panic!("expected card_create") };
    payload.card_id
}

/// 并发隔离：同一 agent_run 列两卡一轮扫描双派发，各自在卡专属 worktree 写独有文件互不踩踏。
#[tokio::test]
async fn two_cards_run_concurrently_in_isolated_worktrees() {
    let workspace = git_repo("concur");
    agents::save(&workspace, &driver_tests::agent_def()).unwrap();
    let mut board = Board::open(&workspace, "board_t").unwrap();
    board
        .apply(KanbanCommand::BoardCreate {
            title: "t".into(),
            columns: Some(vec![
                // 列超时 10s 是死锁保险丝：rendezvous 失败时 run 超时停车、wait_until 断言超时，测试 fail-closed
                driver_tests::column(
                    "implementing",
                    OnEnterKind::AgentRun,
                    Some("exec-impl"),
                    Some("review"),
                    Some("implementing"),
                    Some(10_000),
                ),
                // review 非终态（有出边）：run 落地后 worktree 保留，落后再断言内容
                driver_tests::column("review", OnEnterKind::None, None, Some("done"), None, None),
                driver_tests::column("done", OnEnterKind::None, None, None, None, None),
            ]),
        })
        .unwrap();
    let card_a = create_card_titled(&mut board, "AlphaCard unique file");
    let card_b = create_card_titled(&mut board, "BetaCard unique file");
    drop(board);
    let wt_a = workspace.join(".kxen").join("worktrees").join(format!("card-{card_a}"));
    let wt_b = workspace.join(".kxen").join("worktrees").join(format!("card-{card_b}"));
    assert_ne!(wt_a, wt_b, "两卡 worktree 路径必须按 card_id 派生");

    // 隔离违例记入列表而非流内 panic：run 照常落地，断言集中在测试尾部给出完整清单
    let violations = Arc::new(Mutex::new(Vec::<String>::new()));
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let stream: crate::llm::StreamFn = {
        let (violations, barrier) = (violations.clone(), barrier.clone());
        let (wt_a, wt_b) = (wt_a.clone(), wt_b.clone());
        Arc::new(move |_, messages, _, _| {
            // 渲染上下文含卡标题，据此区分本 run 属于哪张卡，只写自己 worktree 的独有文件
            let prompt: String = messages.iter().map(|m| m.content.as_str()).collect();
            let (mine, other) = if prompt.contains("AlphaCard") {
                (wt_a.join("alpha-only.txt"), wt_a.join("beta-only.txt"))
            } else if prompt.contains("BetaCard") {
                (wt_b.join("beta-only.txt"), wt_b.join("alpha-only.txt"))
            } else {
                panic!("prompt must identify the card: {prompt}");
            };
            let (barrier, violations) = (barrier.clone(), violations.clone());
            let future = async move {
                // rendezvous：两个流都到达才放行，并发执行窗口真实存在（串行执行会卡死 -> 列超时 -> 测试失败）
                barrier.wait().await;
                std::fs::write(&mine, "mine").unwrap();
                // 第二次 rendezvous：双方独有文件都已落盘，再查对方文件（否则检查时序碰运气）
                barrier.wait().await;
                if other.exists() {
                    crate::core::shared::lock(&violations).push(format!("{} leaked into {}", other.display(), mine.display()));
                }
                vec![crate::llm::Delta::Text("done\nVERDICT: success".into()), crate::llm::Delta::Done]
            };
            Box::pin(futures::stream::once(future).flat_map(futures::stream::iter))
        })
    };
    let deps = driver_tests::deps(&workspace, stream);
    let runner = Runner::new();
    assert_eq!(runner.scan_once(&workspace, &deps).await.unwrap(), 2, "一轮扫描必须双派发");
    wait_until("both runs land", || {
        let board = Board::open(&workspace, "board_t").unwrap();
        board.state().runs.len() == 2 && board.state().runs.values().all(|run| run.outcome.is_some())
    })
    .await;
    let board = Board::open(&workspace, "board_t").unwrap();
    assert!(board.state().runs.values().all(|run| run.outcome == Some(Outcome::Success)), "双卡都必须成功: {:?}", board.state().runs);
    assert_eq!(board.state().cards[&card_a].column_id, "review");
    assert_eq!(board.state().cards[&card_b].column_id, "review");
    drop(board);
    assert!(crate::core::shared::lock(&violations).is_empty(), "worktree 互相踩踏: {:?}", crate::core::shared::lock(&violations));
    // 双卡成功落地后 worktree 保留（review 非终态），各自只含自己的独有文件
    assert_eq!(std::fs::read_to_string(wt_a.join("alpha-only.txt")).unwrap(), "mine");
    assert!(!wt_a.join("beta-only.txt").exists());
    assert_eq!(std::fs::read_to_string(wt_b.join("beta-only.txt")).unwrap(), "mine");
    assert!(!wt_b.join("alpha-only.txt").exists());
    std::fs::remove_dir_all(workspace).ok();
}
