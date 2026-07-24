//! MCP client：initialize 握手 + tools/list + tools/call + resources/prompts 清单（协议版本 2024-11-05）。
//! 与传输解耦（Arc<dyn Transport>）：stdio / streamable http / legacy sse 走同一套协议机。

use super::config::{RemoteKind, ServerConfig};
use super::transport::{StdioTransport, Transport};
use serde_json::{Value, json};
use std::sync::Arc;

const PROTOCOL_VERSION: &str = "2024-11-05";
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// 伪工具 read_resource 描述里的资源清单上限：描述太长会白吃每轮 context。
const RESOURCE_LIST_CAP: usize = 20;

#[derive(Debug, Clone, serde::Serialize)]
pub struct McpTool {
    pub server: String,
    pub name: String,
    pub description: String,
    pub schema: Value,
    /// annotations.readOnlyHint；缺省 false = 视为写工具（restricted 角色过滤宁严勿宽）
    pub read_only: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResourceInfo {
    pub uri: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PromptInfo {
    pub name: String,
    pub description: String,
}

pub struct McpClient {
    transport: Arc<dyn Transport>,
    /// 注入了伪工具 read_resource（call 路由到 resources/read 而非 tools/call）
    read_resource_injected: bool,
    pub tools: Vec<McpTool>,
    pub resources: Vec<ResourceInfo>,
    pub prompts: Vec<PromptInfo>,
}

/// roots capability 的值形态：[{uri, name}]（roots/list 反向请求应答用）。
fn roots_value(roots: &[String]) -> Value {
    Value::Array(roots.iter().map(|r| json!({ "uri": format!("file://{r}"), "name": r })).collect())
}

impl McpClient {
    /// 生产建连：remote 一律过 net_guard（SSRF 守卫拦 loopback/内网/metadata）。
    pub async fn connect(server: &str, config: &ServerConfig, roots: &[String]) -> Result<Self, String> {
        Self::connect_inner(server, config, roots, super::remote::Guard::Enforced).await
    }

    /// 测试放行钩子：集成测试的 mock server 监听 127.0.0.1，必被生产守卫拦，只能旁路。
    pub async fn connect_bypassing_guard_for_test(server: &str, config: &ServerConfig, roots: &[String]) -> Result<Self, String> {
        Self::connect_inner(server, config, roots, super::remote::Guard::Bypassed).await
    }

    /// spawn/建连 + initialize + initialized + tools/list + resources/prompts 清单全握手。
    async fn connect_inner(server: &str, config: &ServerConfig, roots: &[String], guard: super::remote::Guard) -> Result<Self, String> {
        let roots = roots_value(roots);
        let transport: Arc<dyn Transport> = match config {
            ServerConfig::Stdio(c) => StdioTransport::spawn(&c.command, &c.args, &c.env, roots)?,
            ServerConfig::Remote(c) => {
                // config 显式配了 Authorization 就不挂 OAuth（显式配置优先，被拒只报失败）
                let explicit_auth = c.headers.keys().any(|k| k.eq_ignore_ascii_case("authorization"));
                let auth = if explicit_auth {
                    None
                } else {
                    super::oauth_store::BearerAuth::from_store(&c.name, &super::oauth_store::store_path(), guard)
                };
                match c.transport {
                    RemoteKind::Http => super::remote::StreamableHttpTransport::connect(&c.url, &c.headers, roots, guard, auth).await?,
                    RemoteKind::Sse => super::remote_sse::SseTransport::connect(&c.url, &c.headers, roots, guard, auth).await?,
                }
            }
        };
        // 子进程启动需要时间（npx 冷启动尤其长），initialize 独立放宽到 60s
        let init = transport
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    // roots capability：server 可反向 roots/list 问 workspace 根
                    "capabilities": { "roots": { "listChanged": false } },
                    "clientInfo": { "name": "kxen", "version": "0.1.0" },
                }),
                std::time::Duration::from_secs(60),
            )
            .await?;
        if init.get("error").is_some() {
            transport.close().await;
            return Err(format!("initialize rejected: {}", init["error"]));
        }
        transport.notify("notifications/initialized", json!({})).await?;
        let caps = init.pointer("/result/capabilities").cloned().unwrap_or(json!({}));

