// exec/task 后台进程 DCP 集成测试：spawn/exit/killed 落盘、中断恢复补投、幂等。
// 覆盖：start->exit 行序（persist-before-deliver）、kill 落 killed 行不补投、
// start 收尾投递 + reaped 幂等锚、exit/killed/reaped 收尾不恢复、非法目录名跳过、
// kanban scope owner 不落盘、存活孤儿进程被 SIGTERM 回收。

use kxen_core::agent::background::{NotifyRouter, notify_on_task_exit, recover_interrupted_tasks};
use kxen_core::core::pending_queue::PendingQueues;
use kxen_core::core::shared::lock;
use kxen_core::tools::shell::{default_shell, wrap_command};
use kxen_core::tools::task::{TaskOwner, TaskRegistry, task_id};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-taskrec-{tag}-{}-{nanos}", std::process::id()))
}

fn journal_lines(dir: &Path, sid: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join(sid).join("tasks.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// 手工造 tasks.jsonl：session 目录由 create 懒建，写日志前确保存在
fn write_journal(dir: &Path, sid: &str, lines: &str) {
    let sub = dir.join(sid);
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("tasks.jsonl"), lines).unwrap();
}

fn kinds(lines: &[serde_json::Value]) -> Vec<&str> {
    lines.iter().map(|line| line["kind"].as_str().unwrap()).collect()
}

/// 真实 spawn 一个短命令（方言无关：CI Linux runner 无 zsh，用默认 shell）。
async fn spawn(registry: &Arc<TaskRegistry>, owner: &TaskOwner, command: &str) -> String {
    let argv = wrap_command(default_shell(), "/tmp", command);
    let id = task_id();
    kxen_core::tools::exec::spawn_task(&id, argv, command, "/tmp", registry, owner, None).await.expect("spawn");
    id
}

#[test]
fn missing_sessions_directory_has_no_interrupted_tasks() {
    let dir = temp("first-start");
    assert!(!dir.exists());
    let queues = PendingQueues::new(dir.clone());

    assert!(recover_interrupted_tasks(&queues, &dir).is_empty());
    assert!(!dir.exists(), "recovery scan must not create storage on a clean first start");
}

#[tokio::test]
async fn spawn_and_exit_persist_start_then_exit_before_notification() {
    let dir = temp("exit");
    let registry = Arc::new(TaskRegistry::with_sessions_dir(dir.clone()));
    let owner = TaskOwner::new("sess-tj-exit", "/tmp").unwrap();
    let id = spawn(&registry, &owner, "echo hi").await;
    let router = Arc::new(NotifyRouter::new());
    notify_on_task_exit(registry.clone(), &owner, &id, router.clone());

    // watcher 100ms 一拍，5s 预算足够；exit 行在 exit_code 公开前落盘，通知必然更晚
    let mut note = None;
    for _ in 0..50 {
        let notes = router.drain();
        if !notes.is_empty() {
            note = Some(notes.join("\n"));
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(note.expect("自行退出必须有通知").contains(&id));

    let lines = journal_lines(&dir, "sess-tj-exit");
    assert_eq!(kinds(&lines), ["start", "exit"], "行序即持久化先于通知的证据: {lines:?}");
    assert_eq!(lines[0]["task_id"].as_str().unwrap(), id);
    assert_eq!(lines[0]["command"].as_str().unwrap(), "echo hi");
    assert!(lines[0]["pid"].as_u64().is_some(), "start 行必须带 pid");
    assert_eq!(lines[1]["exit_code"].as_i64().unwrap(), 0);
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn kill_persists_killed_line_and_recovery_stays_silent() {
    let dir = temp("kill");
    let registry = Arc::new(TaskRegistry::with_sessions_dir(dir.clone()));
    let owner = TaskOwner::new("sess-tj-kill", "/tmp").unwrap();
    let id = spawn(&registry, &owner, "sleep 30").await;
    let router = Arc::new(NotifyRouter::new());
    notify_on_task_exit(registry.clone(), &owner, &id, router.clone());
    assert!(registry.kill(&owner, &id).await, "kill 成功");

    // 等收割协程落定（killed 置位 -> 不写 exit 行）
    let task = registry.get(&owner, &id).unwrap();
    for _ in 0..50 {
        if lock(&task.exit_code).is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(lock(&task.exit_code).is_some(), "被 kill 的进程必须收割");

    let lines = journal_lines(&dir, "sess-tj-kill");
    assert_eq!(kinds(&lines), ["start", "killed"], "主动 kill 落 killed 行且无 exit 行: {lines:?}");
    assert!(router.drain().is_empty(), "主动 kill 不得通知");

    // killed 收尾 = 已收口：重启恢复不补投
    let queues = PendingQueues::new(dir.clone());
    assert!(recover_interrupted_tasks(&queues, &dir).is_empty());
    assert!(queues.snapshot("sess-tj-kill").unwrap().is_empty());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn interrupted_task_is_delivered_reaped_and_idempotent() {
    let dir = temp("recover");
    let session = kxen_core::core::session::create(&dir, "/tmp").unwrap();
    let sid = session.id;
    let start = "{\"kind\":\"start\",\"task_id\":\"task_dead01\",\"command\":\"sleep 300\",\"workdir\":\"/tmp\",\"pid\":4000000,\"started_at\":1}\n";
    write_journal(&dir, &sid, start);
    let queues = PendingQueues::new(dir.clone());

    // pid 4_000_000 合法但无对应进程：kill -0 失败按不存活处理，不发信号，直接补投
    assert_eq!(recover_interrupted_tasks(&queues, &dir), vec![sid.clone()], "start 收尾必须补投");
    let snapshot = queues.snapshot(&sid).unwrap();
    assert_eq!(snapshot.len(), 1, "队列恰好一条补投");
    assert_eq!(snapshot[0].id, "bgtask-task_dead01", "确定性 delivery id");
    assert!(snapshot[0].text.contains("was interrupted by a process restart"), "通知含中断事实: {}", snapshot[0].text);
    assert!(snapshot[0].text.contains("sleep 300"), "通知含命令摘要: {}", snapshot[0].text);

    let lines = journal_lines(&dir, &sid);
    assert_eq!(kinds(&lines), ["start", "reaped"], "投递成功后落 reaped 幂等锚: {lines:?}");

    assert!(recover_interrupted_tasks(&queues, &dir).is_empty(), "重复恢复不重复投递");
    assert_eq!(queues.snapshot(&sid).unwrap().len(), 1, "队列仍只有一条");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn closed_tasks_are_not_recovered() {
    let dir = temp("closed");
    let session = kxen_core::core::session::create(&dir, "/tmp").unwrap();
    let sid = session.id;
    let lines = concat!(
        "{\"kind\":\"start\",\"task_id\":\"task_a\",\"command\":\"true\",\"workdir\":\"/tmp\",\"pid\":100,\"started_at\":1}\n",
        "{\"kind\":\"exit\",\"task_id\":\"task_a\",\"exit_code\":0,\"ended_at\":2}\n",
        "{\"kind\":\"start\",\"task_id\":\"task_b\",\"command\":\"sleep 9\",\"workdir\":\"/tmp\",\"pid\":101,\"started_at\":1}\n",
        "{\"kind\":\"killed\",\"task_id\":\"task_b\",\"ended_at\":2}\n",
        "{\"kind\":\"start\",\"task_id\":\"task_c\",\"command\":\"sleep 9\",\"workdir\":\"/tmp\",\"pid\":102,\"started_at\":1}\n",
        "{\"kind\":\"reaped\",\"task_id\":\"task_c\",\"ended_at\":2}\n",
    );
    write_journal(&dir, &sid, lines);
    let queues = PendingQueues::new(dir.clone());

    assert!(recover_interrupted_tasks(&queues, &dir).is_empty(), "exit/killed/reaped 收尾均已收口");
    assert!(queues.snapshot(&sid).unwrap().is_empty());
    assert_eq!(journal_lines(&dir, &sid).len(), 6, "未投递不得改动日志");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn invalid_session_dirs_are_skipped() {
    let dir = temp("invalid");
    // 空格、点、shell 风格的目录名不过 validate_id：整棵跳过且不 panic
    for name in ["bad name", ".hidden", "..dots"] {
        let sub = dir.join(name);
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            sub.join("tasks.jsonl"),
            "{\"kind\":\"start\",\"task_id\":\"task_x\",\"command\":\"s\",\"workdir\":\"/tmp\",\"pid\":1,\"started_at\":1}\n",
        )
        .unwrap();
    }
    std::fs::write(dir.join("stray.queue.json"), "{}").unwrap();
    let queues = PendingQueues::new(dir.clone());

    assert!(recover_interrupted_tasks(&queues, &dir).is_empty());
    std::fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn kanban_scope_owner_writes_no_journal() {
    let dir = temp("kanban");
    let registry = Arc::new(TaskRegistry::with_sessions_dir(dir.clone()));
    // kanban 的 exec_scope owner 含冒号过不了 id 校验：回执明说无通知，无承诺即无洞，不落盘
    let owner = TaskOwner::new("kanban:r1", "/tmp").unwrap();
    spawn(&registry, &owner, "echo hi").await;

    assert!(!dir.join("kanban:r1").exists(), "kanban scope 不得创建日志目录");
    assert!(std::fs::read_dir(&dir).map(|mut e| e.next().is_none()).unwrap_or(true), "sessions 根不得有任何落盘");
    std::fs::remove_dir_all(dir).ok();
}

#[cfg(unix)]
#[tokio::test]
async fn live_orphan_is_sigtermed_and_delivered() {
    let dir = temp("orphan");
    // pending queue 投递要求 session meta 存在（与生产一致：日志本就落在真实 session 目录下）
    let sid = kxen_core::core::session::create(&dir, "/tmp").unwrap().id;
    let registry = Arc::new(TaskRegistry::with_sessions_dir(dir.clone()));
    let owner = TaskOwner::new(&sid, "/tmp").unwrap();
    let id = spawn(&registry, &owner, "sleep 300").await;
    let pid = registry.get(&owner, &id).unwrap().pid.expect("spawn 必有 pid");

    // 模拟进程重启后的恢复：同一 tasks.jsonl，内存注册表不再参与
    let queues = PendingQueues::new(dir.clone());
    assert_eq!(recover_interrupted_tasks(&queues, &dir), vec![sid.clone()], "存活孤儿必须补投");
    let snapshot = queues.snapshot(&sid).unwrap();
    assert_eq!(snapshot[0].id, format!("bgtask-{id}"));
    assert!(journal_lines(&dir, &sid).iter().any(|line| line["kind"] == "reaped"), "投递后落 reaped 行");

    // 管道断裂的残留进程已被 SIGTERM（进程组）：kill -0 探测转死
    let pid_alive = || {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    let mut reaped = false;
    for _ in 0..30 {
        if !pid_alive() {
            reaped = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(reaped, "存活孤儿进程必须被 SIGTERM 回收");
    std::fs::remove_dir_all(dir).ok();
}
