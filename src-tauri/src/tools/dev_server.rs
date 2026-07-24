//! dev_server 管理：就绪等待（pattern/端口）、restart、list、健康检查。

use crate::tools::exec::{spawn_task, ExecError};
use crate::tools::shell::{wrap_command, ShellKind};
use crate::tools::task::TaskRegistry;
use crate::core::shared::lock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

const READY_DEFAULT_TIMEOUT_MS: u64 = 30_000;
const READY_DEFAULT_PATTERNS: &[&str] = &["listening", "ready", "started", "watching", "serving", "compiled"];
const HEALTH_CHECK_INTERVAL_MS: u64 = 30_000;

#[derive(Debug, Deserialize)]
pub struct DevServerParams {
    pub command: String,
    pub workdir: String,
    #[serde(default)]
    pub ready: Option<ReadySpec>,
    #[serde(default)]
    pub shell: Option<ShellKind>,
}

#[derive(Debug, Deserialize)]
pub struct ReadySpec {
    pub pattern: Option<String>,
    pub port: Option<u16>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct DevServerStarted {
    pub task_id: String,
    pub url: Option<String>,
    pub pid: Option<u32>,
}

/// 启动 dev server 并阻塞等待就绪。
pub async fn dev_server(params: DevServerParams, registry: &Arc<TaskRegistry>) -> Result<DevServerStarted, ExecError> {
    let shell = params.shell.unwrap_or(ShellKind::Zsh);
    let ready = params.ready.unwrap_or(ReadySpec { pattern: None, port: None, timeout_ms: None });
    let timeout = ready.timeout_ms.unwrap_or(READY_DEFAULT_TIMEOUT_MS);

    let argv = wrap_command(shell, &params.workdir, &params.command);
    let task_id = spawn_task(argv, &params.command, &params.workdir, registry, ready.port).await?;
    let task = registry.get(&task_id).expect("just spawned");

    // 健康检查后台挂上
    spawn_health_check(task.clone(), registry.clone());

    let result = tokio::time::timeout(Duration::from_millis(timeout), wait_ready(task.clone(), ready.pattern.clone(), ready.port)).await;
    match result {
        // 就绪但无 url 是正常成功（pattern 命中但输出解析不到端口）
        Ok(Ready::Ready(url)) => Ok(DevServerStarted { task_id, url, pid: task.pid }),
        Ok(Ready::Exited(code)) => {
            // 进程就绪前退出：必须报错带退出信息，不得伪装成「成功但 url 为 None」
            let tail = lock(&task.output).clone();
            Err(ExecError::Spawn(format!(
                "dev server exited before ready (exit code {code}). tail:\n{}",
                crate::tools::task::tail_of(&tail, 800)
            )))
        }
        Err(_) => {
            // readiness 超时：进程必须跟着死（复用进程组 SIGTERM->SIGKILL），不留孤儿
            registry.kill(&task_id).await;
            let tail = lock(&task.output).clone();
            Err(ExecError::Spawn(format!(
                "dev server not ready within {timeout}ms. tail:\n{}",
                crate::tools::task::tail_of(&tail, 800)
            )))
        }
    }
}

/// wait_ready 的两种收敛：就绪（url 可能解析不到）与进程提前退出（带退出码）。
enum Ready {
    Ready(Option<String>),
    Exited(i32),
}

async fn wait_ready(task: Arc<crate::tools::task::TaskHandle>, pattern: Option<String>, port: Option<u16>) -> Ready {
    let patterns: Vec<String> = pattern
        .map(|p| vec![p.to_lowercase()])
        .unwrap_or_else(|| READY_DEFAULT_PATTERNS.iter().map(|s| s.to_string()).collect());

    loop {
        // 进程提前退出 -> 失败
        if let Some(code) = *lock(&task.exit_code) {
            return Ready::Exited(code);
        }
        // pattern 匹配
        {
            let output = lock(&task.output);
            let lower = output.to_lowercase();
            if patterns.iter().any(|p| lower.contains(p)) {
                let port_found = match port {
                    Some(p) => Some(p),
                    None => {
                        let parsed = parse_port(&output);
                        // 解析出的 port 写回 task 状态：health 检查与 task.list 共用同一份
                        *lock(&task.port) = parsed;
                        if parsed.is_none() {
                            tracing::warn!("ready pattern 命中但输出里解析不到 port");
                        }
                        parsed
                    }
                };
                return Ready::Ready(port_found.map(|p| format!("http://localhost:{p}")));
            }
        }
        // 端口可达
        if let Some(p) = port {
            if tokio::net::TcpStream::connect(("127.0.0.1", p)).await.is_ok() {
                return Ready::Ready(Some(format!("http://localhost:{p}")));
            }
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
    }
}

fn parse_port(output: &str) -> Option<u16> {
    static RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?:localhost|127\.0\.0\.1|:):(\d{4,5})\b").unwrap());
    RE.captures(output)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
        .or_else(|| {
            static RE2: std::sync::LazyLock<regex::Regex> =
                std::sync::LazyLock::new(|| regex::Regex::new(r"port\s+(\d{4,5})").unwrap());
            RE2.captures(output).and_then(|c| c.get(1)).and_then(|m| m.as_str().parse().ok())
        })
}

fn spawn_health_check(task: Arc<crate::tools::task::TaskHandle>, registry: Arc<TaskRegistry>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(HEALTH_CHECK_INTERVAL_MS)).await;
            if lock(&task.exit_code).is_some() {
                break;
            }
            // port 由 readiness 解析后写回（spawn 时可能没有）：每轮现读，没有就跳过本轮
            let Some(port) = *lock(&task.port) else { continue };
            if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_err() {
                // 端口失连：进程活着但服务死了——标 Failed 让 list 可见
                //（status 目前是 Copy 写在 handle 里，简化：kill 并标记）
                let _ = registry.kill(&task.id).await;
                break;
            }
        }
    });
}

