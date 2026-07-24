//! legacy SSE transport（MCP 2024-11-05 旧式 remote 形态，兼容存量 server）：
//! GET 长连接收事件，首帧 endpoint 事件给出回 POST 地址；请求走 POST（202 Accepted），
//! 响应经 SSE 流按 id 路由回挂起的 oneshot（与 stdio 读循环同构）。

use futures::StreamExt;
use futures::future::BoxFuture;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::remote::Guard;
use super::transport::Transport;

const ENDPOINT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const POST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub struct SseTransport {
    client: reqwest::Client,
    post_url: reqwest::Url,
    headers: Vec<(String, String)>,
    pending: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>>,
    reader: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    next_id: AtomicU64,
}

impl SseTransport {
    /// 建连 = SSRF 守卫 + GET SSE 流 + 等 endpoint 事件给出 POST 地址。
    pub async fn connect(
        url: &str,
        headers: &HashMap<String, String>,
        roots: Value,
        guard: Guard,
    ) -> Result<Arc<Self>, String> {
        if guard == Guard::Enforced {
            crate::tools::net_guard::check_url(url).await?;
        }
        let base = reqwest::Url::parse(url).map_err(|e| format!("invalid mcp sse url: {e}"))?;
        let pairs = super::remote::validate_headers(headers)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| e.to_string())?;
        let mut req = client
            .get(url)
            .header(reqwest::header::ACCEPT, "text/event-stream");
        for (k, v) in &pairs {
            req = req.header(k, v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("mcp sse connect {url}: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("mcp sse connect http {}", resp.status()));
        }

        let pending: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (endpoint_tx, endpoint_rx) = tokio::sync::oneshot::channel::<reqwest::Url>();
        let reader = {
            let pending = pending.clone();
            let client = client.clone();
            let pairs = pairs.clone();
            tokio::spawn(read_loop(
                resp,
                base,
                pending,
                endpoint_tx,
                client,
                pairs,
                roots,
            ))
        };
        let post_url = match tokio::time::timeout(ENDPOINT_TIMEOUT, endpoint_rx).await {
            Ok(Ok(u)) => u,
            Ok(Err(_)) => return Err("mcp sse stream closed before endpoint event".into()),
            Err(_) => return Err("mcp sse endpoint event timed out".into()),
        };
        Ok(Arc::new(Self {
            client,
            post_url,
            headers: pairs,
            pending,
            reader: tokio::sync::Mutex::new(Some(reader)),
            next_id: AtomicU64::new(1),
        }))
    }

    fn decorate(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut req = req;
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        req
    }

    /// POST 一帧到 endpoint；2xx（规范为 202）即视为送达，响应经 SSE 流回来。
    async fn post(&self, frame: Value) -> Result<(), String> {
        let resp = self
            .decorate(self.client.post(self.post_url.clone()))
            .json(&frame)
            .send()
            .await
            .map_err(|e| format!("mcp sse post: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("mcp sse post http {}", resp.status()));
        }
        Ok(())
    }

    async fn request_inner(
        &self,
        method: &str,
        params: Value,
        timeout: std::time::Duration,
    ) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().expect("mcp pending").insert(id, tx);
        let frame = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if let Err(e) = self.post(frame).await {
            self.pending.lock().expect("mcp pending").remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(_)) => Err("mcp sse stream closed".into()),
            Err(_) => Err(format!("mcp request {method} timed out")),
        }
    }

    async fn notify_inner(&self, method: &str, params: Value) -> Result<(), String> {
        self.post(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
            .await
    }

    async fn close_inner(&self) {
        if let Some(task) = self.reader.lock().await.take() {
            task.abort();
        }
    }
}

/// SSE 读循环：endpoint 事件交出 POST 地址；message 事件按 id 路由或应答 server 反向请求。
async fn read_loop(
    resp: reqwest::Response,
    base: reqwest::Url,
    pending: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>>,
    endpoint_tx: tokio::sync::oneshot::Sender<reqwest::Url>,
    client: reqwest::Client,
    headers: Vec<(String, String)>,
    roots: Value,
) {
    let mut endpoint_tx = Some(endpoint_tx);
    let mut post_url: Option<reqwest::Url> = None;
    let mut parser = super::sse::SseParser::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else { break };
        for ev in parser.feed(&chunk) {
            match ev.event.as_deref().unwrap_or("message") {
                "endpoint" => {
                    // data 常为相对路径（/messages/?session_id=x），按 base join
                    if let Ok(u) = base.join(ev.data.trim()) {
                        post_url = Some(u.clone());
                        if let Some(tx) = endpoint_tx.take() {
                            let _ = tx.send(u);
                        }
                    }
                }
                _ => {
                    let Ok(v) = serde_json::from_str::<Value>(&ev.data) else {
                        continue;
                    };
                    if v.get("method").is_some() {
                        // server 反向请求（roots/list）：应答帧 POST 回 endpoint
                        if let (Some(rid), Some(url)) =
                            (v.get("id").and_then(|i| i.as_u64()), post_url.clone())
                        {
                            let answer = super::transport::answer_server_request(&v, rid, &roots);
                            let mut req = client.post(url).json(&answer);
                            for (k, val) in &headers {
                                req = req.header(k, val);
                            }
                            tokio::spawn(async move {
                                let _ = tokio::time::timeout(POST_TIMEOUT, req.send()).await;
                            });
                        }
                        continue;
                    }
                    if let Some(id) = v.get("id").and_then(|i| i.as_u64()) {
                        if let Some(tx) = pending.lock().expect("mcp pending").remove(&id) {
                            let _ = tx.send(v);
                        }
                    }
                }
            }
        }
    }
    // 流断：挂起请求全部按失败结束（调用方走 lazy restart）
    pending.lock().expect("mcp pending").clear();
}

impl Transport for SseTransport {
    fn request<'a>(
        &'a self,
        method: &'a str,
        params: Value,
        timeout: std::time::Duration,
    ) -> BoxFuture<'a, Result<Value, String>> {
        Box::pin(async move { self.request_inner(method, params, timeout).await })
    }

    fn notify<'a>(&'a self, method: &'a str, params: Value) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move { self.notify_inner(method, params).await })
    }

    fn close<'a>(&'a self) -> BoxFuture<'a, ()> {
        Box::pin(async move { self.close_inner().await })
    }

    fn kind(&self) -> &'static str {
        "sse"
    }
}
