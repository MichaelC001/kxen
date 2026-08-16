use super::*;
use crate::agent::agent_loop::AgentContext;
use crate::core::config::{Config, Limits, ProviderLimit, RoleBinding};
use crate::llm::Delta;
use crate::llm::mrm::ModelResourceManager;
use std::collections::HashMap;

/// 级联回归：作用域结束（超时/提前返回同一 Drop 路径）必须同时置 JS 中断标志
/// 并取消 workflow 令牌——在飞子代理经 dispatch 的 _cascade watcher 收到取消。
#[test]
fn cancel_guard_cascades_to_workflow_token() {
    let flag = Arc::new(AtomicBool::new(false));
    let token = crate::agent::cancel::CancelToken::new();
    {
        let _guard = CancelGuard(flag.clone(), token.clone());
    }
    assert!(flag.load(Ordering::Relaxed), "JS 中断标志必须置位");
    assert!(token.is_cancelled(), "workflow 令牌必须取消（子代理级联取消的源头）");
}

/// 级联回归：父 run abort 经 cascade_parent 传到 workflow 令牌；
/// done_tx 回收后 watcher 退出不再误触。
#[tokio::test]
async fn parent_abort_cascades_into_workflow_token() {
    let parent = crate::agent::cancel::CancelToken::new();
    let child = crate::agent::cancel::CancelToken::new();
    let done = cascade_parent(Some(parent.clone()), &child);
    parent.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(1), child.wait()).await.expect("父取消必须级联到 workflow 令牌");
    drop(done);

    // 无父令牌（subagent 嵌套外路径）：不建 watcher
    assert!(cascade_parent(None, &child).is_none());
}

/// 计数 fake 流：不触网，dispatch 次数即「是否真实派发」的观测口（缓存命中不增计数）。
fn counting_deps(count: Arc<AtomicU32>) -> SubagentDeps {
    let mut roles = HashMap::new();
    roles.insert("execution".into(), RoleBinding { provider: "xai".into(), model: "grok".into(), fallback: None, account: None });
    let config = Config {
        roles,
        limits: Limits { global_concurrent: 4, daily_token_budget: None, providers: HashMap::<String, ProviderLimit>::new() },
        hooks: HashMap::new(),
        statusline: Default::default(),
        voice: Default::default(),
        custom_providers: Default::default(),
        send_when_running: String::new(),
        approval_timeout_seconds: None,
        checkpoint_keep: None,
        embedding: Default::default(),
        composer_suggestions: Default::default(),
        search: Default::default(),
        coding_rules: Default::default(),
        experimental: Default::default(),
        sandbox: Default::default(),
        web: Default::default(),
        tray: Default::default(),
    };
    SubagentDeps {
        registry: Arc::new(crate::tools::task::TaskRegistry::new()),
        workdir: Arc::from(std::path::Path::new("/tmp")),
        path_grants: Arc::new(Default::default()),
        store: crate::auth::credential::AuthStore::default().into(),
        mrm: Arc::new(ModelResourceManager::new(config)),
        hooks: None,
        extras: None,
        cancel: None,
        agents: Arc::new(crate::agent::activity::AgentRegistry::default()),
        session_id: None,
        exec_scope: None,
        bus: crate::core::event::EventBus::default(),
        approvals: None,
        mcp: None,
        lsp: None,
        stream_override: Some(Arc::new(move |_, _, _, _| {
            count.fetch_add(1, Ordering::Relaxed);
            Box::pin(futures::stream::iter(vec![Delta::Text("ok".into()), Delta::Done]))
        })),
        usage_reporter: None,
        code_orchestration: true,
    }
}

fn session_ctx(session_id: &str) -> AgentContext {
    AgentContext {
        registry: Arc::new(crate::tools::task::TaskRegistry::new()),
        tracker: crate::tools::fs_tool::FileTracker::default(),
        workdir: Arc::from(std::path::Path::new("/tmp")),
        child_env: None,
        path_grants: Arc::new(Default::default()),
        model: crate::llm::ModelRef::new("p", "m"),
        path_scope: None,
        store: crate::auth::credential::AuthStore::default().into(),
        max_turns: 4,
        max_pure_retries: None,
        mrm: None,
        allowed_tools: None,
        extras: None,
        hooks: None,
        loop_detector: crate::agent::loop_detect::LoopDetector::new(),
        cancel: None,
        team: None,
        team_identity: None,
        session_id: Some(session_id.to_string()),
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
        code_orchestration: true,
        auxiliary_usage: Arc::default(),
        usage_reporter: None,
        on_event: Arc::new(|_| {}),
        stream_override: None,
    }
}

