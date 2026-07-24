//! McpManager：server 生命周期（start/status/call/reload/restart）+ 工具缓存 + per-tool 策略门。
//! 崩溃 lazy 重启：call 失败标记 down，下次调用前重连（简单重试，无后台 watchdog）。
//! TODO(oauth): remote server 的 OAuth 授权流未实现（静态 headers 已可用，见 config.rs RemoteConfig）。

pub mod client;
pub mod config;
mod remote;
mod remote_sse;
mod sse;
mod transport;
pub mod tools;

use self::client::{McpClient, McpTool};
use self::config::{PolicySet, ServerConfig, ToolPolicy};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// MCP 工具输出上限（字符）：单条 tool result 不许吃爆 context。
const OUTPUT_CAP: usize = 50_000;

/// 输出截断：按 chars 计数防切半 UTF-8；超了加 truncated 标记让模型知道没看全。
fn cap_output(s: &str) -> String {
    let total = s.chars().count();
    if total <= OUTPUT_CAP {
        return s.to_string();
    }
    let kept: String = s.chars().take(OUTPUT_CAP).collect();
    format!("{kept}\n... (truncated, {total} chars total)")
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServerStatus {
    pub name: String,
    /// "running" | "down"
    pub status: String,
    /// "stdio" | "http" | "sse"
    pub transport: String,
    /// remote server 的 URL；stdio 为 None
    pub url: Option<String>,
    pub tools: usize,
    pub resources: usize,
    /// prompt 名称列表（设置页直接展示）
    pub prompts: Vec<String>,
}

struct Entry {
    config: ServerConfig,
    client: Option<Arc<McpClient>>,
}

pub struct McpManager {
    servers: Mutex<HashMap<String, Entry>>,
    /// per-tool 策略表：随 reload 整批更换（读多写少，Mutex 足够）
    policies: Mutex<PolicySet>,
    /// workspace roots：roots/list 反向请求应答 + 重连握手用
    roots: Mutex<Vec<String>>,
    /// reload 串行化：快速连续 switch 若交错 drain/start，被挤掉的 client 无人 shutdown 会泄漏
    reload_lock: tokio::sync::Mutex<()>,
}

impl McpManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            servers: Mutex::new(HashMap::new()),
            policies: Mutex::new(PolicySet::default()),
            roots: Mutex::new(Vec::new()),
            reload_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// 配置驱动启动（无策略/roots 的简口，测试与旧调用方用）。
    pub async fn start(&self, configs: Vec<ServerConfig>) {
        self.reload(configs, PolicySet::default(), Vec::new()).await;
    }

    /// 整批换：drain 旧 server 先 shutdown（进程/连接不泄漏），再按新配置重建。
    /// 永不返回错误：单台失败记 down，状态面板可见，不拖垮整批。
    pub async fn reload(&self, configs: Vec<ServerConfig>, policies: PolicySet, roots: Vec<String>) {
        let _guard = self.reload_lock.lock().await;
        *self.policies.lock().expect("mcp") = policies;
        *self.roots.lock().expect("mcp") = roots;
        let old: Vec<Entry> = std::mem::take(&mut *self.servers.lock().expect("mcp"))
            .into_values()
            .collect();
        for entry in old {
            if let Some(c) = entry.client {
                c.shutdown().await;
            }
        }
        for config in configs {
            let name = config.name().to_string();
            self.servers
                .lock()
                .expect("mcp")
                .insert(name.clone(), Entry { config: config.clone(), client: None });
            let roots = self.roots.lock().expect("mcp").clone();
            match McpClient::connect(&name, &config, &roots).await {
                Ok(client) => {
                    tracing::info!(server = name, tools = client.tools.len(), "mcp server connected");
                    self.servers.lock().expect("mcp").get_mut(&name).map(|e| e.client = Some(Arc::new(client)));
                }
                Err(e) => tracing::warn!(server = name, error = %e, "mcp server connect failed"),
            }
        }
    }

    pub fn status(&self) -> Vec<ServerStatus> {
        self.servers
            .lock()
            .expect("mcp")
            .values()
            .map(|e| ServerStatus {
                name: e.config.name().to_string(),
                status: if e.client.is_some() { "running" } else { "down" }.into(),
                transport: e.config.transport_kind().to_string(),
                url: e.config.url().map(str::to_string),
                tools: e.client.as_ref().map(|c| c.tools.len()).unwrap_or(0),
                resources: e.client.as_ref().map(|c| c.resources.len()).unwrap_or(0),
                prompts: e
                    .client
                    .as_ref()
                    .map(|c| c.prompts.iter().map(|p| p.name.clone()).collect())
                    .unwrap_or_default(),
            })
            .collect()
    }

    pub fn all_tools(&self) -> Vec<McpTool> {
        self.servers
            .lock()
            .expect("mcp")
            .values()
            .filter_map(|e| e.client.as_ref().map(|c| c.tools.clone()))
            .flatten()
            .collect()
    }

    pub fn policy_for(&self, server: &str, tool: &str) -> ToolPolicy {
        self.policies.lock().expect("mcp").for_tool(server, tool)
    }

    /// 工具调用：down 的先 lazy 重启一次；仍失败原样报错。返回路径过 50K cap。
    pub async fn call(&self, server: &str, tool: &str, args: &Value) -> Result<String, String> {
        let client = self.client_or_restart(server).await?;
        client.call(tool, args).await.map(|out| cap_output(&out))
    }

    /// 策略门调用：prefixed = mcp__server__tool。
    /// deny 先于 server 存在性检查即拒；ask 走审批（无通道 fail-closed）；allow 直跑原 call。
    pub async fn call_gated(
        &self,
        prefixed: &str,
        args: &Value,
        approval: Option<&crate::tools::exec::ApprovalCtx<'_>>,
    ) -> Result<String, String> {
        let (server, tool) = tools::split_prefixed(prefixed)
            .ok_or_else(|| format!("invalid mcp tool name: {prefixed}"))?;
        match self.policy_for(server, tool) {
            ToolPolicy::Deny => Err(format!("mcp tool {prefixed} denied by toolPolicies")),
            ToolPolicy::Allow => self.call(server, tool, args).await,
            ToolPolicy::Ask => {
                // fail-closed：无审批通道一律拒，不静默放行
                let Some(appr) = approval else {
                    return Err(format!(
                        "mcp tool {prefixed} needs approval（当前上下文无审批通道，按拒绝处理）"
                    ));
                };
                let reason = format!("MCP 工具 {prefixed} 需要确认（toolPolicies: ask）");
                match crate::agent::approval::request_approval(appr, prefixed, &reason).await {
                    crate::agent::approval::ApprovalOutcome::Allow => self.call(server, tool, args).await,
                    crate::agent::approval::ApprovalOutcome::Timeout => {
                        Err(format!("mcp tool {prefixed} 审批超时未响应"))
                    }
                    crate::agent::approval::ApprovalOutcome::Deny => {
                        Err(format!("mcp tool {prefixed} 已被用户拒绝或中断"))
                    }
                }
            }
        }
    }

    async fn client_or_restart(&self, server: &str) -> Result<Arc<McpClient>, String> {
        let entry = self.servers.lock().expect("mcp").get(server).map(|e| (e.config.clone(), e.client.clone()));
        let Some((config, client)) = entry else {
            return Err(format!("mcp server not found: {server}"));
        };
        if let Some(c) = client {
            return Ok(c);
        }
        let roots = self.roots.lock().expect("mcp").clone();
        let client = McpClient::connect(server, &config, &roots).await?;
        let client = Arc::new(client);
        self.servers.lock().expect("mcp").get_mut(server).map(|e| e.client = Some(client.clone()));
        Ok(client)
    }

    /// 手动重启（设置页按钮）。
    pub async fn restart(&self, server: &str) -> Result<(), String> {
        let old = self.servers.lock().expect("mcp").get_mut(server).and_then(|e| e.client.take());
        if let Some(c) = old {
            c.shutdown().await;
        }
        let client = self.client_or_restart(server).await?;
        drop(client);
        Ok(())
    }
}

