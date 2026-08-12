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
        embedding: Default::default(),
        composer_suggestions: Default::default(),
        search: Default::default(),
        coding_rules: Default::default(),
        experimental: Default::default(),
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
    }
}

fn session_ctx(session_id: &str) -> AgentContext {
    AgentContext {
        registry: Arc::new(crate::tools::task::TaskRegistry::new()),
        tracker: crate::tools::fs_tool::FileTracker::default(),
        workdir: Arc::from(std::path::Path::new("/tmp")),
        path_grants: Arc::new(Default::default()),
        model: crate::llm::ModelRef::new("p", "m"),
        store: crate::auth::credential::AuthStore::default().into(),
        max_turns: 4,
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
        lsp: None,
        notify: None,
        persist_compaction: None,
        persist_turn: None,
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