        // 按 server 声明的 capabilities 拉清单；未声明的请求会吃 -32601，不发
        let mut tools = Vec::new();
        if caps.get("tools").is_some() {
            match transport.request("tools/list", json!({}), REQUEST_TIMEOUT).await {
                Ok(listed) => tools = parse_tools(server, &listed),
                Err(e) => tracing::warn!(server, error = %e, "mcp tools/list failed"),
            }
        }
        let mut resources = Vec::new();
        if caps.get("resources").is_some() {
            match transport.request("resources/list", json!({}), REQUEST_TIMEOUT).await {
                Ok(listed) => resources = parse_resources(&listed),
                // 清单拉取失败只 warn：tools 已可用，不该整台记 down
                Err(e) => tracing::warn!(server, error = %e, "mcp resources/list failed"),
            }
        }
        let mut prompts = Vec::new();
        if caps.get("prompts").is_some() {
            match transport.request("prompts/list", json!({}), REQUEST_TIMEOUT).await {
                Ok(listed) => prompts = parse_prompts(&listed),
                Err(e) => tracing::warn!(server, error = %e, "mcp prompts/list failed"),
            }
        }
        let read_resource_injected = inject_read_resource(server, &mut tools, &resources);
        Ok(Self { transport, read_resource_injected, tools, resources, prompts })
    }

    pub fn transport_kind(&self) -> &'static str {
        self.transport.kind()
    }

    /// 关进程/连接（restart/替换前调用）。
    pub async fn shutdown(&self) {
        self.transport.close().await;
    }

    /// tools/call：result.content[] 拼文本（text 类型为主，其它类型 JSON 化）。
    /// 伪工具 read_resource 路由到 resources/read。
    pub async fn call(&self, tool: &str, args: &Value) -> Result<String, String> {
        if tool == "read_resource" && self.read_resource_injected {
            let uri = args.get("uri").and_then(|u| u.as_str()).ok_or("missing uri")?;
            return self.read_resource(uri).await;
        }
        let resp = self.transport.request("tools/call", json!({ "name": tool, "arguments": args }), REQUEST_TIMEOUT).await?;
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

    /// resources/read：文本直拼；blob（base64）只占位——解码进 prompt 会炸 context。
    async fn read_resource(&self, uri: &str) -> Result<String, String> {
        let resp = self.transport.request("resources/read", json!({ "uri": uri }), REQUEST_TIMEOUT).await?;
        if let Some(err) = resp.get("error") {
            return Err(format!("resources/read error: {}", err.get("message").and_then(|m| m.as_str()).unwrap_or("unknown")));
        }
        let text = resp
            .pointer("/result/contents")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|c| {
                        if let Some(t) = c.get("text").and_then(|t| t.as_str()) {
                            t.to_string()
                        } else if c.get("blob").is_some() {
                            "[binary resource content omitted]".to_string()
                        } else {
                            serde_json::to_string(c).unwrap_or_default()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        Ok(if text.is_empty() { "(empty resource)".into() } else { text })
    }
}

fn parse_tools(server: &str, listed: &Value) -> Vec<McpTool> {
    listed
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
                    read_only: t.pointer("/annotations/readOnlyHint").and_then(|v| v.as_bool()).unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_resources(listed: &Value) -> Vec<ResourceInfo> {
    listed
        .get("result")
        .and_then(|r| r.get("resources"))
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .map(|r| ResourceInfo {
                    uri: r.get("uri").and_then(|u| u.as_str()).unwrap_or_default().to_string(),
                    name: r.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string(),
                    description: r.get("description").and_then(|d| d.as_str()).unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_prompts(listed: &Value) -> Vec<PromptInfo> {
    listed
        .get("result")
        .and_then(|r| r.get("prompts"))
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .map(|p| PromptInfo {
                    name: p.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string(),
                    description: p.get("description").and_then(|d| d.as_str()).unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 声明 resources 的 server 注入伪工具 read_resource（read_only=true），资源清单进描述。
/// 与真工具同名时放弃注入：server 自己的实现优先，伪工具不抢名。
fn inject_read_resource(server: &str, tools: &mut Vec<McpTool>, resources: &[ResourceInfo]) -> bool {
    if resources.is_empty() || tools.iter().any(|t| t.name == "read_resource") {
        return false;
    }
    let mut desc = String::from("Read an MCP resource by uri. Available resources:\n");
    for r in resources.iter().take(RESOURCE_LIST_CAP) {
        desc.push_str(&format!("- {}", r.uri));
        if !r.name.is_empty() {
            desc.push_str(&format!(" ({})", r.name));
        }
        if !r.description.is_empty() {
            desc.push_str(&format!(": {}", r.description));
        }
        desc.push('\n');
    }
    tools.push(McpTool {
        server: server.to_string(),
        name: "read_resource".to_string(),
        description: desc,
        schema: json!({
            "type": "object",
            "properties": { "uri": { "type": "string" } },
            "required": ["uri"]
        }),
        read_only: true,
    });
    true
}
