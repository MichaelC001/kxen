// fs_tool 公开 API 集成测试（从 fs_tool.rs 拆出，350 行门禁）：
// read 分页 / 文件新鲜度（纳秒精度）/ edit 双模式。
use kxen_app::tools::fs_tool::{AnchorEdit, EditSpec, FileTracker, FsToolError, edit, read};
use kxen_app::tools::hashline::generate_anchors;
use std::path::PathBuf;

fn temp_file(tag: &str, content: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kxen-fstool-{tag}-{}-{}", std::process::id(), rand()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.txt");
    std::fs::write(&path, content).unwrap();
    path
}

fn rand() -> u32 {
    // 纳秒 + 进程内序号混合：并行测试同纳秒也不再撞目录（flake 实证）
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    nanos ^ SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed).wrapping_mul(0x9e37)
}

// ---------------- read 分页 ----------------

#[test]
fn read_default_window_unchanged() {
    let body: String = (1..=10).map(|i| format!("line{i:02}\n")).collect();
    let path = temp_file("default", &body);
    let tracker = FileTracker::default();
    let r = read(&path, &tracker, "/tmp", None, None).unwrap();
    assert_eq!(r.total_lines, 10);
    assert_eq!(r.start_line, 1);
    assert_eq!(r.end_line, 10);
    assert!(!r.truncated);
    assert!(r.content.contains("line01") && r.content.contains("line10"));
}

#[test]
fn read_offset_limit_pages() {
    let body: String = (1..=10).map(|i| format!("line{i:02}\n")).collect();
    let path = temp_file("paging", &body);
    let tracker = FileTracker::default();

    let r = read(&path, &tracker, "/tmp", Some(3), Some(4)).unwrap();
    assert_eq!((r.start_line, r.end_line, r.total_lines), (3, 6, 10));
    assert!(r.truncated, "第 6 行后还有内容");
    assert!(r.content.contains("line03") && r.content.contains("line06"));
    assert!(!r.content.contains("line07"));

    // 尾段：truncated 为 false，agent 知道读完了
    let tail = read(&path, &tracker, "/tmp", Some(7), None).unwrap();
    assert_eq!((tail.start_line, tail.end_line), (7, 10));
    assert!(!tail.truncated);
    assert!(tail.content.contains("line10"));

    // offset 越界：空窗口（end_line < start_line），由调用侧出提示
    let beyond = read(&path, &tracker, "/tmp", Some(50), None).unwrap();
    assert!(beyond.end_line < beyond.start_line);
    assert!(beyond.content.is_empty());
}

#[test]
fn paged_anchors_work_for_edit() {
    // 分页读出的锚点基于全文计算，必须能直接用于锚点编辑（否则分页 read 会废掉 edit）
    let body: String = (1..=30).map(|i| format!("line{i:02}\n")).collect();
    let path = temp_file("anchors", &body);
    let tracker = FileTracker::default();

    let page = read(&path, &tracker, "/tmp", Some(21), Some(10)).unwrap();
    let line25 = page.content.lines().find(|l| l.contains("line25")).expect("line25 in page");
    let anchor = line25.split_whitespace().next().unwrap().trim().to_string();
    assert!(anchor.starts_with("25#"), "窗口内锚点须保留全文行号: {anchor}");

    let spec = EditSpec::Anchors { edits: vec![AnchorEdit { anchor, new_text: "LINE25".into() }] };
    assert_eq!(edit(&path, &spec, &tracker, "/tmp").unwrap().applied, 1);
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("LINE25"));
    assert!(after.contains("line24") && after.contains("line26"), "只命中第 25 行");
}

// ---------------- 文件新鲜度 ----------------

#[test]
fn fresh_unchanged_and_size_change() {
    let path = temp_file("fresh", "hello\n");
    let tracker = FileTracker::default();
    assert!(!tracker.fresh(&path), "未 mark 的文件不算 fresh");
    tracker.mark(&path);
    assert!(tracker.fresh(&path));

    std::fs::write(&path, "hello world\n").unwrap();
    assert!(!tracker.fresh(&path), "size 变了必须检出");
}

#[test]
fn fresh_detects_same_second_same_size_rewrite() {
    // 秒级 mtime + size 会漏掉同秒同长度改写；纳秒精度下必须检出
    let path = temp_file("samesec", "AAAA\n");
    let tracker = FileTracker::default();
    tracker.mark(&path);
    // 等 5ms 保证纳秒级 mtime 必然前进（APFS 为纳秒粒度）
    std::thread::sleep(std::time::Duration::from_millis(5));
    std::fs::write(&path, "BBBB\n").unwrap();
    assert!(!tracker.fresh(&path), "同秒同大小改写也必须检出");
}

// ---------------- edit（迁移自 fs_tool.rs 体内测试） ----------------

#[test]
fn anchor_edit_roundtrip() {
    let path = temp_file("roundtrip", "alpha\nbeta\ngamma\n");
    let tracker = FileTracker::default();
    tracker.mark(&path);
    let lines: Vec<&str> = "alpha\nbeta\ngamma\n".lines().collect();
    let anchors = generate_anchors(&lines);
    let spec = EditSpec::Anchors { edits: vec![AnchorEdit { anchor: anchors[1].to_string(), new_text: "BETA".into() }] };
    let result = edit(&path, &spec, &tracker, "/tmp").unwrap();
    assert_eq!(result.applied, 1);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "alpha\nBETA\ngamma\n");
}

#[test]
fn match_edit_ambiguous() {
    let path = temp_file("ambiguous", "x\nx\n");
    let tracker = FileTracker::default();
    tracker.mark(&path);
    let spec = EditSpec::Match { old_string: "x".into(), new_string: "y".into(), expected_replacements: None };
    assert!(matches!(edit(&path, &spec, &tracker, "/tmp"), Err(FsToolError::Ambiguous { .. })));
}
