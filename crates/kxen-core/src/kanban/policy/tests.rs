use super::*;
use crate::kanban::model::PolicySpec;
use crate::kanban::{EventKind, KanbanError, Outcome, board_dir, events_path, load_events};
use crate::tools::auto_approve::AutoApprove;
use crate::tools::exec::{ApprovalCtx, ExecError, safety_gate};
use std::path::Path;
use std::time::Duration;

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-auto-{tag}-{}-{nanos}", std::process::id()))
}

fn spec(allowlist: &[&str], expires_at_ms: Option<u64>, max_uses: Option<u32>) -> PolicySpec {
    PolicySpec { allowlist: allowlist.iter().map(|prefix| prefix.to_string()).collect(), expires_at_ms, max_uses }
}

/// 默认模板建板 + implementing 列建卡 + claim，返回 open run_id。
fn board_with_open_run(workspace: &Path) -> (Board, String) {
    let mut board = Board::open(workspace, "board_t").unwrap();
    board.apply(KanbanCommand::BoardCreate { title: "授权看板".into(), columns: None }).unwrap();
    let event = board
        .apply(KanbanCommand::CardCreate { column_id: Some("implementing".into()), title: "任务".into(), body: String::new() })
        .unwrap();
    let EventKind::CardCreate(card) = event.kind else { panic!("expected card_create") };
    let run = board.apply(KanbanCommand::RunStarted { card_id: card.card_id }).unwrap();
    let EventKind::RunStarted(started) = run.kind else { panic!("expected run_started") };
    (board, started.run_id)
}

fn handle(workspace: &Path, run_id: &str, bus: crate::core::event::EventBus) -> BoardAutoApprove {
    BoardAutoApprove { workspace: workspace.to_path_buf(), board_id: "board_t".into(), run_id: run_id.to_string(), bus }
}

fn event_log(workspace: &Path) -> Vec<crate::kanban::KanbanEvent> {
    load_events(&events_path(&board_dir(workspace, "board_t").unwrap())).unwrap()
}

