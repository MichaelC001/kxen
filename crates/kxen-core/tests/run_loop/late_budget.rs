use super::*;

#[tokio::test]
async fn run_without_goal_never_rebinds_to_goal_created_mid_run() {
    let dir = goals_dir_isolation();
    std::fs::create_dir_all(&dir).expect("mkdir");
    let goal_path = dir.join("run-late-goal.json");
    let _ = std::fs::remove_file(&goal_path);
    let created_dir = dir.clone();
    let stream: StreamFn = Arc::new(move |_model, _messages, _tools, _store| {
        let mut goal = kxen_core::core::goal::Goal::create(
            kxen_core::core::goal::GoalContract {
                objective: "o".into(),
                completion_criteria: "c".into(),
                constraints: None,
                budget: kxen_core::core::goal::GoalBudget::default(),
            },
            "run-late-goal".into(),
        )
        .expect("create");
        goal.activate().expect("activate");
        goal.session_id = Some("run-no-goal-at-start".into());
        goal.save(&created_dir).expect("save");
        Box::pin(futures::stream::iter(vec![Delta::Usage { input: 8, output: 2 }, Delta::Done]))
    });
    let mut ctx = test_ctx(stream, "run-no-goal-at-start");
    let mut messages = Vec::new();

    let out = run_turn(&mut ctx, &mut messages).await;

    assert!(matches!(out.terminal, AgentEvent::Done { .. }));
    assert!(ctx.goal_binding_frozen);
    assert!(ctx.bound_goal_id.is_none());
    let saved = kxen_core::core::goal::Goal::load(&dir, "run-late-goal").expect("load");
    assert_eq!(saved.tokens_used, 0, "本 run 开始后创建的 Goal 不得承接此前 Provider 用量");
    assert_eq!(saved.turns_used, 0);
    let _ = std::fs::remove_file(goal_path);
}

#[tokio::test]
async fn bound_goal_load_failure_stops_before_provider_dispatch() {
    let dir = goals_dir_isolation();
    std::fs::create_dir_all(&dir).expect("mkdir");
    let goal_id = "run-missing-goal";
    let goal_path = dir.join(format!("{goal_id}.json"));
    let _ = std::fs::remove_file(&goal_path);

    let calls = Arc::new(AtomicUsize::new(0));
    let stream = scripted(vec![vec![Delta::Text("must-not-run".into()), Delta::Done]], calls.clone());
    let mut ctx = test_ctx(stream, "run-missing-goal-session");
    ctx.bound_goal_id = Some(goal_id.into());
    ctx.goal_binding_frozen = true;
    let mut messages = Vec::new();

    let out = run_turn(&mut ctx, &mut messages).await;

    assert_eq!(calls.load(Ordering::SeqCst), 0, "不可读取的已绑定 Goal 必须在 Provider 前 fail closed");
    assert!(matches!(out.terminal, AgentEvent::Error { .. }));
    assert!(out.final_text.contains("goal usage save failed"), "terminal reason: {}", out.final_text);
    let _ = std::fs::remove_file(goal_path);
}

/// 部分产出保留：流中途不可重试错误，已流出文本进终态文本与历史（附错误标记），不得整段丢弃。
#[tokio::test]
async fn stream_error_keeps_partial_output() {
    goals_dir_isolation();
    let calls = Arc::new(AtomicUsize::new(0));
    let stream = scripted(vec![vec![Delta::Text("partial answer".into()), Delta::Error("stream reset by peer".into())]], calls.clone());
    let mut ctx = test_ctx(stream, "run-partial");
    let mut messages = Vec::new();
    let out = run_turn(&mut ctx, &mut messages).await;
    assert!(out.final_text.starts_with("partial answer"), "部分产出必须进终态文本: {}", out.final_text);
    assert!(out.final_text.contains("(错误: stream reset by peer)"), "错误标记必须附后: {}", out.final_text);
    assert!(matches!(out.terminal, AgentEvent::Error { .. }));
    assert!(
        messages.iter().any(|m| m.role == kxen_core::llm::types::Role::Assistant && m.content == "partial answer"),
        "部分产出必须进历史"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1, "部分产出后不得重试");
}

