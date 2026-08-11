//! 无 tauri 的 WebServer 端到端回环测试。
//! AppState 依赖 data_dir 与单实例锁；env 是进程全局，故 fork 子进程并覆盖 HOME/KXEN_DATA_DIR 隔离（与 ws 层同规约，父进程并行测试不能写）。

use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use kxen_core::web::WebServer;

const CHILD_ENV: &str = "KXEN_WEB_LOOP_CHILD";

#[test]
fn ws_end_to_end_in_isolated_child() {
    if std::env::var_os(CHILD_ENV).is_none() {
        let home = std::env::temp_dir().join(format!("kxen-loop-{}", std::process::id()));
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "ws_end_to_end_in_isolated_child"])
            .env(CHILD_ENV, "1")
            .env("HOME", &home)
            .env("KXEN_DATA_DIR", home.join("data"))
            .status()
            .unwrap();
        assert!(status.success());
        std::fs::remove_dir_all(home).ok();
        return;
    }
    tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(scenario());
}

async fn scenario() {
    let state = Arc::new(kxen_core::AppState::new().unwrap());
    let token = state.ws_token.clone();
    let loopback = std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
    let handle = WebServer::start((loopback, 0), state.clone(), true, vec!["myhost.tailnet".to_string()]).unwrap();
    let port = handle.port();

    let url = format!("ws://127.0.0.1:{port}/ws?token={token}");
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    socket
        .send(tokio_tungstenite::tungstenite::Message::Text(r#"{"jsonrpc":"3.0","id":1,"method":"rpc.heartbeat"}"#.into()))
        .await
        .unwrap();
    let reply = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next()).await.unwrap().unwrap().unwrap();
    let value: serde_json::Value = serde_json::from_str(reply.into_text().unwrap().as_str()).unwrap();
    assert_eq!(value["id"], 1);
    assert_eq!(value["result"]["alive"], true, "heartbeat 应回 alive: {value}");
    drop(socket);

    let error = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws?token=wrong")).await.unwrap_err();
    match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => assert_eq!(response.status().as_u16(), 403),
        other => panic!("错误 token 应被 HTTP 403 拒绝: {other}"),
    }

    assert_eq!(raw_upgrade_status(port, &token, "evil.com:1").await, 403);
    assert_eq!(raw_upgrade_status(port, &token, &format!("myhost.tailnet:{port}")).await, 101);

    // 静态层不校验 token（由前端从 URL 读取）
    assert_eq!(raw_get_status(port, "/").await, 200);

    handle.shutdown();
}

/// 裸 TCP 发 upgrade，以便控制 Host 头并读回 HTTP 状态码。
async fn raw_upgrade_status(port: u16, token: &str, host: &str) -> u16 {
    let request = format!(
        "GET /ws?token={token} HTTP/1.1\r\nHost: {host}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    raw_http_status(port, &request).await
}

async fn raw_get_status(port: u16, path: &str) -> u16 {
    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    raw_http_status(port, &request).await
}

async fn raw_http_status(port: u16, request: &str) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mut chunk = [0u8; 1024];
        while !buf.windows(2).any(|pair| pair == b"\r\n") {
            let read = stream.read(&mut chunk).await.unwrap();
            assert!(read > 0, "连接在读状态行之前关闭");
            buf.extend_from_slice(&chunk[..read]);
        }
    })
    .await
    .unwrap();
    let line = String::from_utf8_lossy(&buf).lines().next().unwrap().to_string();
    line.split_whitespace().nth(1).unwrap_or_else(|| panic!("状态行缺状态码: {line:?}")).parse().unwrap()
}
