// dev server 生命周期测试（P1-33）：readiness timeout 杀进程组、解析 port 写回 task 状态。
// 走 kxen_app 公共 API，与 tests/safety_eval.rs 同一拆分先例（350 行门禁）。
use kxen_app::core::shared::lock;
use kxen_app::tools::dev_server::{DevServerParams, ReadySpec, dev_server};
use kxen_app::tools::task::TaskRegistry;
use std::sync::Arc;
use std::time::Duration;

fn params(command: &str, timeout_ms: u64) -> DevServerParams {
    DevServerParams {
        command: command.into(),
        workdir: "/tmp".into(),
        ready: Some(ReadySpec {
            pattern: None,
            port: None,
            timeout_ms: Some(timeout_ms),
        }),
        shell: None,
    }
}

fn pid_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// readiness timeout 后已启动的进程必须不在（睡眠型假 server，永不 ready）。
#[tokio::test]
async fn timeout_kills_started_process() {
    let registry = Arc::new(TaskRegistry::new());
    let err = dev_server(params("sleep 30", 300), &registry)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not ready"), "got {err}");

    let info = registry.list();
    let task = registry.get(&info[0].id).expect("task registered");
    let pid = task.pid.expect("spawned pid");
    // kill 内部 TERM 宽限最长 800ms，且收割任务异步写 exit_code：轮询等落定（远小于 sleep 30，到期即证明是被杀的）
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while lock(&task.exit_code).is_none() && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        lock(&task.exit_code).is_some(),
        "timeout 后任务应已被 kill，不得继续运行"
    );
    assert!(!pid_alive(pid), "timeout 后进程 {pid} 不得存活");
}

/// 未显式给 port 时，从输出解析出的 port 要写回 task 状态（health/list 共用同一份）。
#[tokio::test]
async fn parsed_port_written_back_to_task() {
    let registry = Arc::new(TaskRegistry::new());
    // 假 server：输出固定格式 port 行命中默认 ready pattern，然后挂住
    let started = dev_server(
        params(
            "echo 'listening on http://localhost:49217/'; sleep 30",
            5_000,
        ),
        &registry,
    )
    .await
    .expect("pattern 命中应 ready");
    assert_eq!(started.url.as_deref(), Some("http://localhost:49217"));

    let task = registry.get(&started.task_id).expect("task registered");
    assert_eq!(
        *lock(&task.port),
        Some(49217),
        "解析出的 port 应写回 task 状态"
    );
    let info = registry
        .list()
        .into_iter()
        .find(|t| t.id == started.task_id)
        .expect("listed");
    assert_eq!(info.port, Some(49217), "list 快照应带解析出的 port");

    registry.kill(&started.task_id).await;
}
