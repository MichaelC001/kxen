// run 主循环直接单测：stream_override 注入假流，覆盖终态/重试/预算分支。

use kxen_core::agent::agent_loop::{AgentContext, AgentEvent, run_turn};
use kxen_core::llm::types::Delta;
use kxen_core::llm::{ModelRef, StreamFn};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// 进程级隔离 goals 目录：Once 写序同值无竞态（与 KXEN_AUTH_FILE 规约一致）。
/// 不设会读到用户真实 goals 目录（record_goal_turn 按 session 焦点记账，可能误动真数据）。
fn goals_dir_isolation() -> std::path::PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let dir = std::env::temp_dir().join(format!("kxen-run-loop-{}", std::process::id()));
    ONCE.call_once(|| unsafe {
        std::env::set_var("KXEN_GOALS_DIR", &dir);
        std::env::set_var("KXEN_SESSIONS_DIR", dir.join("sessions"));
        // usage 台账必须放子目录：goal store 的 list_checked 会把 goals 目录下
        // 所有 *.json 当 Goal 解析，usage.json 混在同目录会被误读成缺 id 的 Goal（并发竞态）
        std::env::set_var("KXEN_USAGE_FILE", dir.join("ledger").join("usage.json"));
        std::env::set_var("KXEN_USAGE_TREND_FILE", dir.join("ledger").join("usage-trend.json"));
    });
    dir
}

/// 假流工厂：每次调用按序弹出一段脚本 Delta（弹空给 Done 兜底），calls 记调用次数。
fn scripted(scripts: Vec<Vec<Delta>>, calls: Arc<AtomicUsize>) -> StreamFn {
    let scripts = Arc::new(Mutex::new(VecDeque::from(scripts)));
    Arc::new(move |_model, _messages, _tools, _store| {
        calls.fetch_add(1, Ordering::SeqCst);
        let deltas = kxen_core::core::shared::lock(&scripts).pop_front().unwrap_or_else(|| vec![Delta::Done]);
        Box::pin(futures::stream::iter(deltas))
    })
}

/// session 绑定的 goal 用量结算过 live Session admission（load_meta 查活），先落真实会话。
fn create_test_session() -> String {
    kxen_core::core::session::create(&kxen_core::core::paths::sessions_dir(), "/tmp").expect("create session").id
}

fn test_ctx(stream: StreamFn, session_id: &str) -> AgentContext {
    AgentContext {
        registry: Arc::new(kxen_core::tools::task::TaskRegistry::new()),
        tracker: kxen_core::tools::fs_tool::FileTracker::default(),
        workdir: Arc::from(Path::new("/tmp")),
        child_env: None,
        path_grants: Arc::new(Default::default()),
        path_scope: None,
        model: ModelRef::new("p", "m"),
        store: kxen_core::auth::credential::AuthStore::default().into(),
        max_turns: 4,
        max_pure_retries: None,
        mrm: None,
        allowed_tools: None,
        extras: None,
        hooks: None,
        loop_detector: kxen_core::agent::loop_detect::LoopDetector::new(),
        cancel: None,
        team: None,
        team_identity: None,
        session_id: Some(session_id.into()),
        exec_scope: None,
        bound_goal_id: None,
        goal_binding_frozen: false,
        agents: None,
        bus: None,
        approvals: None,
        kanban_auto: None,
        mcp: None,
        mcp_approval_prechecked: false,
        lsp: None,
        notify: None,
        persist_compaction: None,
        persist_turn: None,
        tool_journal: None,
        domain_tools: None,
        auxiliary_usage: Arc::default(),
        usage_reporter: None,
        on_event: Arc::new(|_| {}),
        stream_override: Some(stream),
    }
}

