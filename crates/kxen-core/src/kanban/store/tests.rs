use super::*;
use crate::kanban::events::*;
use crate::kanban::model::default_template;

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-store-{tag}-{}-{nanos}", std::process::id()))
}

fn sample_event(seq: u64) -> KanbanEvent {
    KanbanEvent {
        id: format!("kev_{seq}"),
        board_id: "board_t".into(),
        seq,
        created_at: 1_000 + seq,
        kind: EventKind::BoardCreate(BoardCreatePayload { title: format!("看板{seq}"), columns: default_template() }),
    }
}

fn write_events(workspace: &Path, board_id: &str, count: u64) -> Vec<KanbanEvent> {
    let dir = board_dir(workspace, board_id).unwrap();
    let path = events_path(&dir);
    let mut events = Vec::new();
    for seq in 1..=count {
        let mut event = sample_event(seq);
        // 只有第一个事件能是 board_create，其余用 agent_defined 凑数
        if seq > 1 {
            event.kind = EventKind::AgentDefined(AgentDefinedPayload {
                name: format!("agent_{seq}"),
                role: "r".into(),
                model: "m".into(),
                permission_profile: "p".into(),
                tools: None,
            });
        }
        append_event(&path, &mut event).unwrap();
        events.push(event);
    }
    events
}

#[test]
fn board_id_path_traversal_rejected() {
    for bad in ["../escape", "a/b", "a b", "中文字符", ""] {
        assert!(matches!(board_dir(Path::new("/tmp"), bad), Err(KanbanError::InvalidId(_))), "应拒绝: {bad:?}");
    }
}

