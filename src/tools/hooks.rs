//! hooks：config.toml [hooks] 配置的事件钩子（默认全部关闭）。
//! pre_tool_use 非零退出 -> 阻断工具调用；post_tool_use 仅记录。
//! hook 命令与 exec 同过 safety 拦截；环境变量 KXEN_EVENT / KXEN_TOOL / KXEN_PAYLOAD 注入。

use crate::core::config::{Config, HookDef};
use crate::tools::safety::{evaluate_shell_command, Verdict};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

const HOOK_TIMEOUT: Duration = Duration::from_secs(10);

pub struct HookRunner {
    hooks: HashMap<String, Vec<CompiledHook>>,
}

struct CompiledHook {
    matcher: Option<regex::Regex>,
    command: String,
}

impl HookRunner {
    pub fn from_config(config: &Config) -> Self {
        let mut hooks = HashMap::new();
        for (event, defs) in &config.hooks {
            let compiled: Vec<CompiledHook> = defs
                .iter()
                .map(|d: &HookDef| CompiledHook {
                    matcher: d.matcher.as_deref().and_then(|m| regex::Regex::new(m).ok()),
                    command: d.command.clone(),
                })
                .collect();
            if !compiled.is_empty() {
                hooks.insert(event.clone(), compiled);
            }
        }
        Self { hooks }
    }

    pub fn is_empty(&self) -> bool {
        self.hooks.values().all(|v| v.is_empty())
    }

    /// pre_tool_use：任一匹配 hook 失败（非零退出 / 被 safety 拦 / 超时）即阻断。
    pub async fn run_pre(&self, tool: &str, payload: &Value) -> Result<(), String> {
        for hook in self.matching("pre_tool_use", tool) {
            self.execute(hook, "pre_tool_use", tool, payload).await?;
        }
        Ok(())
    }

    /// post_tool_use：失败只记日志，不影响工具结果。
    pub async fn run_post(&self, tool: &str, payload: &Value) {
        for hook in self.matching("post_tool_use", tool) {
            if let Err(reason) = self.execute(hook, "post_tool_use", tool, payload).await {
                tracing::warn!(tool, reason, "post_tool_use hook failed");
            }
        }
    }

    fn matching(&self, event: &str, tool: &str) -> Vec<&CompiledHook> {
        self.hooks
            .get(event)
            .map(|defs| defs.iter().filter(|h| h.matcher.as_ref().is_none_or(|m| m.is_match(tool))).collect())
            .unwrap_or_default()
    }

    async fn execute(&self, hook: &CompiledHook, event: &str, tool: &str, payload: &Value) -> Result<(), String> {
        if let Verdict::Deny { rule_id, reason, .. } = evaluate_shell_command(&hook.command, "/") {
            return Err(format!("hook blocked by safety rule {rule_id}: {reason}"));
        }
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let result = tokio::time::timeout(
            HOOK_TIMEOUT,
            tokio::process::Command::new("/bin/zsh")
                .arg("-c")
                .arg(&hook.command)
                .env("KXEN_EVENT", event)
                .env("KXEN_TOOL", tool)
                .env("KXEN_PAYLOAD", &payload_str)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .output(),
        )
        .await;
        match result {
            Err(_) => Err(format!("hook timed out after {}s", HOOK_TIMEOUT.as_secs())),
            Ok(Err(e)) => Err(format!("hook spawn failed: {e}")),
            Ok(Ok(out)) if out.status.success() => Ok(()),
            Ok(Ok(out)) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(format!("hook exited {}: {}", out.status.code().unwrap_or(-1), stderr.chars().take(200).collect::<String>()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn runner(toml_str: &str) -> HookRunner {
        let config: Config = toml::from_str(toml_str).unwrap();
        HookRunner::from_config(&config)
    }

    #[tokio::test]
    async fn pre_hook_blocks_on_nonzero_exit() {
        let r = runner(r#"
[[hooks.pre_tool_use]]
matcher = "exec"
command = "exit 1"
"#);
        let err = r.run_pre("exec", &json!({})).await.unwrap_err();
        assert!(err.contains("exited 1"), "unexpected: {err}");
        // 不匹配的工具不受影响
        assert!(r.run_pre("read", &json!({})).await.is_ok());
    }

    #[tokio::test]
    async fn pre_hook_receives_env() {
        let r = runner(r#"
[[hooks.pre_tool_use]]
command = "test \"$KXEN_TOOL\" = \"exec\" && test \"$KXEN_EVENT\" = \"pre_tool_use\""
"#);
        assert!(r.run_pre("exec", &json!({"command": "ls"})).await.is_ok());
    }

    #[tokio::test]
    async fn safety_denied_hook_blocks() {
        let r = runner(r#"
[[hooks.pre_tool_use]]
command = "rm -rf /"
"#);
        let err = r.run_pre("exec", &json!({})).await.unwrap_err();
        assert!(err.contains("safety"), "unexpected: {err}");
    }

    #[tokio::test]
    async fn empty_config_passes_through() {
        let r = runner("");
        assert!(r.is_empty());
        assert!(r.run_pre("exec", &json!({})).await.is_ok());
    }
}