/// 终态分支：不可重试错误直接落终态文本与 Error 事件（run 不许无声结束）。
#[tokio::test]
async fn non_retryable_error_lands_terminal() {
    goals_dir_isolation();
    let calls = Arc::new(AtomicUsize::new(0));
    let stream = scripted(vec![vec![Delta::Error("anthropic credential missing (run doctor)".into())]], calls.clone());
    let mut ctx = test_ctx(stream, "run-terminal");
    let mut messages = Vec::new();
    let out = run_turn(&mut ctx, &mut messages).await;
    assert_eq!(out.final_text, "(错误: anthropic credential missing (run doctor))");
    assert!(matches!(out.terminal, AgentEvent::Error { .. }), "terminal 必须是 Error");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "不可重试错误不得二次调用");
}

/// 重试分支：429 零产出可重试，第二次 attempt 成功则正常 Done。
/// （退避 sleep 真实等待 ~1s：tokio 未开 test-util 特性，不能用 start_paused 跳过）
#[tokio::test]
async fn retryable_error_recovers_on_next_attempt() {
    goals_dir_isolation();
    let calls = Arc::new(AtomicUsize::new(0));
    let stream = scripted(
        vec![
            vec![Delta::Error("xai HTTP 429: too many requests".into())],
            vec![Delta::Text("ok".into()), Delta::Usage { input: 1, output: 1 }, Delta::Done],
        ],
        calls.clone(),
    );
    let mut ctx = test_ctx(stream, "run-retry");
    let mut messages = Vec::new();
    let out = run_turn(&mut ctx, &mut messages).await;
    assert_eq!(out.final_text, "ok");
    assert!(matches!(out.terminal, AgentEvent::Done { .. }), "重试成功应 Done，实际 {:?}", out.terminal);
    assert_eq!(calls.load(Ordering::SeqCst), 2, "429 应触发一次重试");
}

#[tokio::test]
async fn ambiguous_transport_failure_is_never_automatically_replayed() {
    goals_dir_isolation();
    let calls = Arc::new(AtomicUsize::new(0));
    let stream = scripted(
        vec![
            vec![Delta::Error("request failed: connection reset after request write".into())],
            vec![Delta::Text("duplicate".into()), Delta::Done],
        ],
        calls.clone(),
    );
    let mut ctx = test_ctx(stream, "run-ambiguous-reset");
    let mut messages = Vec::new();

    let out = run_turn(&mut ctx, &mut messages).await;

    assert!(matches!(out.terminal, AgentEvent::Error { .. }));
    assert!(out.final_text.contains("connection reset"));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "ambiguous post-send failure requires an explicit user retry");
}