pub async fn restart_task(id: &str, registry: &Arc<TaskRegistry>) -> Result<String, ExecError> {
    let task = registry.get(id).ok_or_else(|| ExecError::Spawn(format!("task not found: {id}")))?;
    let (command, workdir) = (task.command.clone(), task.workdir.clone());
    let port = *lock(&task.port);
    registry.kill(id).await;
    // 给旧进程退出时间
    tokio::time::sleep(Duration::from_millis(300)).await;
    let argv = wrap_command(ShellKind::Zsh, &workdir, &command);
    spawn_task(argv, &command, &workdir, registry, port).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ports() {
        assert_eq!(parse_port("listening on http://localhost:7823/"), Some(7823));
        assert_eq!(parse_port("ready at 127.0.0.1:4096"), Some(4096));
        assert_eq!(parse_port("server port 3000 ready"), Some(3000));
        assert_eq!(parse_port("no port here"), None);
    }

    #[tokio::test]
    async fn early_exit_is_error_with_exit_info() {
        let registry = Arc::new(TaskRegistry::new());
        let params = DevServerParams {
            command: "exit 3".into(),
            workdir: std::env::temp_dir().to_string_lossy().into_owned(),
            ready: None,
            shell: Some(ShellKind::Zsh),
        };
        let err = dev_server(params, &registry).await.expect_err("进程提前退出必须报错");
        let msg = err.to_string();
        assert!(msg.contains("exit code 3"), "报错须含退出信息: {msg}");
    }

    #[tokio::test]
    async fn ready_without_url_is_success() {
        let registry = Arc::new(TaskRegistry::new());
        let params = DevServerParams {
            command: "echo ready; sleep 30".into(),
            workdir: std::env::temp_dir().to_string_lossy().into_owned(),
            ready: None,
            shell: Some(ShellKind::Zsh),
        };
        let started = dev_server(params, &registry).await.expect("就绪但无 url 属正常成功");
        assert!(started.url.is_none());
        registry.kill(&started.task_id).await;
    }
}
