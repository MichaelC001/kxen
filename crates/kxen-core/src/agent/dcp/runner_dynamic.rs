//! DCP 动态工具（dyn__）装配：族能力复验、宏目录加载、提案自动审批通道。
//! 独立文件守 350 行门禁（runner.rs 承载 run 主流程）；语义集中此处可审。
//!
//! 锁语义不变：锁只含族名 `dynamic-tools`（预声明 optional/required），宏实例不进锁；
//! 实例由宏目录在锁解析与每次 run 准备时加载（hash 自洽校验），历史里的 dyn__ 调用
//! 必须能由当前宏目录解析，否则 fail-closed（UNKNOWN 语义：副作用模板不可判定）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::runner_support::DcpAutoApprove;
use super::{DcpAgentLock, DcpRuntimePolicy};

/// 锁能力复验（execute_run 每次 run 前）：可用性 + 闭集（allowed/denied）+ 闭集特例
/// （allow_shell/allow_mcp/allow_code_orchestration/allow_dynamic_tools）同口径。
pub(super) fn verify_locked_capabilities(
    policy: &DcpRuntimePolicy,
    available: &std::collections::BTreeSet<String>,
    capabilities: &[String],
) -> Result<(), String> {
    for capability in capabilities {
        if !available.contains(capability)
            || policy.denied_capabilities.contains(capability)
            || policy.allowed_capabilities.as_ref().is_some_and(|allowed| !allowed.contains(capability))
            || (!policy.allow_shell && matches!(capability.as_str(), "exec" | "task"))
            || (!policy.allow_mcp && capability.starts_with("mcp__"))
            || (!policy.allow_code_orchestration && capability == "workflow")
            || (!policy.allow_dynamic_tools && crate::agent::dynamic::is_dynamic_capability(capability))
        {
            return Err(format!("locked DCPAgent capability is unavailable under the current runtime policy: {capability}"));
        }
    }
    Ok(())
}

/// 锁解析时的宏目录校验（新 session 创建后 fail fast）：族进锁即要求宏目录可定位且加载干净
/// （任一文件 hash 不符整目录不可用），坏宏在 session 创建时报错而不是首个 run 半路炸。
pub(super) fn validate_at_lock(policy_file: Option<&Path>, lock: &DcpAgentLock) -> Result<(), String> {
    if !lock.effective_capabilities.iter().any(|name| name == crate::agent::dynamic::FAMILY) {
        return Ok(());
    }
    let dir = macro_dir(policy_file)?;
    crate::agent::dynamic::macros::load_active(&dir).map(|_| ())
}

/// run 准备：族在锁内才把宏目录加载进会话注册表（tool_define 据此走提案模式），
/// 并复验历史里的 dyn__ 调用都可解析（宏目录缺失/内容变更 = fail closed）。
pub(super) fn mount(
    policy_file: Option<&Path>,
    capabilities: &[String],
    extras: &crate::agent::agent_loop::SessionExtras,
    history: &[crate::core::session::Message],
) -> Result<(), String> {
    if !capabilities.iter().any(|name| name == crate::agent::dynamic::FAMILY) {
        return Ok(());
    }
    let dir = macro_dir(policy_file)?;
    crate::agent::dynamic::macros::load_into_extras(&dir, extras)?;
    crate::agent::dynamic::verify_history_references(history, extras)
}

fn macro_dir(policy_file: Option<&Path>) -> Result<PathBuf, String> {
    crate::agent::dynamic::macros::macro_dir_for(policy_file)
        .ok_or_else(|| format!("{} capability requires a policy file to locate the macro directory", crate::agent::dynamic::FAMILY))
}

/// 提案审批的自主授权装配：allow_shell 与 allow_dynamic_tools 各带独立审计文件，
/// 按命令前缀路由（tool_define 走 dynamic_tool_define 审计，其余维持 shell_command 口径）。
/// 两者都关 = None（exec 回落逐次审批/fail-closed 的既有路径不变）。
pub(super) fn auto_approve(policy: &DcpRuntimePolicy, run_dir: &Path) -> Option<Arc<dyn crate::tools::auto_approve::AutoApprove>> {
    if !policy.allow_shell && !policy.allow_dynamic_tools {
        return None;
    }
    Some(Arc::new(DcpCompositeAutoApprove {
        shell: policy.allow_shell.then(|| DcpAutoApprove::new(run_dir.join("shell-audit.jsonl"), "shell_command")),
        dynamic: policy.allow_dynamic_tools.then(|| DcpAutoApprove::new(run_dir.join("dynamic-tool-audit.jsonl"), "dynamic_tool_define")),
    }))
}

/// 按命令前缀分流的双通道自主授权：未命中的一路返回 Err，调用方回落逐次审批（fail-closed 语义不变）。
pub(super) struct DcpCompositeAutoApprove {
    shell: Option<DcpAutoApprove>,
    dynamic: Option<DcpAutoApprove>,
}

impl crate::tools::auto_approve::AutoApprove for DcpCompositeAutoApprove {
    fn try_auto_allow(&self, command: &str) -> Result<(), String> {
        if command.starts_with("tool_define ") {
            return self
                .dynamic
                .as_ref()
                .ok_or_else(|| "dynamic tool definitions are not auto-approved under this policy".to_string())
                .and_then(|auto| auto.try_auto_allow(command));
        }
        self.shell
            .as_ref()
            .ok_or_else(|| "shell commands are not auto-approved under this policy".to_string())
            .and_then(|auto| auto.try_auto_allow(command))
    }
}
