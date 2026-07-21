//! 内嵌 WebSocket 双通道（前端 <-> Rust）。
//! - /rpc：请求-响应（id 关联，支持并发调用）
//! - /stream：订阅-推送（topic 过滤，server 主动推）
//! 端口启动时随机分配，经 window eval 注入前端。

mod llm_task;
mod rpc;
mod settings;
mod stream;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::net::{TcpListener, TcpStream};

// ---------------- 协议 ----------------

#[derive(Debug, Deserialize)]
struct RpcRequest {
    id: String,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum StreamCtl {
    Subscribe { topics: Vec<String> },
    Unsubscribe { topics: Vec<String> },
}

#[derive(Debug, Clone, Serialize)]
struct StreamPush {
    topic: String,
    payload: Value,
}

// ---------------- 启动 ----------------

pub async fn serve(app: AppHandle) -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(dispatch(stream, app.clone()));
        }
    });
    Ok(port)
}

async fn dispatch(stream: TcpStream, app: AppHandle) {
    use std::sync::Mutex as StdMutex;
    use tokio_tungstenite::tungstenite::handshake::server::{Callback, ErrorResponse, Request, Response};

    struct PathCb(Arc<StdMutex<String>>);
    impl Callback for PathCb {
        fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
            if let Ok(mut path) = self.0.lock() {
                *path = request.uri().path().to_string();
            }
            Ok(response)
        }
    }

    let path = Arc::new(StdMutex::new(String::new()));
    let Ok(ws) = tokio_tungstenite::accept_hdr_async(stream, PathCb(path.clone())).await else {
        return;
    };
    let route = path.lock().map(|p| p.clone()).unwrap_or_default();
    match route.as_str() {
        "/rpc" => rpc::handle_rpc(ws, app).await,
        _ => stream::handle_stream(ws, app).await,
    }
}
