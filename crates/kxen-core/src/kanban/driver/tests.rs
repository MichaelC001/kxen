use super::*;
use crate::kanban::events::*;
use crate::kanban::model::{CardStatus, ColumnDef, OnEnter, OnEnterKind, Transitions};
use crate::kanban::{Board, agents, store};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-driver-{tag}-{}-{nanos}", std::process::id()))
}

pub(crate) fn agent_def() -> agents::AgentDefinition {
    agents::AgentDefinition {
        name: "exec-impl".into(),
        role: "execution".into(),
        model: "auto".into(),
        permission_profile: "full".into(),
        tools: None,
        prompt: "Implement the card, then declare the verdict.".into(),
    }
}

pub(crate) fn column(
    id: &str,
    kind: OnEnterKind,
    agent: Option<&str>,
    success: Option<&str>,
    failure: Option<&str>,
    timeout_ms: Option<u64>,
) -> ColumnDef {
    ColumnDef {
        id: id.into(),
        title: id.into(),
        on_enter: OnEnter { kind, agent: agent.map(str::to_string) },
        transitions: Transitions { on_success: success.map(str::to_string), on_failure: failure.map(str::to_string) },
        wip_limit: None,
        timeout_ms,
    }
}

pub(crate) fn agent_board(workspace: &Path, timeout_ms: Option<u64>) -> Board {
    let mut board = Board::open(workspace, "board_t").unwrap();
    board
        .apply(KanbanCommand::BoardCreate {
            title: "t".into(),
            columns: Some(vec![
                column("implementing", OnEnterKind::AgentRun, Some("exec-impl"), Some("done"), Some("implementing"), timeout_ms),
                column("done", OnEnterKind::None, None, None, None, None),
            ]),
        })
        .unwrap();
    board
}

pub(crate) fn create_card(board: &mut Board, column_id: &str) -> String {
    let event = board
        .apply(KanbanCommand::CardCreate { column_id: Some(column_id.into()), title: "Add login".into(), body: "Email login".into() })
        .unwrap();
    let EventKind::CardCreate(payload) = event.kind else { panic!("expected card_create") };
    payload.card_id
}

pub(crate) fn text_stream(text: &'static str) -> crate::llm::StreamFn {
    Arc::new(move |_, _, _, _| {
        Box::pin(futures::stream::iter(vec![
            crate::llm::Delta::Text(text.into()),
            crate::llm::Delta::Usage { input: 1, output: 1 },
            crate::llm::Delta::Done,
        ]))
    })
}

pub(crate) fn deps(workspace: &Path, stream: crate::llm::StreamFn) -> DriverDeps {
    let mut config = crate::core::config::Config::default();
    config
        .roles
        .insert("execution".into(), crate::core::config::RoleBinding { provider: "p".into(), model: "m".into(), ..Default::default() });
    DriverDeps {
        registry: Arc::new(crate::tools::task::TaskRegistry::new()),
        workdir: Arc::from(workspace),
        store: Arc::new(crate::auth::credential::AuthStore::default()),
        mrm: Arc::new(crate::llm::mrm::ModelResourceManager::new(config)),
        hooks: None,
        bus: crate::core::event::EventBus::default(),
        approvals: None,
        agents: Arc::new(crate::agent::activity::AgentRegistry::default()),
        mcp: None,
        lsp: None,
        stream_override: Some(stream),
        usage_reporter: None,
    }
}

