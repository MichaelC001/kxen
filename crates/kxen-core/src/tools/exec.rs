//! exec 工具：type 必填 + 方言校验 + safety 拦截 + 快照 shell 执行 + auto_bg。
use crate::core::shared::lock;
use crate::tools::safety::{Verdict, evaluate_shell_command};
use crate::tools::shell::{ShellKind, wrap_command};
use crate::tools::task::{TaskHandle, TaskOwner, TaskRegistry, task_id};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

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
    #[error("cancelled")]
    Cancelled,
}

mod dialect;
pub use dialect::validate_dialect;
mod spawn;
pub(crate) use spawn::{RespawnOptions, respawn_task};
pub use spawn::{SpawnOptions, spawn_task, spawn_task_with_env};

/// 审批上下文（Ask 档挂起等待用户决定所需的全部句柄）。
pub struct ApprovalCtx<'a> {
    pub broker: &'a crate::agent::approval::ApprovalBroker,
    pub bus: &'a crate::core::event::EventBus,
    pub cancel: Option<&'a crate::agent::cancel::CancelToken>,
    pub session_id: &'a str,
    /// 看板级自主授权句柄（P3）：命中 allowlist 则自动放行（审计由实现方 durable）；None = 逐次审批。
    pub auto: Option<&'a dyn crate::tools::auto_approve::AutoApprove>,
}

impl<'a> ApprovalCtx<'a> {
    /// broker 与 bus 齐备才算有审批通道（缺一则 Ask 档按拒绝处理，不静默放行）。
    pub fn new(
        broker: Option<&'a crate::agent::approval::ApprovalBroker>,
        bus: Option<&'a crate::core::event::EventBus>,
        cancel: Option<&'a crate::agent::cancel::CancelToken>,
        session_id: Option<&'a str>,
        auto: Option<&'a dyn crate::tools::auto_approve::AutoApprove>,
    ) -> Option<Self> {
        Some(Self { broker: broker?, bus: bus?, cancel, session_id: session_id.unwrap_or(""), auto })
    }
}

/// 宿主机执行闸门：Deny 直接拒绝，其余命令逐次展示完整 command/cwd 并等待用户批准。
/// Shell 不是文件工具的 Workspace sandbox；逐次审批是唯一放行入口，无通道时 fail closed。
/// 评估用的 cwd 必须是真实执行目录（与 spawn 的 current_dir 一致），否则相对路径判定失真。
pub async fn safety_gate(command: &str, cwd: &str, approval: Option<&ApprovalCtx<'_>>) -> Result<(), ExecError> {
    let reason = match evaluate_shell_command(command, cwd) {
        Verdict::Deny { rule_id, reason, suggestion } => Err(ExecError::Safety {
            rule: rule_id.to_string(),
            reason: reason.into_owned(),
            suggestion: suggestion.map(|s| format!(" Suggestion: {s}")).unwrap_or_default(),
        })?,
        Verdict::Ask { reason } => format!("{reason}。Shell 将在宿主机目录 {cwd} 执行，可能访问 Workspace 外数据"),
        Verdict::Recoverable => {
            // 可恢复档文案必须体现可恢复性：删除走系统废纸篓，误删可从废纸篓还原
            format!("命令含可恢复删除（目标移入系统废纸篓，可还原）；Shell 将在宿主机目录 {cwd} 执行，可能访问 Workspace 外数据")
        }
        Verdict::Allow => {
            format!("Shell 将在宿主机目录 {cwd} 执行，可能访问 Workspace 外数据；Kxen 不将其声明为 sandbox")
        }
    };
    let Some(appr) = approval else {
        return Err(ExecError::Safety {
            rule: "approval".into(),
            reason: format!("{reason}（当前上下文无审批通道，按拒绝处理）"),
            suggestion: String::new(),
        });
    };
    // 规则表短路在 Deny 之后、自主授权与逐次审批之前：命中先落 durable 审计再放行；
    // 未命中/失效/审计失败回落后续路径（fail-closed 语义不变），原因只进日志不执行
    match appr.broker.try_rule_allow(appr.session_id, command, &reason) {
        Ok(()) => return Ok(()),
        Err(miss) => tracing::debug!(%miss, "approval rule miss, falling back to auto/manual approval"),
    }
    // 自主授权短路在 Deny 返回之后、人工审批之前：Deny 物理上不可绕过；
    // 未命中/授权失效回落逐次审批（fail-closed 语义不变），原因只进日志不执行
    if let Some(auto) = appr.auto {
        match auto.try_auto_allow(command) {
            Ok(()) => return Ok(()),
            Err(reason) => tracing::debug!(%reason, "auto approve miss, falling back to manual approval"),
        }
    }
    match crate::agent::approval::request_approval(appr, command, &reason).await {
        crate::agent::approval::ApprovalOutcome::Allow => Ok(()),
        crate::agent::approval::ApprovalOutcome::Timeout => {
            Err(ExecError::Safety {
                rule: "approval".into(), reason: format!("{reason}（用户超时未响应）"), suggestion: String::new()
            })
        }
        crate::agent::approval::ApprovalOutcome::Deny => {
            Err(ExecError::Safety {
                rule: "approval".into(), reason: format!("{reason}（用户拒绝或已中断）"), suggestion: String::new()
            })
        }
    }
}

