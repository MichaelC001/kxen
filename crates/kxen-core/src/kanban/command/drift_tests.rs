//! apply 锁内漂移预检：另一实例/锁外写入推进事件流后，本实例先补折再校验。

use super::*;
use crate::kanban::model::{ColumnDef, OnEnter, OnEnterKind, Transitions};

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-drift-{tag}-{}-{nanos}", std::process::id()))
}

fn events_len(workspace: &Path) -> usize {
    let dir = store::board_dir(workspace, "board_t").unwrap();
    store::load_events(&store::events_path(&dir)).unwrap().len()
}

fn wip_one_columns() -> Vec<ColumnDef> {
    vec![
        ColumnDef {
            id: "queued".into(),
            title: "queued".into(),
            on_enter: OnEnter { kind: OnEnterKind::None, agent: None },
            transitions: Transitions::default(),
            wip_limit: Some(1),
            timeout_ms: None,
        },
        ColumnDef {
            id: "done".into(),
            title: "done".into(),
            on_enter: OnEnter { kind: OnEnterKind::None, agent: None },
            transitions: Transitions::default(),
            wip_limit: None,
            timeout_ms: None,
        },
    ]
}

#[test]
fn apply_reloads_state_after_other_instance_appended() {
    let workspace = temp("reload");
    let mut first = Board::open(&workspace, "board_t").unwrap();
    first.apply(KanbanCommand::BoardCreate { title: "t".into(), columns: None }).unwrap();
    // 另一实例（共享 board_lock 的正常路径）推进事件流
    let mut second = Board::open(&workspace, "board_t").unwrap();
    second.apply(KanbanCommand::CardCreate { column_id: None, title: "来自第二实例".into(), body: String::new() }).unwrap();
    assert_eq!(first.state().seq, 1, "本实例投影仍是旧的");
    // 本实例 apply 合法命令：先补折再校验，seq 连续
    let event = first.apply(KanbanCommand::CardCreate { column_id: None, title: "本实例".into(), body: String::new() }).unwrap();
    assert_eq!(event.seq, 3);
    assert_eq!(first.state().seq, 3);
    assert_eq!(first.state().cards.len(), 2, "补折后第二实例的卡也在投影里");
    assert_eq!(events_len(&workspace), 3);
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn apply_stale_projection_rejects_without_durable_write() {
    let workspace = temp("stale");
    let mut first = Board::open(&workspace, "board_t").unwrap();
    first.apply(KanbanCommand::BoardCreate { title: "t".into(), columns: Some(wip_one_columns()) }).unwrap();
    // 第二实例把 WIP=1 的列占满；第一实例投影里该列还是空的
    let mut second = Board::open(&workspace, "board_t").unwrap();
    second.apply(KanbanCommand::CardCreate { column_id: Some("queued".into()), title: "占位".into(), body: String::new() }).unwrap();
    let before = events_len(&workspace);
    // 仅对旧投影合法的命令：补折后必须被拒，且非法事件零 durable
    let error =
        first.apply(KanbanCommand::CardCreate { column_id: Some("queued".into()), title: "超限".into(), body: String::new() }).unwrap_err();
    assert!(matches!(error, KanbanError::WipLimit { .. }), "{error:?}");
    assert_eq!(events_len(&workspace), before, "非法事件不得落盘");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn apply_times_out_when_another_process_holds_events_lock() {
    let workspace = temp("locked");
    let mut board = Board::open(&workspace, "board_t").unwrap();
    // 模拟另一进程持锁：独立 File 句柄抢先拿住 events.lock
    let dir = store::board_dir(&workspace, "board_t").unwrap();
    let _held = store::lock_events(&dir).unwrap();
    let started = std::time::Instant::now();
    let error = board.apply(KanbanCommand::BoardCreate { title: "t".into(), columns: None }).unwrap_err();
    assert!(error.to_string().contains("locked by another process"), "{error}");
    assert!(started.elapsed() < std::time::Duration::from_secs(3), "测试超时时长已缩小，不得等满生产 5s");
    assert_eq!(events_len(&workspace), 0, "锁冲突不得落任何事件");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn apply_recovers_from_lock_external_write() {
    let workspace = temp("external");
    let mut first = Board::open(&workspace, "board_t").unwrap();
    first.apply(KanbanCommand::BoardCreate { title: "t".into(), columns: None }).unwrap();
    // 模拟锁外写入者：直接往事件流追加一条合法事件
    let dir = store::board_dir(&workspace, "board_t").unwrap();
    let mut forged = KanbanEvent {
        id: ids::new_id("kev"),
        board_id: "board_t".into(),
        seq: 0,
        created_at: now_ms(),
        kind: EventKind::AgentDefined(AgentDefinedPayload {
            name: "external-agent".into(),
            role: "r".into(),
            model: "m".into(),
            permission_profile: "full".into(),
            tools: None,
        }),
    };
    store::append_event(&store::events_path(&dir), &mut forged).unwrap();
    // 本实例 apply：预检发现漂移，先补折再校验
    let event = first.apply(KanbanCommand::CardCreate { column_id: None, title: "补折后".into(), body: String::new() }).unwrap();
    assert_eq!(event.seq, 3);
    assert_eq!(first.state().seq, 3);
    assert!(first.state().agents.contains_key("external-agent"), "锁外写入的事件必须补折进投影");
    assert_eq!(events_len(&workspace), 3);
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn apply_reloads_state_after_equal_length_rewrite() {
    let workspace = temp("rewrite");
    let mut board = Board::open(&workspace, "board_t").unwrap();
    board.apply(KanbanCommand::BoardCreate { title: "旧看板".into(), columns: None }).unwrap();
    // 模拟锁外写入者等长重写：同条数同 seq，不同事件 id 与内容（合法事件流，纯 seq 比对会漏检）
    let dir = store::board_dir(&workspace, "board_t").unwrap();
    let rewritten = KanbanEvent {
        id: ids::new_id("kev"),
        board_id: "board_t".into(),
        seq: 1,
        created_at: now_ms(),
        kind: EventKind::BoardCreate(BoardCreatePayload { title: "重写后".into(), columns: wip_one_columns() }),
    };
    let mut line = serde_json::to_vec(&rewritten).unwrap();
    line.push(b'\n');
    std::fs::write(store::events_path(&dir), line).unwrap();
    // 本实例 apply：内容锚不符先补折，后续行为必须基于新投影（旧投影里根本没有 queued 列）
    board.apply(KanbanCommand::CardCreate { column_id: Some("queued".into()), title: "占满 WIP".into(), body: String::new() }).unwrap();
    assert_eq!(board.state().title.as_deref(), Some("重写后"), "必须按重写后的内容补折");
    let error =
        board.apply(KanbanCommand::CardCreate { column_id: Some("queued".into()), title: "超限".into(), body: String::new() }).unwrap_err();
    assert!(matches!(error, KanbanError::WipLimit { .. }), "WIP 判定必须按新投影: {error:?}");
    std::fs::remove_dir_all(workspace).ok();
}