/// 暂停分支：run 在飞期间 goal 被暂停（流闭包内落盘模拟 RPC/工具暂停），
/// 轮末记账发现非 Active 必须落终态停出，不得继续下一轮 LLM 请求。
#[tokio::test]
async fn paused_goal_terminates_in_flight_run() {
    let dir = goals_dir_isolation();
    std::fs::create_dir_all(&dir).expect("mkdir");
    let mut goal = kxen_core::core::goal::Goal::create(
        kxen_core::core::goal::GoalContract {
            objective: "o".into(),
            completion_criteria: "c".into(),
            constraints: None,
            budget: kxen_core::core::goal::GoalBudget::default(),
        },
        "run-pause-goal".into(),
    )
    .expect("create");
    goal.activate().expect("activate");
    let session_id = create_test_session();
    goal.session_id = Some(session_id.clone());
    goal.save(&dir).expect("save");

    let calls = Arc::new(AtomicUsize::new(0));
    let pause_dir = dir.clone();
    let call_count = calls.clone();
    let stream: StreamFn = Arc::new(move |_model, _messages, _tools, _store| {
        call_count.fetch_add(1, Ordering::SeqCst);
        // 模拟 run 在飞期间用户暂停 goal（RPC/工具暂停的同一落盘形态）
        let mut g = kxen_core::core::goal::Goal::load(&pause_dir, "run-pause-goal").expect("load");
        g.pause().expect("pause");
        g.save(&pause_dir).expect("save");
        Box::pin(futures::stream::iter(vec![Delta::Text("partial".into()), Delta::Usage { input: 10, output: 5 }, Delta::Done]))
    });
    let mut ctx = test_ctx(stream, &session_id);
    let mut messages = Vec::new();
    let out = run_turn(&mut ctx, &mut messages).await;
    assert!(out.final_text.contains("已暂停"), "终态文本须带暂停原因: {}", out.final_text);
    assert!(matches!(out.terminal, AgentEvent::Error { .. }), "暂停终态必须是 Error: {:?}", out.terminal);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "暂停后不得发起下一轮 LLM 请求");
    let _ = std::fs::remove_file(dir.join("run-pause-goal.json"));
}

/// 并发槽排队期间 wall 预算到期：释放槽后也不得启动真实 Provider stream。
#[tokio::test]
async fn wall_budget_is_rechecked_after_mrm_queue() {
    let dir = goals_dir_isolation();
    std::fs::create_dir_all(&dir).expect("mkdir");
    let mut goal = kxen_core::core::goal::Goal::create(
        kxen_core::core::goal::GoalContract {
            objective: "o".into(),
            completion_criteria: "c".into(),
            constraints: None,
            budget: kxen_core::core::goal::GoalBudget { wall_clock_ms: Some(30), ..Default::default() },
        },
        "run-queued-wall-goal".into(),
    )
    .expect("create");
    goal.activate().expect("activate");
    let session_id = create_test_session();
    goal.session_id = Some(session_id.clone());
    goal.save(&dir).expect("save");

    let mut config = kxen_core::core::config::Config::default();
    config.limits.global_concurrent = 1;
    config.limits.providers.insert("p".into(), kxen_core::core::config::ProviderLimit { concurrent: Some(1), ..Default::default() });
    let mrm = Arc::new(kxen_core::llm::mrm::ModelResourceManager::new(config));
    let held = mrm.acquire_slot("p").await;
    let calls = Arc::new(AtomicUsize::new(0));
    let stream = scripted(vec![vec![Delta::Text("must-not-run".into()), Delta::Done]], calls.clone());
    let mut ctx = test_ctx(stream, &session_id);
    ctx.mrm = Some(mrm);
    let mut messages = Vec::new();
    let run = tokio::spawn(async move { run_turn(&mut ctx, &mut messages).await });

    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    drop(held);
    let out = tokio::time::timeout(std::time::Duration::from_secs(1), run).await.expect("queued run should finish").expect("join");

    assert_eq!(calls.load(Ordering::SeqCst), 0, "expired goal must not start a Provider request after queueing");
    assert!(out.final_text.contains("预算耗尽"), "terminal reason: {}", out.final_text);
    assert!(matches!(out.terminal, AgentEvent::Error { .. }));
    let _ = std::fs::remove_file(dir.join("run-queued-wall-goal.json"));
}