/// 启动与 workspace switch 共用的重载入口：信任门 + 双 scope 加载 + 整批换。
/// roots 取 workdir：roots/list 反向请求答的就是当前 workspace 根。
pub async fn reload_for_workspace(workdir: &std::path::Path, mcp: &Arc<McpManager>) {
    let trusted = crate::core::trust::is_trusted(workdir);
    let (configs, policies) = config::load(workdir, trusted);
    let roots = vec![workdir.to_string_lossy().into_owned()];
    mcp.reload(configs, policies, roots).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_output_truncates_without_splitting_utf8() {
        let short = "abc汉字";
        assert_eq!(cap_output(short), short);
        let long: String = "汉".repeat(OUTPUT_CAP + 10);
        let capped = cap_output(&long);
        assert!(capped.contains("truncated"), "截断必须带标记");
        assert!(capped.chars().count() > OUTPUT_CAP, "标记本身在 cap 之外");
        assert!(!capped.contains('\u{fffd}'), "不得出半个 UTF-8 的替换符");
    }

    #[tokio::test]
    async fn reload_is_serialized() {
        let m = McpManager::new();
        let guard = m.reload_lock.lock().await;
        let m2 = m.clone();
        let pending = tokio::spawn(async move { m2.reload(vec![], PolicySet::default(), vec![]).await });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!pending.is_finished(), "持锁期间并发 reload 不得进入执行");
        drop(guard);
        pending.await.expect("锁释放后 reload 必须完成");
    }
}