#[tokio::test]
async fn usage_observation_disables_even_an_explicit_rate_limit_retry() {
    goals_dir_isolation();
    let calls = Arc::new(AtomicUsize::new(0));
    let stream = scripted(
        vec![
            vec![Delta::Usage { input: 7, output: 0 }, Delta::Error("HTTP 429 rate limit".into())],
            vec![Delta::Text("duplicate".into()), Delta::Done],
        ],
        calls.clone(),
    );
    let mut ctx = test_ctx(stream, "run-rate-limit-with-usage");
    let mut messages = Vec::new();

    let out = run_turn(&mut ctx, &mut messages).await;

    assert!(matches!(out.terminal, AgentEvent::Error { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "observed usage proves the request crossed the billing boundary");
}

#[tokio::test]
async fn explicit_auth_rejection_settles_transactional_zero_without_unknown_usage() {
    let dir = goals_dir_isolation();
    std::fs::create_dir_all(&dir).expect("mkdir");
    let goal_id = "run-known-zero-goal";
    let session = kxen_core::core::session::create(&kxen_core::core::paths::sessions_dir(), "/tmp").expect("create session");
    let session_id = session.id;
    let mut goal = kxen_core::core::goal::Goal::create(
        kxen_core::core::goal::GoalContract {
            objective: "o".into(),
            completion_criteria: "c".into(),
            constraints: None,
            budget: kxen_core::core::goal::GoalBudget { tokens: Some(1), ..Default::default() },
        },
        goal_id.into(),
    )
    .expect("create");
    goal.activate().expect("activate");
    goal.session_id = Some(session_id.clone());
    goal.save(&dir).expect("save");

    let calls = Arc::new(AtomicUsize::new(0));
    let stream = scripted(vec![vec![Delta::Error("p HTTP 401: rejected before inference".into())]], calls.clone());
    let usage = Arc::new(Mutex::new(Default::default()));
    let attempts = dir.join("known-zero-attempts");
    let mut ctx = test_ctx(stream, &session_id);
    ctx.usage_reporter = Some(kxen_core::agent::agent_loop::UsageReporter::new_in(
        session_id.clone(),
        usage.clone(),
        kxen_core::core::event::EventBus::default(),
        attempts.clone(),
    ));
    let mut messages = Vec::new();

    let out = run_turn(&mut ctx, &mut messages).await;

    assert!(matches!(out.terminal, AgentEvent::Error { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let usage = kxen_core::core::shared::lock(&usage);
    let session = usage.get(&session_id).expect("zero receipt creates a complete session ledger entry");
    assert_eq!((session.input, session.output, session.unmetered_calls), (0, 0, 0));
    assert!(session.metering_receipts.is_empty());
    assert!(session.pending_goal_charges.is_empty());
    drop(usage);
    assert!(kxen_core::core::usage::ProviderAttemptStore::new(attempts).load_all().unwrap().is_empty());
    let saved = kxen_core::core::goal::Goal::load(&dir, goal_id).expect("load");
    assert_eq!(saved.status, kxen_core::core::goal::GoalStatus::Active);
    assert_eq!((saved.tokens_used, saved.unmetered_calls), (0, 0));
    assert!(saved.metering_receipts.is_empty());
    let _ = std::fs::remove_file(dir.join(format!("{goal_id}.json")));
}

#[tokio::test]
async fn retry_stops_when_failed_attempt_exhausts_goal_budget() {
    let dir = goals_dir_isolation();
    std::fs::create_dir_all(&dir).expect("mkdir");
    let mut goal = kxen_core::core::goal::Goal::create(
        kxen_core::core::goal::GoalContract {
            objective: "o".into(),
            completion_criteria: "c".into(),
            constraints: None,
            budget: kxen_core::core::goal::GoalBudget { tokens: Some(5), ..Default::default() },
        },
        "run-retry-budget-goal".into(),
    )
    .expect("create");
    goal.activate().expect("activate");
    let session_id = create_test_session();
    goal.session_id = Some(session_id.clone());
    goal.save(&dir).expect("save");
    let calls = Arc::new(AtomicUsize::new(0));
    let stream = scripted(
        vec![
            vec![Delta::Usage { input: 10, output: 0 }, Delta::Error("xai HTTP 429: too many requests".into())],
            vec![Delta::Text("must-not-run".into()), Delta::Done],
        ],
        calls.clone(),
    );
    let mut ctx = test_ctx(stream, &session_id);
    let mut messages = Vec::new();

    let out = run_turn(&mut ctx, &mut messages).await;

    assert_eq!(calls.load(Ordering::SeqCst), 1, "budget must be settled before the next retry attempt");
    assert!(out.final_text.contains("预算耗尽"));
    let saved = kxen_core::core::goal::Goal::load(&dir, "run-retry-budget-goal").expect("load");
    assert_eq!(saved.status, kxen_core::core::goal::GoalStatus::BudgetLimited);
    assert_eq!(saved.tokens_used, 10);
    let _ = std::fs::remove_file(dir.join("run-retry-budget-goal.json"));
}

/// abort 在重试退避期立即生效。
/// 判定信号：退避期取消后不得发起第二次 LLM 请求。
#[tokio::test]
async fn abort_during_retry_backoff_interrupts_immediately() {
    goals_dir_isolation();
    let calls = Arc::new(AtomicUsize::new(0));
    let stream = scripted(
        vec![vec![Delta::Error("xai HTTP 429: too many requests".into())], vec![Delta::Text("should-not-reach".into()), Delta::Done]],
        calls.clone(),
    );
    let token = kxen_core::agent::cancel::CancelToken::new();
    let mut ctx = test_ctx(stream, "run-abort-backoff");
    ctx.cancel = Some(token.clone());
    let mut messages = Vec::new();
    let run = tokio::spawn(async move { run_turn(&mut ctx, &mut messages).await });
    // 首次 LLM 请求发出（calls==1）即取消：此刻 run 必在「错误处理 -> 退避」窗口内，
    // 比固定 sleep 稳（固定时延在高负载下可能睡过整个退避窗口造成假失败）。
    while calls.load(Ordering::SeqCst) == 0 && !run.is_finished() {
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    token.cancel();
    let out = tokio::time::timeout(std::time::Duration::from_secs(10), run).await.expect("abort 不得卡在退避期").expect("join");
    assert!(out.aborted, "退避期 abort 必须生效");
    assert!(matches!(out.terminal, AgentEvent::Aborted));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "退避期取消后不得发起第二次 LLM 请求");
}

/// 预算分支：本轮 usage 超 goal tokens 预算 -> BudgetLimited 终态并落盘。
#[tokio::test]
async fn token_budget_limited_terminates_run() {
    let dir = goals_dir_isolation();
    std::fs::create_dir_all(&dir).expect("mkdir");
    let mut goal = kxen_core::core::goal::Goal::create(
        kxen_core::core::goal::GoalContract {
            objective: "o".into(),
            completion_criteria: "c".into(),
            constraints: None,
            budget: kxen_core::core::goal::GoalBudget { tokens: Some(1), ..Default::default() },
        },
        "run-budget-goal".into(),
    )
    .expect("create");
    goal.activate().expect("activate");
    let session_id = create_test_session();
    goal.session_id = Some(session_id.clone());
    goal.save(&dir).expect("save");

    let calls = Arc::new(AtomicUsize::new(0));
    let stream = scripted(vec![vec![Delta::Usage { input: 100, output: 0 }, Delta::Done]], calls);
    let mut ctx = test_ctx(stream, &session_id);
    let mut messages = Vec::new();
    let out = run_turn(&mut ctx, &mut messages).await;
    assert!(out.final_text.contains("预算耗尽"), "终态文本须带预算原因: {}", out.final_text);
    assert!(matches!(out.terminal, AgentEvent::Error { .. }));
    let saved = kxen_core::core::goal::Goal::load(&dir, "run-budget-goal").expect("load");
    assert_eq!(saved.status, kxen_core::core::goal::GoalStatus::BudgetLimited, "预算超限必须落盘 BudgetLimited");
    let _ = std::fs::remove_file(dir.join("run-budget-goal.json"));
}

#[tokio::test]
async fn fatal_stream_error_still_charges_known_goal_usage() {
    let dir = goals_dir_isolation();
    std::fs::create_dir_all(&dir).expect("mkdir");
    let mut goal = kxen_core::core::goal::Goal::create(
        kxen_core::core::goal::GoalContract {
            objective: "o".into(),
            completion_criteria: "c".into(),
            constraints: None,
            budget: kxen_core::core::goal::GoalBudget { tokens: Some(1_000), ..Default::default() },
        },
        "run-fatal-usage-goal".into(),
    )
    .expect("create");
    goal.activate().expect("activate");
    let session_id = create_test_session();
    goal.session_id = Some(session_id.clone());
    goal.save(&dir).expect("save");

    let stream = scripted(
        vec![vec![Delta::Usage { input: 7, output: 3 }, Delta::Error("provider terminal failure".into())]],
        Arc::new(AtomicUsize::new(0)),
    );
    let mut ctx = test_ctx(stream, &session_id);
    let mut messages = Vec::new();
    let out = run_turn(&mut ctx, &mut messages).await;

    assert!(matches!(out.terminal, AgentEvent::Error { .. }));
    let saved = kxen_core::core::goal::Goal::load(&dir, "run-fatal-usage-goal").expect("load");
    assert_eq!(saved.tokens_used, 10, "fatal path must settle usage emitted before the error");
    let _ = std::fs::remove_file(dir.join("run-fatal-usage-goal.json"));
}

#[path = "run_loop/late_budget.rs"]
mod late_budget;
