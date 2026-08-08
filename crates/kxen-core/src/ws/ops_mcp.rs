//! MCP 域 RPC：status / restart / auth（OAuth 交互授权流编排）。

use serde_json::{Value, json};
use std::sync::Arc;

use crate::AppState;

pub(super) const METHODS: &[&str] = &["mcp.status", "mcp.restart", "mcp.auth"];

pub(super) async fn handle(method: &str, params: &Value, state: &Arc<AppState>) -> Result<Value, String> {
    match method {
        "mcp.status" => {
            let runtime = state.ready_active_runtime().await?;
            Ok(serde_json::to_value(runtime.mcp().status()).map_err(|e| e.to_string())?)
        }
        "mcp.restart" => {
            let name = params.get("name").and_then(Value::as_str).ok_or("missing name")?;
            state.ready_active_runtime().await?.mcp().restart(name).await?;
            Ok(json!({ "restarted": true }))
        }
        "mcp.auth" => mcp_auth(params, state).await,
        other => Err(format!("unknown method: {other}")),
    }
}

/// 交互授权：begin 同步返回授权 URL（前端 RPC 30s 超时，回调等待最长 300s 必须挪后台）；
/// 后台等回调换 token 落盘后 restart 生效，成败经通知中心告知。
async fn mcp_auth(params: &Value, state: &Arc<AppState>) -> Result<Value, String> {
    let name = params.get("name").and_then(Value::as_str).ok_or("missing name")?;
    let mcp = state.ready_active_runtime().await?.mcp();
    let session = mcp.begin_auth(name).await?;
    mcp.set_auth_error(name, None); // 新一次发起：清掉上一轮的失败原因
    let url = session.authorize_url.clone();
    let opened = open_browser(&url);
    if !opened {
        tracing::warn!("浏览器打开失败，授权 URL 交前端展示供手动复制");
    }
    let bus = state.bus.clone();
    let server = name.to_string();
    tokio::spawn(async move {
        match mcp.finish_auth(&session).await {
            Ok(()) => {
                mcp.set_auth_error(&server, None);
                let note = match mcp.restart(&server).await {
                    Ok(()) => format!("MCP server {server} 认证完成，已重连"),
                    Err(e) => format!("MCP server {server} 认证完成，但重连失败: {e}"),
                };
                bus.publish(kxen_core::core::event::Event::notify(note, None));
            }
            Err(e) => {
                mcp.set_auth_error(&server, Some(e.clone()));
                bus.publish(kxen_core::core::event::Event::notify(format!("MCP server {server} 认证失败: {e}"), None));
            }
        }
    });
    Ok(json!({ "authorize_url": url, "opened": opened }))
}

/// 开浏览器：桌面 GUI 平台用各自系统 opener；失败或无 GUI（kxen 无头服务器）返回 false，
/// 调用方把 URL 给前端展示（web 模式前端本就在浏览器，直接展示/复制链接即可）。
pub(super) fn open_browser(url: &str) -> bool {
    if !is_web_url(url) {
        return false;
    }
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    // URL 来自可能恶意的 MCP server，经 cmd /C start 会被 cmd 解释其中的 & 等字符拼出额外命令；
    // explorer.exe 把 URL 当单个参数接收、不经 shell，直接打开默认浏览器
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer.exe").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    result.is_ok()
}

/// opener 只接 http/https：授权 URL 上游（mcp::oauth::authorize_url -> validate_secure_endpoint）
/// 已强制 https（仅 loopback 放行 http），此处 fail-closed 再挡一层，防止 javascript:/file:
/// 等伪协议经系统 opener 触发本地动作。
fn is_web_url(url: &str) -> bool {
    reqwest::Url::parse(url).map(|u| matches!(u.scheme(), "http" | "https")).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_url_accepts_https_and_loopback_http() {
        assert!(is_web_url("https://auth.example.test/authorize?client_id=x&state=%22y%22"));
        assert!(is_web_url("http://localhost:3000/callback?state=abc"));
    }

    #[test]
    fn web_url_rejects_non_web_schemes_and_garbage() {
        for url in ["javascript:alert(1)", "file:///C:/Windows/system32/calc.exe", "not a url", ""] {
            assert!(!is_web_url(url), "{url}");
        }
    }
}