/// Provider 已开始但永不产出 delta：wall deadline 必须主动唤醒，不能依赖下一帧到达。
#[tokio::test]
async fn wall_budget_interrupts_a_silent_provider_stream() {
    let dir = goals_dir_isolation();
    std::fs::create_dir_all(&dir).expect("mkdir");
    let mut goal = kxen_core::core::goal::Goal::create(
        kxen_core::core::goal::GoalContract {
            objective: "o".into(),
            completion_criteria: "c".into(),
            constraints: None,
            budget: kxen_core::core::goal::GoalBudget { wall_clock_ms: Some(500), ..Default::default() },
        },
        "run-silent-wall-goal".into(),
    )
    .expect("create");
    goal.activate().expect("activate");
    let session_id = create_test_session();
    goal.session_id = Some(session_id.clone());
    goal.save(&dir).expect("save");

    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let stream: StreamFn = Arc::new(move |_model, _messages, _tools, _store| {
        seen.fetch_add(1, Ordering::SeqCst);
        Box::pin(futures::stream::pending())
    });
    let mut config = kxen_core::core::config::Config::default();
    config.limits.providers.insert(
        "p".into(),
        kxen_core::core::config::ProviderLimit {
            circuit_failure_threshold: Some(1),
            circuit_cooldown_seconds: Some(0),
            ..Default::default()
        },
    );
    let mrm = Arc::new(kxen_core::llm::mrm::ModelResourceManager::new(config));
    mrm.record_result("p", false).await;
    let mut ctx = test_ctx(stream, &session_id);
    ctx.mrm = Some(mrm.clone());
    let mut messages = Vec::new();
    let out = tokio::time::timeout(std::time::Duration::from_secs(2), run_turn(&mut ctx, &mut messages))
        .await
        .expect("silent stream must be interrupted by wall deadline");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(out.final_text.contains("预算耗尽"), "terminal reason: {}", out.final_text);
    assert!(matches!(out.terminal, AgentEvent::Error { .. }));
    let report = mrm.health().await.into_iter().find(|report| report.provider == "p").expect("health report");
    assert_eq!(report.consecutive_failures, 1, "local goal timeout must not close a half-open Provider circuit as success");
    let _ = std::fs::remove_file(dir.join("run-silent-wall-goal.json"));
}

/// run 内 compaction 的摘要必须先写 checkpoint；持久化失败时禁止继续主请求。
#[tokio::test]
async fn compaction_checkpoint_failure_stops_before_main_request() {
    goals_dir_isolation();
    let calls = Arc::new(AtomicUsize::new(0));
    let stream = scripted(vec![vec![Delta::Text("durable summary".into()), Delta::Done]], calls.clone());
    let mut ctx = test_ctx(stream, "run-compact-persist");
    ctx.mrm = Some(Arc::new(kxen_core::llm::mrm::ModelResourceManager::new(Default::default())));
    ctx.usage_reporter = Some(kxen_core::agent::agent_loop::UsageReporter::new_in(
        "run-compact-persist".into(),
        Arc::new(Mutex::new(Default::default())),
        kxen_core::core::event::EventBus::default(),
        goals_dir_isolation().join("usage-attempts"),
    ));
    ctx.persist_compaction = Some(Arc::new(|_summary, _covered| Err("checkpoint unavailable".into())));
    let mut messages =
        (0..9).map(|index| kxen_core::llm::Message::user(format!("message-{index}-{}", "x".repeat(80_000)))).collect::<Vec<_>>();

    let out = run_turn(&mut ctx, &mut messages).await;

    assert_eq!(calls.load(Ordering::SeqCst), 0, "checkpoint failure must stop before the main injected stream starts");
    assert_eq!(messages.len(), 10, "failed checkpoint may add only the run-owned system prompt");
    assert_eq!(messages.last().map(|message| message.content.len()), Some(80_010), "all user history must remain intact");
    assert!(out.final_text.contains("checkpoint unavailable"), "terminal reason: {}", out.final_text);
    assert!(matches!(out.terminal, AgentEvent::Error { .. }));
}