pub async fn exec(
    params: ExecParams,
    registry: &Arc<TaskRegistry>,
    cwd: &str,
    owner: &TaskOwner,
    approval: Option<&ApprovalCtx<'_>>,
) -> Result<ExecOutcome, ExecError> {
    exec_with_env(params, registry, cwd, owner, approval, None).await
}

pub async fn exec_with_env(
    params: ExecParams,
    registry: &Arc<TaskRegistry>,
    cwd: &str,
    owner: &TaskOwner,
    approval: Option<&ApprovalCtx<'_>>,
    child_env: Option<crate::agent::agent_loop::ChildEnvironment>,
) -> Result<ExecOutcome, ExecError> {
    validate_dialect(params.shell_type, &params.command)?;

    let workdir: std::borrow::Cow<'_, str> = if params.path.starts_with('/') {
        std::borrow::Cow::Borrowed(params.path.as_str())
    } else {
        std::borrow::Cow::Owned(format!("{cwd}/{}", params.path))
    };
    safety_gate(&params.command, &workdir, approval).await?;
    let argv = wrap_command(params.shell_type, &workdir, &params.command);

    if params.background {
        let id = task_id();
        let task = spawn_task_with_env(
            &id,
            argv,
            &params.command,
            &workdir,
            registry,
            owner,
            SpawnOptions { port: None, child_env: child_env.clone() },
        )
        .await?;
        // 显式 background 给了 timeout_ms 也要挂看门狗：与 auto-bg 同规约，失控长跑进程不能无限存活
        if let Some(timeout_ms) = params.timeout_ms {
            spawn_timeout_watchdog(registry, &id, task.generation, timeout_ms);
        }
        return Ok(ExecOutcome::Background { task_id: id });
    }

    // 前台：auto_bg 预算内等待，超时自动转后台
    let budget = params.timeout_ms.map(|t| t.min(AUTO_BG_BUDGET_MS)).unwrap_or(AUTO_BG_BUDGET_MS);
    let hard_timeout = params.timeout_ms.unwrap_or(120_000);

    let out_id = task_id();
    let task =
        spawn_task_with_env(&out_id, argv, &params.command, &workdir, registry, owner, SpawnOptions { port: None, child_env }).await?;

    let wait = wait_task(task.clone());
    let sleep = tokio::time::sleep(Duration::from_millis(budget));
    let cancelled = async {
        match approval.and_then(|context| context.cancel) {
            Some(cancel) => cancel.wait().await,
            None => std::future::pending::<()>().await,
        }
    };
    tokio::pin!(sleep);
    tokio::pin!(cancelled);

    tokio::select! {
        (output, exit_code, truncated) = wait => {
            Ok(ExecOutcome::Foreground { output, exit_code, truncated })
        }
        _ = &mut cancelled => {
            registry.kill_if_current(&out_id, task.generation).await;
            Err(ExecError::Cancelled)
        }
        _ = sleep => {
            if hard_timeout <= budget {
                // 模型给了短 timeout 且到点：杀任务报超时
                let _ = registry.kill_if_current(&out_id, task.generation).await;
                return Ok(ExecOutcome::Foreground {
                    output: format!("(timed out after {hard_timeout}ms)\n{}", lock(&task.output)),
                    exit_code: 124,
                    truncated: true,
                });
            }
            // auto background 后仍保留 hard timeout：失控长跑进程不能无限存活
            spawn_timeout_watchdog(registry, &out_id, task.generation, hard_timeout - budget);
            Ok(ExecOutcome::Background { task_id: out_id })
        }
    }
}

/// auto-bg 的 hard timeout 看门狗：到期 kill 整个进程组。
fn spawn_timeout_watchdog(registry: &Arc<TaskRegistry>, task_id: &str, generation: u64, remaining_ms: u64) {
    let registry = registry.clone();
    let id = task_id.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(remaining_ms)).await;
        registry.kill_if_current(&id, generation).await;
    });
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

#[cfg(test)]
#[path = "exec_tests.rs"]
mod tests;