#[tokio::test]
async fn end_to_end_success_claim_precedes_llm_and_outcome_lands() {
    let workspace = temp("e2e");
    agents::save(&workspace, &agent_def()).unwrap();
    let mut board = agent_board(&workspace, None);
    let card_id = create_card(&mut board, "implementing");
    drop(board);
    let events_file = store::events_path(&store::board_dir(&workspace, "board_t").unwrap());
    let check = events_file.clone();
    let stream: crate::llm::StreamFn = Arc::new(move |_, _, _, _| {
        // 完成协议第一阶段：LLM 请求发起前 run_started 必须已 durable
        // （P4 起 claim 与 LLM 之间可能插入 kanban-driver 的 worktree 降级评论，只断言 claim 先于 LLM）
        let events = store::load_events(&check).unwrap();
        assert!(events.iter().any(|e| matches!(&e.kind, EventKind::RunStarted(_))), "LLM 请求前必须先落 run_started");
        Box::pin(futures::stream::iter(vec![
            crate::llm::Delta::Text("implemented\nVERDICT: success".into()),
            crate::llm::Delta::Usage { input: 1, output: 1 },
            crate::llm::Delta::Done,
        ]))
    });
    let landing = execute(&workspace, "board_t", &card_id, &deps(&workspace, stream), None).await.unwrap();
    let run_id = landing.run_id.clone();
    assert_eq!(landing.kind, LandingKind::Finished(Outcome::Success));
    let board = Board::open(&workspace, "board_t").unwrap();
    assert_eq!(board.state().cards[&card_id].column_id, "done", "on_success 流转到 done");
    // 两阶段落点顺序：同一 run_id 的 run_started 先于 run_finished
    let events = store::load_events(&events_file).unwrap();
    let started = events.iter().position(|e| matches!(&e.kind, EventKind::RunStarted(p) if p.run_id == run_id)).unwrap();
    let finished = events.iter().position(|e| matches!(&e.kind, EventKind::RunFinished(p) if p.run_id == run_id)).unwrap();
    assert!(started < finished);
    // turns JSONL 增量自持久化：u + final（无工具调用故无 t*）
    let turns = crate::core::session::load_lines(&turns_path(&workspace, "board_t", &run_id).unwrap()).unwrap();
    let ids: Vec<&str> = turns.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, [format!("{run_id}:u"), format!("{run_id}:final")]);
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn missing_verdict_lands_failure_with_comment() {
    let workspace = temp("noverdict");
    agents::save(&workspace, &agent_def()).unwrap();
    let mut board = agent_board(&workspace, None);
    let card_id = create_card(&mut board, "implementing");
    drop(board);
    let landing =
        execute(&workspace, "board_t", &card_id, &deps(&workspace, text_stream("some output without protocol")), None).await.unwrap();
    assert_eq!(landing.kind, LandingKind::Finished(Outcome::Failure), "跑完未声明 VERDICT 落 Failure");
    let board = Board::open(&workspace, "board_t").unwrap();
    let card = &board.state().cards[&card_id];
    assert_eq!(card.column_id, "implementing", "on_failure 回流");
    assert_eq!(card.status, CardStatus::Ready);
    assert!(card.comments.iter().any(|c| c.body.contains("VERDICT")), "未声明 verdict 必须有审计评论");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn timeout_lands_blocked_and_explicit_retry_recovers() {
    let workspace = temp("timeout");
    agents::save(&workspace, &agent_def()).unwrap();
    let mut board = agent_board(&workspace, Some(200));
    let card_id = create_card(&mut board, "implementing");
    drop(board);
    let pending: crate::llm::StreamFn = Arc::new(|_, _, _, _| Box::pin(futures::stream::pending()));
    let landing = execute(&workspace, "board_t", &card_id, &deps(&workspace, pending), None).await.unwrap();
    assert_eq!(landing.kind, LandingKind::TimedOut);
    let mut board = Board::open(&workspace, "board_t").unwrap();
    assert_eq!(board.state().runs[&landing.run_id].outcome, Some(Outcome::Timeout));
    assert_eq!(board.state().cards[&card_id].status, CardStatus::Blocked, "超时进 blocked，绝不永远 running");
    // 显式重试：新 claim（attempt 2），driver 收养执行，成功后正常流转
    let event = board.apply(KanbanCommand::RunStarted { card_id: card_id.clone() }).unwrap();
    let EventKind::RunStarted(started) = event.kind else { panic!("expected run_started") };
    assert_eq!(started.attempt, 2);
    drop(board);
    let retry =
        execute(&workspace, "board_t", &card_id, &deps(&workspace, text_stream("fixed\nVERDICT: success")), Some(started.run_id.clone()))
            .await
            .unwrap();
    assert_eq!(retry.kind, LandingKind::Finished(Outcome::Success));
    assert_eq!(retry.run_id, started.run_id, "收养路径不二次 claim");
    let board = Board::open(&workspace, "board_t").unwrap();
    assert_eq!(board.state().cards[&card_id].column_id, "done");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn persistence_failure_lands_unknown_not_failure() {
    let workspace = temp("persist");
    agents::save(&workspace, &agent_def()).unwrap();
    let mut board = agent_board(&workspace, None);
    let card_id = create_card(&mut board, "implementing");
    drop(board);
    // run_id 确定性派生（attempt 1）：预造同名目录让 turns JSONL 追加必失败
    let run_id = format!("board_t:{card_id}:implementing:1");
    std::fs::create_dir_all(turns_path(&workspace, "board_t", &run_id).unwrap()).unwrap();
    let landing = execute(&workspace, "board_t", &card_id, &deps(&workspace, text_stream("x\nVERDICT: success")), None).await.unwrap();
    assert_eq!(landing.kind, LandingKind::TimedOut, "持久化失败按 Unknown 处置（run_timeout），不得落 Failure");
    let board = Board::open(&workspace, "board_t").unwrap();
    assert_eq!(board.state().runs[&run_id].outcome, Some(Outcome::Timeout));
    assert_eq!(board.state().cards[&card_id].status, CardStatus::Blocked);
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn workflow_kind_derives_journal_run_id() {
    let workspace = temp("workflow");
    let mut wf = agent_def();
    wf.name = "wf-col".into();
    wf.prompt = "const answer = await agent(\"execution\", \"reply PONG\");\nreturn \"wf-done:\" + answer;".into();
    agents::save(&workspace, &wf).unwrap();
    let mut board = Board::open(&workspace, "board_t").unwrap();
    board
        .apply(KanbanCommand::BoardCreate {
            title: "t".into(),
            columns: Some(vec![
                column("building", OnEnterKind::Workflow, Some("wf-col"), Some("done"), Some("building"), None),
                column("done", OnEnterKind::None, None, None, None, None),
            ]),
        })
        .unwrap();
    let card_id = create_card(&mut board, "building");
    drop(board);
    let run_id = format!("board_t:{card_id}:building:1");
    let journal = crate::core::paths::KxenPaths::user().workflow_journal(&scoped_journal_id(&run_id));
    let _ = std::fs::remove_file(&journal);
    let _ = std::fs::remove_file(journal.with_extension("jsonl.lock"));
    let landing = execute(&workspace, "board_t", &card_id, &deps(&workspace, text_stream("PONG")), None).await.unwrap();
    assert_eq!(landing.kind, LandingKind::Finished(Outcome::Success));
    assert!(journal.exists(), "journal 必须按 P1 派生 run_id（board:card:column:attempt）建立: {}", journal.display());
    let turns = crate::core::session::load_lines(&turns_path(&workspace, "board_t", &run_id).unwrap()).unwrap();
    let ids: Vec<&str> = turns.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, [format!("{run_id}:u"), format!("{run_id}:final")]);
    let _ = std::fs::remove_file(&journal);
    let _ = std::fs::remove_file(journal.with_extension("jsonl.lock"));
    std::fs::remove_dir_all(workspace).ok();
}

/// journal 命名空间（workflow_journal.rs open_scoped，session_id=None -> "no-session"）：
/// sha256 多段哈希，段间 0 分隔。
fn scoped_journal_id(run_id: &str) -> String {
    use sha2::Digest;
    let mut hash = sha2::Sha256::new();
    for segment in ["no-session", run_id] {
        hash.update(segment.as_bytes());
        hash.update([0u8]);
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[tokio::test]
async fn claim_and_landing_publish_kanban_update() {
    let workspace = temp("bus");
    agents::save(&workspace, &agent_def()).unwrap();
    let mut board = agent_board(&workspace, None);
    let card_id = create_card(&mut board, "implementing");
    drop(board);
    let driver_deps = deps(&workspace, text_stream("done\nVERDICT: success"));
    let mut events = driver_deps.bus.subscribe();
    execute(&workspace, "board_t", &card_id, &driver_deps, None).await.unwrap();
    let mut updates = Vec::new();
    loop {
        match events.try_recv() {
            Ok(crate::core::event::Event::KanbanUpdate { board_id, workspace: ws }) => updates.push((board_id, ws)),
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    // claim 与 outcome 落地各补发一次（中途审计评论落盘成功会再补，只断言下限与内容）
    assert!(updates.len() >= 2, "claim 与落地都必须补发 KanbanUpdate: {updates:?}");
    assert!(updates.iter().all(|(board_id, ws)| board_id == "board_t" && *ws == workspace.to_string_lossy()));
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn parse_verdict_reads_last_declaration() {
    assert_eq!(parse_verdict("work done\nVERDICT: success"), Some(Outcome::Success));
    assert_eq!(parse_verdict("VERDICT: failure\nreasons"), Some(Outcome::Failure));
    assert_eq!(parse_verdict("VERDICT: success\nchanged mind\nVERDICT: failure"), Some(Outcome::Failure), "以最后声明为准");
    assert_eq!(parse_verdict("verdict: success"), Some(Outcome::Success), "大小写不敏感");
    assert_eq!(parse_verdict("no verdict here"), None);
}

/// custom profile 列执行：定义即挂载（spec 含 deferred 的 lsp、不含白名单外 exec），
/// 模型伪造白名单外 tool_call 被 tool_permitted 拒，错误进 turn 记录，run 按协议走到终态。
#[tokio::test]
async fn custom_profile_mounts_deferred_specs_and_rejects_forged_calls() {
    let workspace = temp("custom");
    let mut definition = agent_def();
    definition.permission_profile = "custom".into();
    definition.tools = Some(vec!["read".into(), "glob".into(), "grep".into(), "edit".into(), "lsp".into()]);
    agents::save(&workspace, &definition).unwrap();
    let mut board = agent_board(&workspace, None);
    let card_id = create_card(&mut board, "implementing");
    drop(board);
    let captured = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let specs = captured.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let stream: crate::llm::StreamFn = Arc::new(move |_, _, tools, _| {
        if seen.fetch_add(1, AtomicOrdering::SeqCst) == 0 {
            *specs.lock().unwrap() = tools.iter().map(|tool| tool.function.name.clone()).collect();
            // 伪造白名单外的 exec：执行侧白名单复验必须拦下
            Box::pin(futures::stream::iter(vec![
                crate::llm::Delta::ToolFragments(vec![crate::llm::tool::ChunkToolCall {
                    index: Some(0),
                    id: Some("call_1".into()),
                    function: Some(crate::llm::tool::ChunkFunction {
                        name: Some("exec".into()),
                        arguments: Some(r#"{"command":"echo forged"}"#.into()),
                    }),
                }]),
                crate::llm::Delta::Done,
            ]))
        } else {
            Box::pin(futures::stream::iter(vec![
                crate::llm::Delta::Text("done\nVERDICT: success".into()),
                crate::llm::Delta::Usage { input: 1, output: 1 },
                crate::llm::Delta::Done,
            ]))
        }
    });
    let landing = execute(&workspace, "board_t", &card_id, &deps(&workspace, stream), None).await.unwrap();
    assert_eq!(landing.kind, LandingKind::Finished(Outcome::Success), "伪造调用被拒后 run 按协议走向终态");
    let names = captured.lock().unwrap();
    assert!(names.iter().any(|n| n == "lsp"), "custom 工具集必须挂载 deferred 的 lsp: {names:?}");
    assert!(names.iter().any(|n| n == "read"));
    assert!(!names.iter().any(|n| n == "exec"), "白名单外的 exec 不得出现在 spec: {names:?}");
    drop(names);
    let turns = crate::core::session::load_lines(&turns_path(&workspace, "board_t", &landing.run_id).unwrap()).unwrap();
    let serialized = serde_json::to_string(&turns).unwrap();
    assert!(serialized.contains("tool not allowed"), "被拒的 exec 错误必须进 turn 记录: {serialized}");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn config_error_lands_failure_without_llm_call() {
    let workspace = temp("config");
    // 不保存 agent 定义文件：列引用了不存在的定义
    let mut board = agent_board(&workspace, None);
    let card_id = create_card(&mut board, "implementing");
    drop(board);
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let stream: crate::llm::StreamFn = Arc::new(move |_, _, _, _| {
        seen.fetch_add(1, AtomicOrdering::SeqCst);
        Box::pin(futures::stream::iter(vec![crate::llm::Delta::Done]))
    });
    let landing = execute(&workspace, "board_t", &card_id, &deps(&workspace, stream), None).await.unwrap();
    assert_eq!(landing.kind, LandingKind::Finished(Outcome::Failure));
    assert_eq!(calls.load(AtomicOrdering::SeqCst), 0, "配置错误不得发起 LLM 请求");
    let board = Board::open(&workspace, "board_t").unwrap();
    assert!(board.state().cards[&card_id].comments.iter().any(|c| c.body.contains("agent definition")), "配置错误必须有审计评论");
    std::fs::remove_dir_all(workspace).ok();
}
