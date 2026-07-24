//! streamable HTTP transport（MCP 2025-03-26 形态的常见实现）：单端点 POST JSON-RPC，
//! 响应可为 application/json（单帧）或 text/event-stream（SSE 帧流，读到本请求应答为止）。
//! 会话：server 下发 Mcp-Session-Id 后续请求回带；close 时按规范发 DELETE 结束会话。
//! 不做 GET 流（server 主动推送通道）：roots/list 等反向请求只在 POST 响应流内应答，
//! 仅依赖 GET 流推送的 server 暂不支持（当前罕见）。

use super::oauth;
use super::oauth_store::BearerAuth;
use super::transport::Transport;
use futures::StreamExt;
use futures::future::BoxFuture;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

// Guard 定义上移到 mcp 根（oauth 等 pub 接口要暴露它）；此处 re-export 保持既有路径可用。
pub use super::Guard;

enum PostOutcome {
    /// 202 Accepted：通知/应答帧无 body
    Accepted,
    /// json 单帧或 SSE 流读到的全部 JSON-RPC 消息
    Messages(Vec<Value>),
}

/// post_once 的拒绝形态：Auth 是 401/403（可 refresh 后重试一次），Other 不可自愈。
enum PostReject {
    Auth(u16),
    Other(String),
}

pub struct StreamableHttpTransport {
    client: reqwest::Client,
    url: String,
    headers: Vec<(String, String)>,
    /// OAuth Bearer 供应（config 显式配了 Authorization 时为 None：显式配置被拒只报失败，不回落）
    auth: Option<Arc<BearerAuth>>,
    /// config headers 显式带 Authorization（大小写不敏感）：401/403 不回落 OAuth
    explicit_auth: bool,
    session: Mutex<Option<String>>,
    roots: Value,
    next_id: AtomicU64,
}

impl StreamableHttpTransport {
    pub async fn connect(
        url: &str,
        headers: &HashMap<String, String>,
        roots: Value,
        guard: Guard,
        auth: Option<Arc<BearerAuth>>,
    ) -> Result<Arc<Self>, String> {
        if guard == Guard::Enforced {
            crate::tools::net_guard::check_url(url).await?;
        }
        let pairs = validate_headers(headers)?;
        let explicit_auth = headers.keys().any(|k| k.eq_ignore_ascii_case("authorization"));
        let client = reqwest::Client::builder()
            // 自动跟随重定向发生在 net_guard 之外；POST 307/308 语义复杂，统一报错让用户的配置指到最终地址
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(Self {
            client,
            url: url.to_string(),
            headers: pairs,
            auth,
            explicit_auth,
            session: Mutex::new(None),
            roots,
            next_id: AtomicU64::new(1),
        }))
    }

    fn decorate(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut req = req.header(reqwest::header::ACCEPT, "application/json, text/event-stream");
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        // Bearer 每请求现取：refresh 换 token 后下一帧立即生效
        if let Some(auth) = &self.auth {
            req = req.header(reqwest::header::AUTHORIZATION, auth.header_value());
        }
        if let Some(s) = self.session.lock().expect("mcp session").clone() {
            req = req.header("mcp-session-id", s);
        }
        req
    }

    /// 单发一帧并按 content-type 读尽响应；401/403 单独成 Auth 交给 post 决定 refresh 重试。
    async fn post_once(&self, frame: &Value) -> Result<PostOutcome, PostReject> {
        let resp = self
            .decorate(self.client.post(&self.url))
            .json(frame)
            .send()
            .await
            .map_err(|e| PostReject::Other(format!("mcp http post {}: {e}", self.url)))?;
        if let Some(sid) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            *self.session.lock().expect("mcp session") = Some(sid.to_string());
        }
        let status = resp.status();
        if status == reqwest::StatusCode::ACCEPTED {
            return Ok(PostOutcome::Accepted);
        }
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(PostReject::Auth(status.as_u16()));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let body: String = body.chars().take(200).collect();
            return Err(PostReject::Other(format!("mcp http {status}: {body}")));
        }
        let is_sse = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.contains("text/event-stream"));
        if !is_sse {
            let v = resp.json::<Value>().await.map_err(|e| PostReject::Other(format!("mcp http bad json: {e}")))?;
            return Ok(PostOutcome::Messages(vec![v]));
        }
        let mut parser = super::sse::SseParser::new();
        let mut messages = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| PostReject::Other(format!("mcp http stream: {e}")))?;
            for ev in parser.feed(&chunk) {
                if let Ok(v) = serde_json::from_str::<Value>(&ev.data) {
                    messages.push(v);
                }
            }
        }
        Ok(PostOutcome::Messages(messages))
    }

    /// 401/403 自愈链：显式 Authorization 被拒 -> 直接报失败不回落；有 OAuth token ->
    /// refresh 后整帧重试一次，refresh 被拒或重试仍 401/403 才抛 AUTH_REQUIRED 让上层标 needs_auth。
    async fn post(&self, frame: Value, timeout: std::time::Duration) -> Result<PostOutcome, String> {
        let work = async {
            let reject = match self.post_once(&frame).await {
                Ok(out) => return Ok(out),
                Err(PostReject::Other(e)) => return Err(e),
                Err(PostReject::Auth(code)) => code,
            };
            if self.explicit_auth {
                return Err(format!("mcp http {reject}: configured Authorization header rejected"));
            }
            let Some(auth) = &self.auth else {
                return Err(oauth::err_auth_required(&format!("mcp http {reject}")));
            };
            match auth.refresh().await {
                Ok(()) => match self.post_once(&frame).await {
                    Ok(out) => Ok(out),
                    Err(PostReject::Auth(code)) => Err(oauth::err_auth_required(&format!("mcp http {code} after token refresh"))),
                    Err(PostReject::Other(e)) => Err(e),
                },
                Err(e) => Err(oauth::err_auth_required(&format!("token refresh failed: {e}"))),
            }
        };
        match tokio::time::timeout(timeout, work).await {
            Ok(r) => r,
            Err(_) => Err("mcp http request timed out".into()),
        }
    }

    async fn request_inner(&self, method: &str, params: Value, timeout: std::time::Duration) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let frame = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        match self.post(frame, timeout).await? {
            PostOutcome::Accepted => Err(format!("mcp http request {method} got 202 without body")),
            PostOutcome::Messages(messages) => {
                for msg in messages {
                    // server 反向请求（method+id 同帧）：只懂 roots/list，答完继续等自己的响应
                    if msg.get("method").is_some() {
                        if let Some(rid) = msg.get("id").and_then(|i| i.as_u64()) {
                            let answer = super::transport::answer_server_request(&msg, rid, &self.roots);
                            // 应答帧走新 POST，server 侧期待 202；失败不阻断主请求
                            let _ = self.post(answer, std::time::Duration::from_secs(10)).await;
                        }
                        continue;
                    }
                    if msg.get("id").and_then(|i| i.as_u64()) == Some(id) {
                        return Ok(msg);
                    }
                }
                Err(format!("mcp http request {method} got no matching response"))
            }
        }
    }

    async fn notify_inner(&self, method: &str, params: Value) -> Result<(), String> {
        let frame = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.post(frame, std::time::Duration::from_secs(30)).await.map(|_| ())
    }

    async fn close_inner(&self) {
        // 按规范：client 不再需要会话时发 DELETE；无会话或失败都静默（shutdown 路径不可失败）
        let Some(session) = self.session.lock().expect("mcp session").take() else {
            return;
        };
        let req = self.client.delete(&self.url).header("mcp-session-id", session);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), self.decorate(req).send()).await;
    }
}