#[test]
fn try_auto_allow_denies_without_policy_and_appends_nothing() {
    let workspace = temp("nopolicy");
    let (board, run_id) = board_with_open_run(&workspace);
    drop(board);
    let bus = crate::core::event::EventBus::new(8);
    let error = handle(&workspace, &run_id, bus).try_auto_allow("echo hi").unwrap_err();
    assert!(error.contains("policy denied: no active policy"), "{error}");
    assert!(!event_log(&workspace).iter().any(|event| matches!(event.kind, EventKind::AutoApproved(_))), "守卫拒绝不得落审计事件");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn safety_gate_auto_hit_skips_manual_approval_with_durable_audit() {
    let workspace = temp("hit");
    let (mut board, run_id) = board_with_open_run(&workspace);
    board.apply(KanbanCommand::PolicySet { policy: spec(&["echo"], None, Some(3)) }).unwrap();
    drop(board);
    let bus = crate::core::event::EventBus::new(8);
    let mut events = bus.subscribe();
    let auto = handle(&workspace, &run_id, bus.clone());
    // broker 无人应答：命中放行必须完全不触碰人工审批（超时档 broker 只是结构占位）
    let broker = crate::agent::approval::ApprovalBroker::with_timeout(Duration::from_millis(50));
    let appr = ApprovalCtx { broker: &broker, bus: &bus, cancel: None, session_id: "", auto: Some(&auto) };
    safety_gate("echo auto-allowed", workspace.to_str().unwrap(), Some(&appr)).await.expect("allowlist 命中自动放行");
    // 审计 durable：事件流有 auto_approved，计数递增
    let board = Board::open(&workspace, "board_t").unwrap();
    assert_eq!(board.state().policy.as_ref().unwrap().used, 1);
    assert!(
        event_log(&workspace)
            .iter()
            .any(|event| matches!(&event.kind, EventKind::AutoApproved(p) if p.run_id == run_id && p.command == "echo auto-allowed")),
        "放行必须先落 auto_approved 事件"
    );
    // bus 收到板粒度失效通知；全程无人工审批请求，命令原文不进全局流
    let mut saw_update = false;
    loop {
        match events.try_recv() {
            Ok(crate::core::event::Event::KanbanUpdate { board_id, workspace: ws }) => {
                assert_eq!(board_id, "board_t");
                assert_eq!(ws, workspace.to_string_lossy());
                saw_update = true;
            }
            Ok(crate::core::event::Event::LlmDelta(payload)) => {
                assert_ne!(payload["kind"], "approval", "命中放行不得发起人工审批");
                assert!(!payload.to_string().contains("echo auto-allowed"), "命令原文不得进全局流");
            }
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    assert!(saw_update, "自动放行必须广播 KanbanUpdate 失效通知");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn safety_gate_without_policy_falls_back_to_manual_approval() {
    let workspace = temp("miss");
    let (board, run_id) = board_with_open_run(&workspace);
    drop(board);
    let bus = crate::core::event::EventBus::new(8);
    let auto = handle(&workspace, &run_id, bus.clone());
    // 无 broker：auto 句柄不构成审批通道（ApprovalCtx::new 门禁不变），按拒绝处理
    assert!(ApprovalCtx::new(None, None, None, None, Some(&auto)).is_none());
    assert!(matches!(safety_gate("echo hi", workspace.to_str().unwrap(), None).await, Err(ExecError::Safety { .. })));
    // 有通道但无授权：回落人工审批，无人应答超时按拒绝
    let broker = crate::agent::approval::ApprovalBroker::with_timeout(Duration::from_millis(50));
    let mut events = bus.subscribe();
    let appr = ApprovalCtx { broker: &broker, bus: &bus, cancel: None, session_id: "", auto: Some(&auto) };
    assert!(matches!(safety_gate("echo hi", workspace.to_str().unwrap(), Some(&appr)).await, Err(ExecError::Safety { .. })));
    assert!(
        matches!(events.try_recv(), Ok(crate::core::event::Event::LlmDelta(ref payload)) if payload["kind"] == "approval"),
        "未授权回落必须发起人工审批"
    );
    // 未放行：无授权、无 auto_approved 事件
    let board = Board::open(&workspace, "board_t").unwrap();
    assert!(board.state().policy.is_none());
    assert!(!event_log(&workspace).iter().any(|event| matches!(event.kind, EventKind::AutoApproved(_))));
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn safety_gate_deny_is_never_auto_approved() {
    // allowlist 含 rm，但 rm 是 Safety Deny 档（F5 不可恢复删除）：Deny 在 auto 检查之前返回，物理上不可绕过
    let workspace = temp("deny");
    let (mut board, run_id) = board_with_open_run(&workspace);
    board.apply(KanbanCommand::PolicySet { policy: spec(&["rm"], None, None) }).unwrap();
    drop(board);
    let bus = crate::core::event::EventBus::new(8);
    let auto = handle(&workspace, &run_id, bus.clone());
    let broker = crate::agent::approval::ApprovalBroker::with_timeout(Duration::from_millis(50));
    let appr = ApprovalCtx { broker: &broker, bus: &bus, cancel: None, session_id: "", auto: Some(&auto) };
    let error = safety_gate("rm -rf junk", workspace.to_str().unwrap(), Some(&appr)).await.unwrap_err();
    let ExecError::Safety { rule, .. } = error else { panic!("Deny 命令必须被 Safety 拒绝: {error}") };
    assert_eq!(rule, "F5");
    let board = Board::open(&workspace, "board_t").unwrap();
    assert_eq!(board.state().policy.as_ref().unwrap().used, 0, "Deny 不得消耗授权计数");
    assert!(!event_log(&workspace).iter().any(|event| matches!(event.kind, EventKind::AutoApproved(_))), "Deny 不得落放行审计");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn auto_approved_rejects_closed_run_via_guard() {
    let workspace = temp("closedrun");
    let (mut board, run_id) = board_with_open_run(&workspace);
    board.apply(KanbanCommand::PolicySet { policy: spec(&["echo"], None, None) }).unwrap();
    board.apply(KanbanCommand::RunFinished { run_id: run_id.clone(), outcome: Outcome::Success }).unwrap();
    drop(board);
    let bus = crate::core::event::EventBus::new(8);
    let error = handle(&workspace, &run_id, bus).try_auto_allow("echo hi").unwrap_err();
    assert!(error.contains(&KanbanError::RunNotOpen(run_id).to_string()), "{error}");
    std::fs::remove_dir_all(workspace).ok();
}
