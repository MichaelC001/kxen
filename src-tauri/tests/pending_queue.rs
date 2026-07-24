//! P1-13 pending queue 落盘回归：入队写盘、消费重写、崩溃重启恢复、非法 id 拒绝。

use kxen_app::core::pending_queue::{PendingQueues, file_path};

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kxen-pq-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn ctx_file(path: &str) -> kxen_app::agent::context::ContextItem {
    kxen_app::agent::context::ContextItem::File { path: path.into() }
}

fn img() -> kxen_app::llm::types::ImagePart {
    kxen_app::llm::types::ImagePart { media_type: "image/png".into(), data: "aGVsbG8=".into() }
}

#[test]
fn enqueue_persists_and_pop_rewrites() {
    let dir = tmp_dir("rw");
    let q = PendingQueues::new(dir.clone());
    assert_eq!(q.enqueue("s1", "第一条".into(), vec![ctx_file("a.rs")], vec![img()]), 1);
    assert_eq!(q.enqueue("s1", "第二条".into(), vec![], vec![]), 2);
    assert!(file_path(&dir, "s1").exists(), "入队必须落盘");

    // context/images 随条目完整往返
    let first = q.pop("s1").unwrap();
    assert_eq!(first.text, "第一条");
    assert!(matches!(first.context.first(), Some(kxen_app::agent::context::ContextItem::File { path }) if path == "a.rs"));
    assert_eq!(first.images.len(), 1);
    // 消费后重写：盘上只剩第二条
    let on_disk: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(file_path(&dir, "s1")).unwrap()).unwrap();
    assert_eq!(on_disk.len(), 1);
    assert_eq!(on_disk[0]["text"], "第二条");

    // 排空即删文件：残留空文件会被 restore 当成有效队列
    assert_eq!(q.pop("s1").unwrap().text, "第二条");
    assert!(q.pop("s1").is_none());
    assert!(!file_path(&dir, "s1").exists(), "排空后 queue 文件必须删除");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn restore_recovers_queue_after_restart() {
    let dir = tmp_dir("restart");
    {
        let q = PendingQueues::new(dir.clone());
        q.enqueue("s1", "m1".into(), vec![], vec![]);
        q.enqueue("s1", "m2".into(), vec![], vec![]);
        q.enqueue("s2", "other".into(), vec![], vec![]);
        // s2 排空：restore 不应带出它
        q.pop("s2");
    }
    // 模拟重启：全新实例从磁盘恢复，内存为空的断言靠新实例保证
    let q = PendingQueues::new(dir.clone());
    let mut ready = q.restore();
    ready.sort();
    assert_eq!(ready, vec!["s1".to_string()]);
    // 顺序保持：先进先出
    assert_eq!(q.pop("s1").unwrap().text, "m1");
    assert_eq!(q.pop("s1").unwrap().text, "m2");
    assert!(q.pop("s1").is_none());
    assert!(!q.has_queued("s2"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn clear_removes_memory_and_disk() {
    let dir = tmp_dir("clear");
    let q = PendingQueues::new(dir.clone());
    q.enqueue("s1", "a".into(), vec![], vec![]);
    q.enqueue("s1", "b".into(), vec![], vec![]);
    assert_eq!(q.clear("s1"), 2);
    assert!(!file_path(&dir, "s1").exists());
    assert_eq!(q.restore(), Vec::<String>::new(), "clear 后恢复不得再带出队列");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn invalid_session_id_is_rejected_before_disk() {
    let dir = tmp_dir("badid");
    let before = std::fs::read_dir(&dir).unwrap().count();
    let q = PendingQueues::new(dir.clone());
    assert_eq!(q.enqueue("../escape", "x".into(), vec![], vec![]), 0, "路径穿越 id 必须拒");
    assert!(!q.has_queued("../escape"));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), before, "拒绝发生在落盘之前");
    std::fs::remove_dir_all(&dir).ok();
}
