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
        Ok(url) => Ok(DevServerStarted { task_id, url, pid: task.pid }),
        Err(_) => {
            let tail = lock(&task.output).clone();
            Err(ExecError::Spawn(format!(
                "dev server not ready within {timeout}ms. tail:\n{}",
                crate::tools::task::tail_of(&tail, 800)
            )))
        }
    }
}

async fn wait_ready(task: Arc<crate::tools::task::TaskHandle>, pattern: Option<String>, port: Option<u16>) -> Option<String> {
    let patterns: Vec<String> = pattern
        .map(|p| vec![p.to_lowercase()])
        .unwrap_or_else(|| READY_DEFAULT_PATTERNS.iter().map(|s| s.to_string()).collect());

    loop {
        // 进程提前退出 -> 失败
        if lock(&task.exit_code).is_some() {
            return None;
        }
        // pattern 匹配
        {
            let output = lock(&task.output);
            let lower = output.to_lowercase();
            if patterns.iter().any(|p| lower.contains(p)) {
                let port_found = port.or_else(|| parse_port(&output));
                return port_found.map(|p| format!("http://localhost:{p}"));
            }
        }
        // 端口可达
        if let Some(p) = port {
            if tokio::net::TcpStream::connect(("127.0.0.1", p)).await.is_ok() {
                return Some(format!("http://localhost:{p}"));
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
    let Some(port) = task.port else { return };
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(HEALTH_CHECK_INTERVAL_MS)).await;
            if lock(&task.exit_code).is_some() {
                break;
            }
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
    let port = task.port;
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
}