#[test]
fn torn_line_blocks_load_and_append() {
    let dir = temp("torn");
    let path = dir.join("b/events.jsonl");
    let mut first = sample_event(1);
    append_event(&path, &mut first).unwrap();
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(br#"{"id":"#).unwrap();
    file.sync_all().unwrap();
    drop(file);
    let before = std::fs::read(&path).unwrap();

    assert!(load_events(&path).is_err(), "torn 行必须阻断读取");
    assert!(append_event(&path, &mut sample_event(2)).is_err(), "torn 行必须阻断追加");
    assert_eq!(std::fs::read(&path).unwrap(), before, "失败路径不得继续追加 torn JSONL");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn corrupt_middle_line_and_bad_enum_fail_closed() {
    let dir = temp("corrupt");
    let path = dir.join("events.jsonl");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&path, "{\"not\":\"an event\"}\n").unwrap();
    assert!(load_events(&path).is_err(), "缺字段事件必须拒绝");
    std::fs::write(
        &path,
        concat!(r#"{"id":"kev_1","board_id":"b","seq":1,"created_at":1,"kind":{"type":"teleport","payload":{}}}"#, "\n",),
    )
    .unwrap();
    assert!(load_events(&path).is_err(), "未知事件类型必须拒绝");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn non_contiguous_seq_rejected() {
    let dir = temp("seq");
    let path = dir.join("events.jsonl");
    std::fs::create_dir_all(&dir).unwrap();
    let line = serde_json::to_string(&sample_event(7)).unwrap();
    std::fs::write(&path, format!("{line}\n")).unwrap();
    let error = load_events(&path).unwrap_err();
    assert!(error.to_string().contains("non-contiguous"), "{error}");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn idempotent_append_skips_duplicate_and_rejects_conflict() {
    let dir = temp("idem");
    let path = dir.join("events.jsonl");
    let mut event = sample_event(1);
    append_event(&path, &mut event).unwrap();
    let mut replay = event.clone();
    replay.created_at = 999;
    replay.seq = 0;
    append_event(&path, &mut replay).unwrap();
    assert_eq!(load_events(&path).unwrap().len(), 1, "同 id 同内容重放不得写双份");
    assert_eq!(replay.seq, 1, "幂等跳过后应回填已存 seq");
    let mut conflict = sample_event(1);
    conflict.id = event.id.clone();
    conflict.kind = EventKind::RunTimeout(RunTimeoutPayload { run_id: "r".into() });
    assert!(append_event(&path, &mut conflict).unwrap_err().to_string().contains("collision"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn snapshot_deleted_or_corrupt_rebuilds_from_events() {
    let workspace = temp("rebuild");
    let events = write_events(&workspace, "board_a", 4);
    let expected = serde_json::to_string(&projection::replay("board_a", &events).unwrap()).unwrap();
    let dir = board_dir(&workspace, "board_a").unwrap();

    // 无快照：从事件流重建
    let state = load_state(&workspace, "board_a").unwrap();
    save_snapshot(&dir, &state).unwrap();
    assert!(snapshot_path(&dir).is_file());
    assert_eq!(serde_json::to_string(&state).unwrap(), expected);

    // 删除快照：重建结果与全量重放逐字节一致
    std::fs::remove_file(snapshot_path(&dir)).unwrap();
    assert_eq!(serde_json::to_string(&load_state(&workspace, "board_a").unwrap()).unwrap(), expected);

    // 快照损坏（垃圾字节）：同样回退到事件流重建
    std::fs::write(snapshot_path(&dir), b"garbage{{{").unwrap();
    assert_eq!(serde_json::to_string(&load_state(&workspace, "board_a").unwrap()).unwrap(), expected);

    // 快照属于别的 board：锚点不符，重建
    let other = projection::BoardState::new("board_other");
    std::fs::write(snapshot_path(&dir), serde_json::to_vec(&other).unwrap()).unwrap();
    assert_eq!(serde_json::to_string(&load_state(&workspace, "board_a").unwrap()).unwrap(), expected);
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn stale_snapshot_tail_folds_to_same_state() {
    let workspace = temp("tail");
    let events = write_events(&workspace, "board_b", 5);
    let dir = board_dir(&workspace, "board_b").unwrap();
    // 快照停在 seq 3：启动只补折 seq 4..5，结果必须等于全量重放
    let partial = projection::replay("board_b", &events[..3]).unwrap();
    save_snapshot(&dir, &partial).unwrap();
    let loaded = load_state(&workspace, "board_b").unwrap();
    assert_eq!(serde_json::to_string(&loaded).unwrap(), serde_json::to_string(&projection::replay("board_b", &events).unwrap()).unwrap());
    assert_eq!(loaded.seq, 5);
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn snapshot_anchor_matches_tail_event() {
    let workspace = temp("anchor");
    let events = write_events(&workspace, "board_t", 3);
    let dir = board_dir(&workspace, "board_t").unwrap();
    save_snapshot(&dir, &projection::replay("board_t", &events).unwrap()).unwrap();
    let loaded = load_state(&workspace, "board_t").unwrap();
    assert_eq!(loaded.anchor_event_id.as_deref(), Some(events[2].id.as_str()), "快照路径加载后锚必须与尾事件一致");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn tampered_snapshot_anchor_falls_back_to_replay() {
    let workspace = temp("tamper");
    let events = write_events(&workspace, "board_t", 3);
    let expected = serde_json::to_string(&projection::replay("board_t", &events).unwrap()).unwrap();
    let dir = board_dir(&workspace, "board_t").unwrap();
    // 伪造快照：board_id/seq 锚点都合法、内容被篡改、anchor 指到不存在的事件
    let mut forged = projection::replay("board_t", &events).unwrap();
    forged.title = Some("被篡改".into());
    forged.anchor_event_id = Some("kev_forged".into());
    save_snapshot(&dir, &forged).unwrap();
    let loaded = load_state(&workspace, "board_t").unwrap();
    assert_eq!(serde_json::to_string(&loaded).unwrap(), expected, "锚不符必须全量 replay，篡改内容不得入投影");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn legacy_snapshot_without_anchor_replays_then_reanchors() {
    let workspace = temp("legacy");
    let events = write_events(&workspace, "board_t", 3);
    let expected = serde_json::to_string(&projection::replay("board_t", &events).unwrap()).unwrap();
    let dir = board_dir(&workspace, "board_t").unwrap();
    // 旧格式快照：手工删掉 anchor_event_id 字段
    let mut json = serde_json::to_value(projection::replay("board_t", &events).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("anchor_event_id");
    std::fs::write(snapshot_path(&dir), serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(serde_json::to_string(&load_state(&workspace, "board_t").unwrap()).unwrap(), expected, "旧快照必须 replay 兜底加载");
    // replay 后重存的新快照带锚，之后走快照路径
    save_snapshot(&dir, &load_state(&workspace, "board_t").unwrap()).unwrap();
    assert_eq!(load_state(&workspace, "board_t").unwrap().anchor_event_id.as_deref(), Some(events[2].id.as_str()));
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn last_event_seq_reads_tail_only() {
    let workspace = temp("lastseq");
    let missing = workspace.join("board_c").join("events.jsonl");
    assert_eq!(last_event_seq(&missing).unwrap(), None, "文件不存在 = 空事件流");
    write_events(&workspace, "board_c", 3);
    let dir = board_dir(&workspace, "board_c").unwrap();
    let path = events_path(&dir);
    assert_eq!(last_event_seq(&path).unwrap(), Some(3));
    // 超长尾行（超过 8KiB 预检窗口）：扩窗后仍取到正确 seq
    let mut long = sample_event(4);
    long.kind = EventKind::CardComment(CardCommentPayload { card_id: "c".into(), author: "a".into(), body: "x".repeat(16 * 1024) });
    append_event(&path, &mut long).unwrap();
    assert_eq!(last_event_seq(&path).unwrap(), Some(4));
    // torn 尾行：解析失败必须 Err，不猜
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(br#"{"id":"#).unwrap();
    file.sync_all().unwrap();
    assert!(last_event_seq(&path).is_err(), "torn 尾行不得猜 seq");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn events_lock_excludes_second_handle_until_released() {
    let dir = temp("flock");
    let guard = lock_events(&dir).unwrap();
    // 模拟另一进程：独立 File 句柄对同一锁文件 try_lock 必冲突（flock 按打开文件描述互斥）
    let other = || std::fs::OpenOptions::new().read(true).write(true).open(dir.join("events.lock")).unwrap();
    assert!(other().try_lock().is_err(), "同一锁文件的第二句柄必须冲突");
    drop(guard);
    assert!(other().try_lock().is_ok(), "持锁句柄释放后必须能拿到锁");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn last_event_seq_empty_file_is_none() {
    let dir = temp("emptyseq");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("events.jsonl");
    std::fs::write(&path, b"").unwrap();
    assert_eq!(last_event_seq(&path).unwrap(), None);
    std::fs::remove_dir_all(dir).ok();
}