/// 结果文本里 `[workflow run_id: <id> - ...]` 的 id 段（resume 提示的唯一回传通道）。
fn extract_run_id(out: &str) -> &str {
    out.split("workflow run_id: ").nth(1).and_then(|rest| rest.split(' ').next()).expect("结果文本必须回传 run_id")
}

/// run_id 缺省：自动生成 + journal 落 scoped 目录 + 结果文本回传 id；
/// 凭返回 id 带同 script 重跑：已完成派发命中缓存，不重复派发。
#[tokio::test]
async fn missing_run_id_auto_generates_journal_and_resumes_from_cache() {
    let dispatch_count = Arc::new(AtomicU32::new(0));
    let script = "const a = await agent('execution', 'do A'); return a;";
    let ctx = session_ctx("sess-wf-auto");
    let out = run_tool(script, counting_deps(dispatch_count.clone()), &ctx, None).await.expect("workflow should succeed");
    assert!(out.contains("workflow run_id: wf_"), "{out}");
    let run_id = extract_run_id(&out).to_string();
    assert_eq!(dispatch_count.load(Ordering::Relaxed), 1, "首跑真实派发一次");

    // journal 落在 session 派生命名空间；file lock 随 workflow 线程退出释放，轮询等待而非固定 sleep
    let file = crate::agent::workflow_journal::scoped_journal_file(Some("sess-wf-auto"), &run_id);
    let mut journal = None;
    for _ in 0..40 {
        if let Ok(opened) = crate::agent::workflow_journal::Journal::open_scoped(Some("sess-wf-auto"), &run_id, script) {
            journal = Some(opened);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let journal = journal.expect("journal lock must be released after run");
    assert!(file.exists(), "journal 文件必须生成在 scoped 目录");
    assert_eq!(journal.completed(), 1, "已完成派发必须落 journal");
    drop(journal);

    // 凭返回的 id 带同 script 重跑：命中缓存不重复派发；显式 id 不重复标注
    let out2 = run_tool(script, counting_deps(dispatch_count.clone()), &ctx, Some(&run_id)).await.expect("resume should succeed");
    assert_eq!(dispatch_count.load(Ordering::Relaxed), 1, "重跑必须命中 journal 缓存，不重复派发");
    assert!(!out2.contains("workflow run_id:"), "显式 id 不重复标注: {out2}");

    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_file(file.with_extension("jsonl.lock"));
}

// ---------------- tool() 通用工具桥（code mode） ----------------

/// 带桥跑一脚本：journal=None（不碰 data_dir），工作目录为测试临时目录。
async fn run_with_bridge(script: &str, workdir: &std::path::Path) -> Result<String, String> {
    let ctx = {
        let mut ctx = session_ctx("sess-wf-tools");
        ctx.workdir = Arc::from(workdir);
        ctx
    };
    let bridge = super::ToolBridge::new(&ctx, crate::agent::cancel::CancelToken::new());
    let (tx, _rx) = mpsc::unbounded_channel();
    run_script(script, counting_deps(Arc::new(AtomicU32::new(0))), tx, Arc::new(AtomicBool::new(false)), None, Some(bridge)).await
}

fn temp_workdir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kxen-wf-tools-{tag}-{}-{}", std::process::id(), crate::core::ids::new_id("d")));
    std::fs::create_dir_all(&dir).expect("create temp workdir");
    dir
}

/// 一次 workflow 调用内完成 read + grep 多步编排；子调用结构化块随结果产出（前端投影数据源）。
#[tokio::test]
async fn tool_bridge_runs_multiple_calls_in_one_workflow() {
    let dir = temp_workdir("multi");
    std::fs::write(dir.join("a.txt"), "needle one\nhay\n").unwrap();
    std::fs::write(dir.join("b.txt"), "needle two\n").unwrap();
    let script = "const c = await tool('read', { path: 'a.txt' });\n\
                  const g = await tool('grep', { pattern: 'needle' });\n\
                  return String(c.includes('needle one')) + '|' + String(g.includes('b.txt'))";
    let out = run_with_bridge(script, &dir).await.expect("workflow with tool bridge should succeed");
    assert!(out.starts_with("true|true"), "{out}");
    assert!(out.contains("[kxen:tool-calls]"), "{out}");
    assert!(out.contains(r#"{"name":"read","status":"ok","ms":"#), "{out}");
    assert!(out.contains(r#"{"name":"grep","status":"ok","ms":"#), "{out}");
    assert!(out.contains("0 agents"), "{out}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 递归编排拒绝：桥内 tool("workflow") 报错，不开第二层沙箱。
#[tokio::test]
async fn tool_bridge_rejects_recursive_workflow() {
    let dir = temp_workdir("recursion");
    let script = "try { await tool('workflow', { script: 'return 1' }); return 'not-rejected'; } catch (e) { return String(e); }";
    let out = run_with_bridge(script, &dir).await.expect("script catches bridge error");
    assert!(out.contains("recursive"), "{out}");
    assert!(!out.contains("[kxen:tool-calls]"), "递归拒绝不留子调用记录: {out}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 调用预算封顶（64）：第 65 次调用报 budget exhausted，脚本可捕获。
#[tokio::test]
async fn tool_bridge_budget_caps_at_64_calls() {
    let dir = temp_workdir("budget");
    std::fs::write(dir.join("x.txt"), "x\n").unwrap();
    let script = "let msg = '';\n\
                  for (let i = 0; i < 65; i++) { try { await tool('read', { path: 'x.txt' }); } catch (e) { msg = String(e); break; } }\n\
                  return msg";
    let out = run_with_bridge(script, &dir).await.expect("script catches budget error");
    assert!(out.contains("tool call budget exhausted (64)"), "{out}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 桥关闭（code_orchestration=false）：tool() 是显式拒绝 stub，不是 ReferenceError。
#[tokio::test]
async fn tool_bridge_disabled_stub_rejects_clearly() {
    let dir = temp_workdir("disabled");
    let script = "try { await tool('read', { path: 'x' }); return 'not-rejected'; } catch (e) { return String(e); }";
    let (tx, _rx) = mpsc::unbounded_channel();
    let out = run_script(script, counting_deps(Arc::new(AtomicU32::new(0))), tx, Arc::new(AtomicBool::new(false)), None, None)
        .await
        .expect("script catches stub rejection");
    assert!(out.contains("code orchestration not allowed"), "{out}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// journal 逐次 intent/record：同 run_id 重跑逐条 replay——write 不重复执行（文件不复活），
/// read 回缓存；子调用块标 cached 且不虚构耗时。
#[tokio::test]
async fn tool_calls_journal_replays_per_dispatch() {
    let dir = temp_workdir("replay");
    let session = "sess-wf-tools-replay";
    let run_id = format!("wftoolsreplay{}", std::process::id());
    let script = "await tool('write', { path: 'made.txt', content: 'hello' });\n\
                  const c = await tool('read', { path: 'made.txt' });\n\
                  return String(c.includes('hello'))";
    let ctx = {
        let mut ctx = session_ctx(session);
        ctx.workdir = Arc::from(dir.as_path());
        ctx
    };
    let out = run_tool(script, counting_deps(Arc::new(AtomicU32::new(0))), &ctx, Some(&run_id)).await.expect("first run should succeed");
    assert!(out.starts_with("true"), "{out}");
    assert!(dir.join("made.txt").exists(), "首跑真实执行 write");

    // file lock 随 workflow 线程退出释放：轮询等待（同 missing_run_id 测试先例）
    let file = crate::agent::workflow_journal::scoped_journal_file(Some(session), &run_id);
    let mut released = false;
    for _ in 0..40 {
        if let Ok(journal) = crate::agent::workflow_journal::Journal::open_scoped(Some(session), &run_id, script) {
            assert_eq!(journal.completed(), 2, "write + read 两次调用都必须落 journal");
            released = true;
            drop(journal);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(released, "journal lock must be released after run");

    // 删除产物后同 run_id 重跑：write 命中缓存不重复执行，文件不得复活
    std::fs::remove_file(dir.join("made.txt")).unwrap();
    let out2 = run_tool(script, counting_deps(Arc::new(AtomicU32::new(0))), &ctx, Some(&run_id)).await.expect("resume should succeed");
    assert!(out2.starts_with("true"), "{out2}");
    assert!(!dir.join("made.txt").exists(), "缓存回放不得重复执行 write");
    assert!(out2.contains(r#""cached":true"#), "{out2}");
    assert!(!out2.contains(r#"{"name":"write","status":"ok","ms""#), "回放不虚构耗时: {out2}");

    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_file(file.with_extension("jsonl.lock"));
    std::fs::remove_dir_all(&dir).unwrap();
}
