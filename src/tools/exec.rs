//! exec 工具：type 必填 + 方言校验 + safety 拦截 + 快照 shell 执行 + auto_bg。

use crate::tools::safety::{evaluate_shell_command, Verdict};
use crate::tools::shell::{wrap_command, ShellKind};
use crate::tools::task::{append_capped, task_id, TaskHandle, TaskRegistry};
use crate::core::shared::{lock, SharedStr};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

const OUTPUT_CAP: usize = 64 * 1024;
const AUTO_BG_BUDGET_MS: u64 = 15_000;

#[derive(Debug, Deserialize)]
pub struct ExecParams {
    #[serde(rename = "type")]
    pub shell_type: ShellKind,
    pub path: String,
    pub command: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub background: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ExecOutcome {
    Foreground { output: String, exit_code: i32, truncated: bool },
    Background { task_id: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("dialect: {0}")]
    Dialect(String),
    #[error("blocked by safety rule {rule}: {reason}{suggestion}")]
    Safety { rule: String, reason: String, suggestion: String },
    #[error("spawn failed: {0}")]
    Spawn(String),
}

/// 方言校验（命中即拒绝 + 纠正文案）。
pub fn validate_dialect(kind: ShellKind, command: &str) -> Result<(), ExecError> {
    let hint = match kind {
        ShellKind::Fish if command.contains("export ") => {
            Some("fish has no `export`. Use `set -x NAME value`.")
        }
        ShellKind::Zsh if command.contains("[0]") => Some("zsh arrays are 1-indexed, not 0-indexed."),
        _ => None,
    };
    match hint {
        Some(h) => Err(ExecError::Dialect(h.to_string())),
        None => Ok(()),
    }
}

pub async fn exec(params: ExecParams, registry: &Arc<TaskRegistry>, cwd: &str) -> Result<ExecOutcome, ExecError> {
    validate_dialect(params.shell_type, &params.command)?;

    match evaluate_shell_command(&params.command, cwd) {
        Verdict::Deny { rule_id, reason, suggestion } => {
            return Err(ExecError::Safety {
                rule: rule_id.to_string(),
                reason: reason.into_owned(),
                suggestion: suggestion.map(|s| format!(" Suggestion: {s}")).unwrap_or_default(),
            });
        }
        _ => {}
    }

    let workdir: std::borrow::Cow<'_, str> = if params.path.starts_with('/') {
        std::borrow::Cow::Borrowed(params.path.as_str())
    } else {
        std::borrow::Cow::Owned(format!("{cwd}/{}", params.path))
    };
    let argv = wrap_command(params.shell_type, &workdir, &params.command);

    if params.background {
        let id = spawn_task(argv, &params.command, &workdir, registry, None).await?;
        return Ok(ExecOutcome::Background { task_id: id });
    }

    // 前台：auto_bg 预算内等待，超时自动转后台
    let budget = params.timeout_ms.map(|t| t.min(AUTO_BG_BUDGET_MS)).unwrap_or(AUTO_BG_BUDGET_MS);
    let hard_timeout = params.timeout_ms.unwrap_or(120_000);

    let out_id = spawn_task(argv, &params.command, &workdir, registry, None).await?;
    let task = registry.get(&out_id).expect("spawned task must be registered");

    let wait = wait_task(task.clone());
    let sleep = tokio::time::sleep(Duration::from_millis(budget));
    tokio::pin!(sleep);

    tokio::select! {
        (output, exit_code, truncated) = wait => {
            Ok(ExecOutcome::Foreground { output, exit_code, truncated })
        }
        _ = sleep => {
            if hard_timeout <= budget {
                // 模型给了短 timeout 且到点：杀任务报超时
                let _ = registry.kill(&out_id).await;
                return Ok(ExecOutcome::Foreground {
                    output: format!("(timed out after {hard_timeout}ms)\n{}", lock(&task.output)),
                    exit_code: 124,
                    truncated: true,
                });
            }
            Ok(ExecOutcome::Background { task_id: out_id })
        }
    }
}

async fn wait_task(task: Arc<TaskHandle>) -> (String, i32, bool) {
    // 轮询退出状态（简单可靠；task 结束时 child.wait 已写 exit_code）
    loop {
        if let Some(code) = *lock(&task.exit_code) {
            let output = lock(&task.output).clone();
            let truncated = *lock(&task.truncated);
            return (output, code, truncated);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub async fn spawn_task(
    argv: Vec<String>,
    display_command: &str,
    workdir: &str,
    registry: &Arc<TaskRegistry>,
    port: Option<u16>,
) -> Result<String, ExecError> {
    let id = task_id();
    let (bin, args) = argv.split_first().ok_or_else(|| ExecError::Spawn("empty argv".into()))?;
    let mut child = Command::new(bin)
        .args(args)
        .current_dir(workdir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| ExecError::Spawn(format!("{bin}: {e}")))?;

    let output = Arc::new(Mutex::new(String::new()));
    let truncated = Arc::new(Mutex::new(false));
    let exit_code = Arc::new(Mutex::new(None));
    let pid = child.id();

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let handle = Arc::new(TaskHandle {
        id: id.clone(),
        command: SharedStr::from(display_command),
        workdir: SharedStr::from(workdir),
        output: output.clone(),
        truncated: truncated.clone(),
        started_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        pid,
        exit_code: exit_code.clone(),
        child: Arc::new(Mutex::new(None)),
        port,
    });
    registry.register(handle.clone());

    // 输出泵（合并 stdout/stderr 按到达顺序）
    if let Some(mut out) = stdout {
        let (output, truncated) = (output.clone(), truncated.clone());
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            while let Ok(n) = out.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                append_capped(&output, &truncated, &String::from_utf8_lossy(&buf[..n]), OUTPUT_CAP);
            }
        });
    }
    if let Some(mut err) = stderr {
        let (output, truncated) = (output.clone(), truncated.clone());
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            while let Ok(n) = err.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                append_capped(&output, &truncated, &String::from_utf8_lossy(&buf[..n]), OUTPUT_CAP);
            }
        });
    }

    // 退出收割
    tokio::spawn(async move {
        let status = child.wait().await;
        let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
        *exit_code.lock().expect("exit") = Some(code);
    });

    Ok(id)
}
