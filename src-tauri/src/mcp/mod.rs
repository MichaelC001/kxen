//! McpManager：server 生命周期（start/status/call/restart）+ 工具缓存。
//! 崩溃 lazy 重启：call 失败标记 down，下次调用前重连（简单重试，无后台 watchdog）。

pub mod client;
pub mod config;
mod transport;
pub mod tools;

use self::client::{McpClient, McpTool};
use self::config::ServerConfig;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServerStatus {
    pub name: String,
    /// "running" | "down" | "starting"
    pub status: String,
    pub tools: usize,
}

struct Entry {
    config: ServerConfig,
    client: Option<Arc<McpClient>>,
}

pub struct McpManager {
    servers: Mutex<HashMap<String, Entry>>,
}

impl McpManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { servers: Mutex::new(HashMap::new()) })
    }

    /// 配置驱动启动：逐 server 连接（失败记 down 不阻断其它）。
    pub async fn start(&self, configs: Vec<ServerConfig>) {
        for config in configs {
            let name = config.name.clone();
            self.servers.lock().expect("mcp").insert(name.clone(), Entry { config: config.clone(), client: None });
            match McpClient::connect(&name, &config).await {
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
                name: e.config.name.clone(),
                status: if e.client.is_some() { "running" } else { "down" }.into(),
                tools: e.client.as_ref().map(|c| c.tools.len()).unwrap_or(0),
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

    /// 工具调用：down 的先 lazy 重启一次；仍失败原样报错。
    pub async fn call(&self, server: &str, tool: &str, args: &Value) -> Result<String, String> {
        let client = self.client_or_restart(server).await?;
        client.call(tool, args).await
    }

    async fn client_or_restart(&self, server: &str) -> Result<Arc<McpClient>, String> {
        let entry = self.servers.lock().expect("mcp").get(server).map(|e| (e.config.clone(), e.client.clone()));
        let Some((config, client)) = entry else {
            return Err(format!("mcp server not found: {server}"));
        };
        if let Some(c) = client {
            return Ok(c);
        }
        let client = McpClient::connect(server, &config).await?;
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
