use super::*;
use crate::kanban::driver::tests as driver_tests;
use crate::kanban::driver::{LandingKind, execute, turns_path};
use crate::kanban::events::{EventKind, KanbanCommand, Outcome};
use crate::kanban::model::PolicySpec;
use crate::kanban::{agents, store};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-context-{tag}-{}-{nanos}", std::process::id()))
}

/// exec_scope 端到端：readonly+test profile 的 kanban run 无持久 session 也能 exec，
/// 命中看板 policy allowlist 的命令经 BoardAutoApprove 自动放行且审计 durable。
#[tokio::test]
async fn exec_auto_approved_end_to_end_without_session() {
    let workspace = temp("execauto");
    let mut definition = driver_tests::agent_def();
    definition.permission_profile = "readonly+test".into();
    agents::save(&workspace, &definition).unwrap();
    let mut board = driver_tests::agent_board(&workspace, None);
    board
        .apply(KanbanCommand::PolicySet { policy: PolicySpec { allowlist: vec!["echo".into()], expires_at_ms: None, max_uses: None } })
        .unwrap();
    let card_id = driver_tests::create_card(&mut board, "implementing");
    drop(board);
    let calls = std::sync::Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let stream: crate::llm::StreamFn = std::sync::Arc::new(move |_, _, _, _| {
        if seen.fetch_add(1, Ordering::SeqCst) == 0 {
            Box::pin(futures::stream::iter(vec![
                crate::llm::Delta::ToolFragments(vec![crate::llm::tool::ChunkToolCall {
                    index: Some(0),
                    id: Some("call_1".into()),
                    function: Some(crate::llm::tool::ChunkFunction {
                        name: Some("exec".into()),
                        arguments: Some(r#"{"command":"echo kanban-exec-ok"}"#.into()),
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
    // 审批通道（broker+bus）齐备才抵达 auto 短路；生产 runner 恒挂载 approvals（runner.rs tick）
    let mut deps = driver_tests::deps(&workspace, stream);
    deps.approvals = Some(std::sync::Arc::new(crate::agent::approval::ApprovalBroker::new()));
    let landing = execute(&workspace, "board_t", &card_id, &deps, None).await.unwrap();
    assert_eq!(landing.kind, LandingKind::Finished(Outcome::Success));
    // exec 真实执行：迭代持久化里的 tool output 带 exit 0 与命令回显
    let turns = crate::core::session::load_lines(&turns_path(&workspace, "board_t", &landing.run_id).unwrap()).unwrap();
    let serialized = serde_json::to_string(&turns).unwrap();
    assert!(serialized.contains("exit 0"), "exec 必须真实执行成功: {serialized}");
    assert!(serialized.contains("kanban-exec-ok"), "exec 输出必须含命令回显: {serialized}");
    // 自主授权审计落事件流（auto_approved 事件）
    let events_file = store::events_path(&store::board_dir(&workspace, "board_t").unwrap());
    let events = store::load_events(&events_file).unwrap();
    assert!(
        events.iter().any(|e| matches!(&e.kind, EventKind::AutoApproved(p) if p.command == "echo kanban-exec-ok")),
        "命中 allowlist 的 exec 必须落 auto_approved 审计事件"
    );
    std::fs::remove_dir_all(workspace).ok();
}

/// fail-closed 不回归：session_id 与 exec_scope 皆 None 时 exec 仍拒绝。
#[tokio::test]
async fn exec_without_session_or_scope_still_denied() {
    let workspace = temp("execnone");
    let deps = driver_tests::deps(&workspace, driver_tests::text_stream("x"));
    let ctx = base_context(&deps, ModelRef::new("p", "m"), None, None, CancelToken::new(), None);
    let error = crate::agent::agent_loop::execute_tool("exec", r#"{"command":"echo hi"}"#, &ctx).await.unwrap_err();
    assert!(error.contains("exec requires a session context"), "{error}");
    std::fs::remove_dir_all(workspace).ok();
}
