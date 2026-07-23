//! MCP client：initialize 握手 + tools/list + tools/call（协议版本 2024-11-05）。

use super::transport::StdioTransport;
use serde_json::{json, Value};
use std::sync::Arc;

const PROTOCOL_VERSION: &str = "2024-11-05";
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone, serde::Serialize)]
pub struct McpTool {
    pub server: String,
    pub name: String,
    pub description: String,
    pub schema: Value,
}

pub struct McpClient {
    transport: Arc<StdioTransport>,
    pub tools: Vec<McpTool>,
}

impl McpClient {
    /// spawn + initialize + initialized + tools/list 全握手。
    pub async fn connect(server: &str, config: &super::config::ServerConfig) -> Result<Self, String> {
        let transport = StdioTransport::spawn(&config.command, &config.args, &config.env)?;
        // 子进程启动需要时间（npx 冷启动尤其长），initialize 独立放宽到 60s
        let init = transport
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "kxen", "version": "0.1.0" },
                }),
                std::time::Duration::from_secs(60),
            )
            .await?;
        if init.get("error").is_some() {
            transport.kill().await;
            return Err(format!("initialize rejected: {}", init["error"]));
        }
        transport.notify("notifications/initialized", json!({})).await?;
        let listed = transport.request("tools/list", json!({}), REQUEST_TIMEOUT).await?;
        let tools = listed
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|t| McpTool {
                        server: server.to_string(),
                        name: t.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string(),
                        description: t.get("description").and_then(|d| d.as_str()).unwrap_or_default().to_string(),
                        schema: t.get("inputSchema").cloned().unwrap_or_else(|| json!({ "type": "object" })),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self { transport, tools })
    }

    /// 关进程（restart/替换前调用）。
    pub async fn shutdown(&self) {
        self.transport.kill().await;
    }

    /// tools/call：result.content[] 拼文本（text 类型为主，其它类型 JSON 化）。
    pub async fn call(&self, tool: &str, args: &Value) -> Result<String, String> {
        let resp = self
            .transport
            .request("tools/call", json!({ "name": tool, "arguments": args }), REQUEST_TIMEOUT)
            .await?;
        if let Some(err) = resp.get("error") {
            return Err(format!("tools/call error: {}", err.get("message").and_then(|m| m.as_str()).unwrap_or("unknown")));
        }
        let content = resp
            .get("result")
            .and_then(|r| r.get("content"))
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|c| match c.get("type").and_then(|t| t.as_str()) {
                        Some("text") => c.get("text").and_then(|t| t.as_str()).unwrap_or_default().to_string(),
                        _ => serde_json::to_string(c).unwrap_or_default(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        Ok(if content.is_empty() { "(empty result)".into() } else { content })
    }
}
