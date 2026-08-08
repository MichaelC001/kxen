use super::*;
use crate::kanban::model::PolicySpec;

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-policy-{tag}-{}-{nanos}", std::process::id()))
}

fn open_board(workspace: &Path) -> Board {
    let mut board = Board::open(workspace, "board_t").unwrap();
    board.apply(KanbanCommand::BoardCreate { title: "授权看板".into(), columns: None }).unwrap();
    board
}

fn spec(allowlist: &[&str], expires_at_ms: Option<u64>, max_uses: Option<u32>) -> PolicySpec {
    PolicySpec { allowlist: allowlist.iter().map(|prefix| prefix.to_string()).collect(), expires_at_ms, max_uses }
}

/// 在 implementing 列直接建卡并 claim，返回 open run_id。
fn open_run(board: &mut Board) -> String {
    let event = board
        .apply(KanbanCommand::CardCreate { column_id: Some("implementing".into()), title: "任务".into(), body: String::new() })
        .unwrap();
    let EventKind::CardCreate(card) = event.kind else { panic!("expected card_create") };
    let run = board.apply(KanbanCommand::RunStarted { card_id: card.card_id }).unwrap();
    let EventKind::RunStarted(started) = run.kind else { panic!("expected run_started") };
    started.run_id
}

fn reject(board: &mut Board, command: KanbanCommand) -> KanbanError {
    board.apply(command).unwrap_err()
}

/// 守卫拒绝必须零副作用：事件流长度与投影序列化在拒绝前后完全一致。
fn assert_fail_closed(board: &Board, workspace: &Path, before_events: usize, before_state: String) {
    let dir = store::board_dir(workspace, "board_t").unwrap();
    assert_eq!(store::load_events(&store::events_path(&dir)).unwrap().len(), before_events, "拒绝的命令不得落事件");
    assert_eq!(serde_json::to_string(board.state()).unwrap(), before_state, "拒绝的命令不得改投影");
}

