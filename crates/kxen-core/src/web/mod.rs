//! 单一 HTTP 端点（axum）：GET /ws（WebSocket 升级）+ dist 静态托管（SPA 回退）。
//! 桌面 webview 与外部浏览器是同一个内嵌服务的两个平等客户端；
//! 握手检查（token/Origin/Host）在 upgrade 之前完成，协议帧经单 adapter 映射进 ws 连接核心。

mod guard;
mod static_files;

use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{RawQuery, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures::{Sink, SinkExt, StreamExt};
use tokio::sync::oneshot;

use crate::AppState;
use crate::ws::Frame;

#[derive(Clone)]
struct WebContext {
    state: Arc<AppState>,
    static_enabled: Arc<AtomicBool>,
    /// 实际 bind IP（Host 白名单一员；桌面恒 127.0.0.1，kxen 可对外）
    bind_host: Arc<str>,
    /// Host 白名单追加项（kxen `--allow-host`；桌面恒空）
    extra_hosts: Arc<[String]>,
}

/// 运行中的 Web 服务句柄：端口回读、静态托管开关、graceful shutdown。
pub struct WebServerHandle {
    port: u16,
    static_enabled: Arc<AtomicBool>,
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
}

impl WebServerHandle {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn set_static_enabled(&self, enabled: bool) {
        self.static_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn shutdown(&self) {
        if let Ok(mut guard) = self.shutdown.lock()
            && let Some(tx) = guard.take()
        {
            let _ = tx.send(());
        }
    }
}

pub struct WebServer;

/// Encode the browser access credential as a standards-compliant query.
/// Keep this shared with the handshake parser so custom tokens round-trip.
pub fn token_query(token: &str) -> String {
    form_urlencoded::Serializer::new(String::new()).append_pair("token", token).finish()
}

impl WebServer {
    /// 同步 bind 后立即返回句柄；serve 循环在 tokio 任务内驱动。
    /// 端口占用回退由调用方以 port 0 重试表达（桌面：7824 优先，回退随机）。
    /// `extra_hosts` 追加进 Host 白名单（kxen `--allow-host`；桌面传空）。
    pub fn start(
        bind: (IpAddr, u16),
        state: Arc<AppState>,
        static_enabled: bool,
        extra_hosts: Vec<String>,
    ) -> std::io::Result<WebServerHandle> {
        let (ip, port) = bind;
        let std_listener = std::net::TcpListener::bind(SocketAddr::new(ip, port))?;
        let addr = std_listener.local_addr()?;
        std_listener.set_nonblocking(true)?;
        let static_flag = Arc::new(AtomicBool::new(static_enabled));
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let ctx = WebContext {
            state,
            static_enabled: static_flag.clone(),
            bind_host: Arc::from(ip.to_string()),
            extra_hosts: Arc::from(extra_hosts),
        };
        let app = router(ctx);
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener)?;
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await
        });
        Ok(WebServerHandle { port: addr.port(), static_enabled: static_flag, shutdown: Mutex::new(Some(shutdown_tx)) })
    }
}

fn router(ctx: WebContext) -> Router {
    Router::new().route("/ws", get(ws_handshake)).fallback(get(static_files::serve)).with_state(ctx)
}

/// GET /ws：先过握手检查（403），再 upgrade；Message <-> Frame 在此单层 adapter 完成。
async fn ws_handshake(State(ctx): State<WebContext>, headers: HeaderMap, RawQuery(query): RawQuery, ws: WebSocketUpgrade) -> Response {
    let host = headers.get("host").and_then(|value| value.to_str().ok());
    let origin = headers.get("origin").and_then(|value| value.to_str().ok());
    if !guard::handshake_allowed(query.as_deref(), origin, host, &ctx.state.ws_token, &ctx.bind_host, &ctx.extra_hosts) {
        return (StatusCode::FORBIDDEN, "ws handshake rejected: bad token, origin or host").into_response();
    }
    let state = ctx.state;
    ws.on_upgrade(move |socket| drive(socket, state))
}

async fn drive(socket: WebSocket, state: Arc<AppState>) {
    let (sink, source) = socket.split();
    // 入向：读错误或 Close 都收敛为流结束（连接核心按断开处理，语义与旧裸 listener 一致）
    let source = source
        .map(|result| match result {
            Ok(Message::Text(text)) => Some(Frame::Text(text.as_str().as_bytes().to_vec())),
            Ok(Message::Binary(bytes)) => Some(Frame::Binary(bytes.to_vec())),
            Ok(Message::Ping(payload)) => Some(Frame::Ping(payload.to_vec())),
            Ok(Message::Pong(payload)) => Some(Frame::Pong(payload.to_vec())),
            Ok(Message::Close(_)) | Err(_) => None,
        })
        .take_while(|frame| std::future::ready(frame.is_some()))
        .map(|frame| frame.expect("take_while 只放行 Some"));
    let sink = axum_sink(sink);
    crate::ws::connection::handle(source, sink, state).await;
}

/// 出向 adapter：核心文本帧恒为 JSON（合法 UTF-8），非法时 lossy 兜底（不静默断连）。
/// Box::pin 收口：`SinkExt::with` 的 async 闭包不满足 Unpin，钉装后统一成 Unpin 的 trait object。
fn axum_sink<S>(sink: S) -> std::pin::Pin<Box<dyn Sink<Frame, Error = std::io::Error> + Send>>
where
    S: Sink<Message, Error = axum::Error> + Send + 'static,
{
    Box::pin(sink.sink_map_err(|error| std::io::Error::other(error.to_string())).with(|frame| async move {
        Ok::<Message, std::io::Error>(match frame {
            Frame::Text(bytes) => Message::Text(match String::from_utf8(bytes) {
                Ok(text) => text.into(),
                Err(error) => String::from_utf8_lossy(error.as_bytes()).into_owned().into(),
            }),
            Frame::Binary(bytes) => Message::Binary(bytes.into()),
            Frame::Ping(payload) => Message::Ping(payload.into()),
            Frame::Pong(payload) => Message::Pong(payload.into()),
            Frame::Close => Message::Close(None),
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// axum Error -> io::Error 的 sink 适配满足连接核心的类型约束（Unpin + Send）。
    #[test]
    fn adapter_sink_is_unpin_and_send() {
        fn require<T: Sink<Frame, Error = std::io::Error> + Unpin + Send>(_value: &T) {}
        let (tx, _rx) = futures::channel::mpsc::channel::<Message>(1);
        let sink = tx.sink_map_err(axum::Error::new);
        require(&axum_sink(sink));
    }
}
