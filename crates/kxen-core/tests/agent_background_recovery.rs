// 后台子代理中断补投集成测试：进程死在子代理完结前，重启把「中断事实」投递给父 session pending queue。
// 覆盖：Shutdown 投递 + marker 幂等、done 不投递、多 session 聚合、非法目录名跳过、marker 主锚单跳。

use kxen_core::agent::background::recover_interrupted;
use kxen_core::core::pending_queue::PendingQueues;
use std::path::{Path, PathBuf};

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-bgrec-{tag}-{}-{nanos}", std::process::id()))
}

fn write_transcript(dir: &Path, sid: &str, name: &str, lines: &str) {
    let agents = dir.join(sid).join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(agents.join(format!("{name}.transcript.jsonl")), lines).unwrap();
}

fn marker(dir: &Path, sid: &str, name: &str) -> PathBuf {
    dir.join(sid).join("agents").join(format!("{name}.shutdown-notified"))
}

fn delivery_ids(queues: &PendingQueues, sid: &str) -> Vec<String> {
    queues.snapshot(sid).unwrap().into_iter().map(|item| item.id).collect()
}

#[test]
fn shutdown_agent_gets_interruption_notice_and_is_idempotent() {
    let dir = temp("shutdown");
    let session = kxen_core::core::session::create(&dir, "/tmp").unwrap();
    let sid = session.id;
    write_transcript(&dir, &sid, "kxen-research-abc123", "{\"kind\":\"text\",\"text\":\"half\"}\n");
    let queues = PendingQueues::new(dir.clone());

    assert_eq!(recover_interrupted(&queues, &dir), 1, "Shutdown 子代理必须补投");
    let snapshot = queues.snapshot(&sid).unwrap();
    assert_eq!(snapshot.len(), 1, "队列恰好一条补投");
    assert_eq!(snapshot[0].id, "bgshutdown-kxen-research-abc123", "确定性 delivery id");
    assert!(snapshot[0].text.contains("interrupted"), "通知文本含中断事实: {}", snapshot[0].text);
    assert!(marker(&dir, &sid, "kxen-research-abc123").exists(), "投递后写 marker");

    assert_eq!(recover_interrupted(&queues, &dir), 0, "重复恢复不重复投递");
    assert_eq!(delivery_ids(&queues, &sid), vec!["bgshutdown-kxen-research-abc123".to_string()], "队列仍只有一条");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn done_transcript_is_not_re_delivered() {
    let dir = temp("done");
    let session = kxen_core::core::session::create(&dir, "/tmp").unwrap();
    let sid = session.id;
    write_transcript(&dir, &sid, "kxen-exec-def456", "{\"kind\":\"text\",\"text\":\"ok\"}\n{\"kind\":\"done\",\"turns\":2}\n");
    let queues = PendingQueues::new(dir.clone());

    assert_eq!(recover_interrupted(&queues, &dir), 0, "done = 进程死前已完结，不投递");
    assert!(queues.snapshot(&sid).unwrap().is_empty());
    assert!(!marker(&dir, &sid, "kxen-exec-def456").exists(), "未投递不写 marker");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn two_sessions_each_recover_one_shutdown_agent() {
    let dir = temp("multi");
    let first = kxen_core::core::session::create(&dir, "/tmp").unwrap().id;
    let second = kxen_core::core::session::create(&dir, "/tmp").unwrap().id;
    write_transcript(&dir, &first, "kxen-research-aaa111", "{\"kind\":\"text\",\"text\":\"a\"}\n");
    write_transcript(&dir, &second, "kxen-exec-bbb222", "{\"kind\":\"text\",\"text\":\"b\"}\n");
    let queues = PendingQueues::new(dir.clone());

    assert_eq!(recover_interrupted(&queues, &dir), 2, "两个 session 各补投一条");
    assert_eq!(delivery_ids(&queues, &first), vec!["bgshutdown-kxen-research-aaa111".to_string()]);
    assert_eq!(delivery_ids(&queues, &second), vec!["bgshutdown-kxen-exec-bbb222".to_string()]);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn invalid_session_dir_names_are_skipped_without_panic() {
    let dir = temp("invalid");
    // 空格、点、shell 风格的目录名不过 validate_id：整棵跳过且不 panic
    for name in ["bad name", ".hidden", "..dots"] {
        write_transcript(&dir, name, "kxen-research-ccc333", "{\"kind\":\"text\",\"text\":\"x\"}\n");
    }
    // sessions_dir 下的普通文件也不是 session，跳过
    std::fs::write(dir.join("stray.queue.json"), "{}").unwrap();
    let queues = PendingQueues::new(dir.clone());

    assert_eq!(recover_interrupted(&queues, &dir), 0);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn existing_marker_without_queued_delivery_skips() {
    let dir = temp("marker");
    let session = kxen_core::core::session::create(&dir, "/tmp").unwrap();
    let sid = session.id;
    write_transcript(&dir, &sid, "kxen-research-ddd444", "{\"kind\":\"text\",\"text\":\"half\"}\n");
    // 主锚：marker 在而队列无该 delivery（上轮投递已消费）-> 不重投
    std::fs::write(marker(&dir, &sid, "kxen-research-ddd444"), "1").unwrap();
    let queues = PendingQueues::new(dir.clone());

    assert_eq!(recover_interrupted(&queues, &dir), 0, "marker 主锚单跳");
    assert!(queues.snapshot(&sid).unwrap().is_empty());
    std::fs::remove_dir_all(dir).ok();
}