#[test]
fn policy_set_happy_path_and_reset_on_renew() {
    let workspace = temp("set");
    let mut board = open_board(&workspace);
    let run_id = open_run(&mut board);
    board.apply(KanbanCommand::PolicySet { policy: spec(&["cargo", "git status"], None, Some(2)) }).unwrap();
    assert_eq!(board.state().policy.as_ref().unwrap().used, 0);
    board.apply(KanbanCommand::AutoApproved { run_id, command: "cargo test".into() }).unwrap();
    assert_eq!(board.state().policy.as_ref().unwrap().used, 1);
    // 重设即重置计数（显式续期语义）
    board.apply(KanbanCommand::PolicySet { policy: spec(&["cargo"], None, None) }).unwrap();
    let policy = board.state().policy.as_ref().unwrap();
    assert_eq!(policy.used, 0);
    assert_eq!(policy.spec.allowlist, ["cargo"]);
    // 重启重放：授权从事件流确定性恢复
    let reopened = Board::open(&workspace, "board_t").unwrap();
    assert_eq!(reopened.state().policy, board.state().policy.clone());
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn policy_set_rejects_misconfiguration_fail_closed() {
    let workspace = temp("setguard");
    let mut board = Board::open(&workspace, "board_t").unwrap();
    assert!(matches!(
        reject(&mut board, KanbanCommand::PolicySet { policy: spec(&["cargo"], None, None) }),
        KanbanError::BoardNotCreated(_)
    ));
    board.apply(KanbanCommand::BoardCreate { title: "t".into(), columns: None }).unwrap();
    let dir = store::board_dir(&workspace, "board_t").unwrap();
    let events = store::load_events(&store::events_path(&dir)).unwrap().len();
    let state = serde_json::to_string(board.state()).unwrap();

    // 空 allowlist / 空白前缀
    assert!(matches!(reject(&mut board, KanbanCommand::PolicySet { policy: spec(&[], None, None) }), KanbanError::InvalidCommand(_)));
    assert!(matches!(
        reject(&mut board, KanbanCommand::PolicySet { policy: spec(&["cargo", "  "], None, None) }),
        KanbanError::InvalidCommand(_)
    ));
    // max_uses = 0 等于设了立即失效
    assert!(matches!(
        reject(&mut board, KanbanCommand::PolicySet { policy: spec(&["cargo"], None, Some(0)) }),
        KanbanError::InvalidCommand(_)
    ));
    // 过去时 expires
    assert!(matches!(
        reject(&mut board, KanbanCommand::PolicySet { policy: spec(&["cargo"], Some(now_ms() - 1), None) }),
        KanbanError::InvalidCommand(_)
    ));
    assert_fail_closed(&board, &workspace, events, state);
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn auto_approved_guards_fail_closed() {
    let workspace = temp("autoguard");
    let mut board = open_board(&workspace);
    let run_id = open_run(&mut board);
    // 无授权
    assert!(matches!(
        reject(&mut board, KanbanCommand::AutoApproved { run_id: run_id.clone(), command: "cargo test".into() }),
        KanbanError::PolicyDenied(_)
    ));
    board.apply(KanbanCommand::PolicySet { policy: spec(&["cargo", "git status"], None, Some(3)) }).unwrap();
    let dir = store::board_dir(&workspace, "board_t").unwrap();
    let events = store::load_events(&store::events_path(&dir)).unwrap().len();
    let state = serde_json::to_string(board.state()).unwrap();

    // 前缀不匹配
    assert!(matches!(
        reject(&mut board, KanbanCommand::AutoApproved { run_id: run_id.clone(), command: "npm test".into() }),
        KanbanError::PolicyDenied(_)
    ));
    // run 不存在
    assert!(matches!(
        reject(&mut board, KanbanCommand::AutoApproved { run_id: "r_nope".into(), command: "cargo test".into() }),
        KanbanError::RunNotOpen(_)
    ));
    assert_fail_closed(&board, &workspace, events, state);

    // trim_start 后命中前缀即放行
    board.apply(KanbanCommand::AutoApproved { run_id: run_id.clone(), command: "  cargo test".into() }).unwrap();
    assert_eq!(board.state().policy.as_ref().unwrap().used, 1);
    // run 已结束（其余守卫全过，抵达 open_run 检查）
    board.apply(KanbanCommand::RunFinished { run_id: run_id.clone(), outcome: Outcome::Success }).unwrap();
    assert!(matches!(
        reject(&mut board, KanbanCommand::AutoApproved { run_id, command: "cargo build".into() }),
        KanbanError::RunNotOpen(_)
    ));
    // 次数用尽：重设 max_uses=1（计数重置），放行一次后即耗尽
    let run_id2 = open_run(&mut board);
    board.apply(KanbanCommand::PolicySet { policy: spec(&["cargo"], None, Some(1)) }).unwrap();
    board.apply(KanbanCommand::AutoApproved { run_id: run_id2.clone(), command: "cargo test".into() }).unwrap();
    assert!(matches!(
        reject(&mut board, KanbanCommand::AutoApproved { run_id: run_id2.clone(), command: "cargo build".into() }),
        KanbanError::PolicyDenied(_)
    ));
    // 过期：窗口给足避免并行负载下 set 守卫竞态（过去时拒绝已由上方用例覆盖），
    // 轮询等过期生效而非定长 sleep（负载下 sleep 返回时点不可控）
    board.apply(KanbanCommand::PolicySet { policy: spec(&["cargo"], Some(now_ms() + 1_500), None) }).unwrap();
    let mut denied = false;
    for _ in 0..200 {
        match board.apply(KanbanCommand::AutoApproved { run_id: run_id2.clone(), command: "cargo test".into() }) {
            Err(KanbanError::PolicyDenied(_)) => {
                denied = true;
                break;
            }
            // 未过期时放行属预期（事件无害：无次数上限且断言只看终态），继续等过期
            _ => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    assert!(denied, "过期后 AutoApproved 必须 PolicyDenied");
    std::fs::remove_dir_all(workspace).ok();
}
