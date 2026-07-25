// 后台 agent 派发与通知路由（块一：流式归约）集成测试。
// 覆盖：回执不阻塞（拿到回执而非 dispatch 结果）、完成通知进路由通道、无通道上下文显式报错、
// close 前后路由切换（通道 -> late 闭包）、残留合并投出、通知文本截断、多路合并 user 消息。
// 不触网：空凭证下 dispatch 仍 resolve（子 loop 把 LLM 错误吞成返回文本，同 tests/workflow.rs 口径）。

use kxen_app::agent::agent_loop::{AgentContext, dispatch_tool};
use kxen_app::agent::background::{NotifyRouter, notification_text, notifications_message};
use kxen_app::core::config::{Config, Limits, ProviderLimit, RoleBinding};
use kxen_app::llm::ModelRef;
use kxen_app::llm::mrm::ModelResourceManager;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

fn test_ctx(notify: Option<Arc<NotifyRouter>>) -> AgentContext {
    let mut roles = HashMap::new();
    roles.insert("execution".into(), RoleBinding { provider: "xai".into(), model: "grok".into(), fallback: None, account: None });
    let config = Config {
        roles,
        limits: Limits { global_concurrent: 4, providers: HashMap::<String, ProviderLimit>::new() },
        hooks: HashMap::new(),
        statusline: Default::default(),
        voice: Default::default(),
        custom_providers: Default::default(),
        send_when_running: String::new(),
        embedding: Default::default(),
    };
    AgentContext {
        registry: Arc::new(kxen_app::tools::task::TaskRegistry::new()),
        tracker: kxen_app::tools::fs_tool::FileTracker::default(),
        workdir: Arc::from(Path::new("/tmp")),
        model: ModelRef::new("xai", "grok"),
        store: kxen_app::auth::credential::AuthStore::default(),
        max_turns: 1,
        mrm: Some(Arc::new(ModelResourceManager::new(config))),
        allowed_tools: None,
        extras: None,
        hooks: None,
        loop_detector: kxen_app::agent::loop_detect::LoopDetector::new(),
        cancel: None,
        team: None,
        team_identity: None,
        session_id: Some("s-bg".into()),
        agents: Some(Arc::new(kxen_app::agent::activity::AgentRegistry::default())),
        bus: Some(kxen_app::core::event::EventBus::default()),
        approvals: None,
        mcp: None,
        lsp: None,
        notify,
        on_event: Arc::new(|_| {}),
    }
}

#[tokio::test]
async fn background_receipt_returns_before_dispatch_finishes() {
    let router = Arc::new(NotifyRouter::new());
    let ctx = test_ctx(Some(router.clone()));
    // 回执先行：返回的是 backgrounded 回执而非 dispatch 结果
    //（同步路径在空凭证下会返回 LLM 错误文本——拿到回执本身即证明未等 dispatch 完成）
    let receipt = dispatch_tool("agent", &json!({ "role": "execution", "prompt": "noop", "background": true }), "/tmp", &ctx)
        .await
        .expect("background dispatch should be accepted");
    assert!(receipt.contains("backgrounded"), "应为回执而非结果: {receipt}");
    assert!(!receipt.contains("错误"), "回执里不得混进 dispatch 结果: {receipt}");
    // 完成通知随后送达（空凭证：子 loop LLM 错误吞成返回文本，dispatch 很快结束）
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let notes = loop {
        let drained = router.drain();
        if !drained.is_empty() {
            break drained;
        }
        assert!(std::time::Instant::now() < deadline, "后台完成通知 10s 内未送达");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    assert!(notes[0].starts_with("[task notification] agent"), "got: {}", notes[0]);
    assert!(notes[0].contains("(execution)"), "通知需带 role 标注: {}", notes[0]);
}

#[tokio::test]
async fn background_without_notify_channel_errors() {
    // 无通道上下文（subagent/teammate 不嵌套派发）：background=true 显式报错而非静默吞掉
    let ctx = test_ctx(None);
    let err =
        dispatch_tool("agent", &json!({ "role": "execution", "prompt": "noop", "background": true }), "/tmp", &ctx).await.unwrap_err();
    assert!(err.contains("notify channel"), "got: {err}");
}

#[test]
fn router_drains_before_close_and_redirects_after() {
    let router = NotifyRouter::new();
    router.notify("a".into());
    assert_eq!(router.drain(), vec!["a".to_string()]);
    assert!(router.drain().is_empty(), "drain 后通道应为空");
    let collected = Arc::new(std::sync::Mutex::new(Vec::new()));
    let c = collected.clone();
    router.close(Arc::new(move |text: String| kxen_app::core::shared::lock(&c).push(text)));
    router.notify("b".into());
    assert_eq!(kxen_app::core::shared::lock(&collected).as_slice(), &["b".to_string()], "close 后通知必须直投 late 闭包");
}

#[test]
fn close_flushes_leftover_merged_into_late() {
    // run 收尾时通道残留合并为一条投出（分节标注）：逐条入队会连拉 N 个续跑 run
    let router = NotifyRouter::new();
    router.notify("x".into());
    router.notify("y".into());
    let collected = Arc::new(std::sync::Mutex::new(Vec::new()));
    let c = collected.clone();
    router.close(Arc::new(move |text: String| kxen_app::core::shared::lock(&c).push(text)));
    let got = kxen_app::core::shared::lock(&collected).clone();
    assert_eq!(got.len(), 1, "残留应合并为一条: {got:?}");
    assert!(got[0].contains('x') && got[0].contains('y') && got[0].contains("---"), "got: {got:?}");
}

#[test]
fn notification_text_truncates_long_results() {
    let long = "x".repeat(5000);
    let text = notification_text("kxen-review-2", "review", &long);
    assert!(text.starts_with("[task notification] agent kxen-review-2 (review) finished:\n"), "{text}");
    assert!(text.contains("truncated"), "超 4000 字符需截断标记");
    assert!(text.len() < 4200, "截断后总长有界: {}", text.len());
}

#[test]
fn notifications_message_merges_paths_into_one_user_message() {
    assert!(notifications_message(vec![]).is_none());
    let msg = notifications_message(vec!["n1".into(), "n2".into()]).expect("some");
    assert!(matches!(msg.role, kxen_app::llm::types::Role::User));
    assert!(
        msg.content.contains("n1") && msg.content.contains("n2") && msg.content.contains("---"),
        "多路合一条需分节标注: {}",
        msg.content
    );
}