impl Transport for StreamableHttpTransport {
    fn request<'a>(&'a self, method: &'a str, params: Value, timeout: std::time::Duration) -> BoxFuture<'a, Result<Value, String>> {
        Box::pin(async move { self.request_inner(method, params, timeout).await })
    }

    fn notify<'a>(&'a self, method: &'a str, params: Value) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move { self.notify_inner(method, params).await })
    }

    fn close<'a>(&'a self) -> BoxFuture<'a, ()> {
        Box::pin(async move { self.close_inner().await })
    }

    fn kind(&self) -> &'static str {
        "http"
    }
}

/// config 的 headers 表校验为可下发形态；非法 header 名/值在建连前报错，比请求时才炸好定位。
/// pub(crate)：streamable http 与 legacy sse 共用。
pub(crate) fn validate_headers(headers: &HashMap<String, String>) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for (k, v) in headers {
        reqwest::header::HeaderName::from_bytes(k.as_bytes()).map_err(|e| format!("invalid mcp header name {k}: {e}"))?;
        reqwest::header::HeaderValue::from_str(v).map_err(|e| format!("invalid mcp header value for {k}: {e}"))?;
        out.push((k.clone(), v.clone()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_header_pairs() {
        let mut ok = HashMap::new();
        ok.insert("Authorization".to_string(), "Bearer t".to_string());
        assert_eq!(validate_headers(&ok).unwrap().len(), 1);
        let mut bad = HashMap::new();
        bad.insert("bad\nname".to_string(), "v".to_string());
        assert!(validate_headers(&bad).is_err(), "换行注入必须拒绝");
        let mut bad_v = HashMap::new();
        bad_v.insert("X".to_string(), "v\r\nEvil: 1".to_string());
        assert!(validate_headers(&bad_v).is_err(), "值内 CRLF 注入必须拒绝");
    }
}
