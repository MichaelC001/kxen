use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;

/// 可复用 mock server：按请求行路由，响应直到 listener 随线程结束。
fn mock_server(respond: impl Fn(&str) -> (u16, String) + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            let (status, body) = respond(&request);
            let response = format!(
                "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len(),
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn discover_project_accepts_string_form() {
    let base = mock_server(|_| (200, r#"{"cloudaicompanionProject":"proj-string","currentTier":{"id":"free"}}"#.to_string()));
    let project =
        discover_project(&reqwest::Client::new(), &base, "token-string-form", crate::llm::gemini::Flavor::GeminiCli).await.unwrap();
    assert_eq!(project, "proj-string");
}

#[tokio::test]
async fn discover_project_accepts_object_form() {
    let base = mock_server(|_| (200, r#"{"cloudaicompanionProject":{"id":"proj-object"}}"#.to_string()));
    let project =
        discover_project(&reqwest::Client::new(), &base, "token-object-form", crate::llm::gemini::Flavor::GeminiCli).await.unwrap();
    assert_eq!(project, "proj-object");
}

#[tokio::test]
async fn discover_project_onboards_and_polls_lro() {
    let base = mock_server(|request| {
        let line = request.lines().next().unwrap_or("");
        if line.starts_with("POST /v1internal:loadCodeAssist") {
            (200, r#"{"allowedTiers":[{"id":"free-tier","isDefault":true}]}"#.to_string())
        } else if line.starts_with("POST /v1internal:onboardUser") {
            (200, r#"{"name":"operations/op1","done":false}"#.to_string())
        } else if line.starts_with("GET /v1internal/operations/op1") {
            (200, r#"{"name":"operations/op1","done":true,"response":{"cloudaicompanionProject":{"id":"proj-onboarded"}}}"#.to_string())
        } else {
            (404, format!("unexpected request: {line}"))
        }
    });
    let project =
        discover_project(&reqwest::Client::new(), &base, "token-onboard-flow", crate::llm::gemini::Flavor::GeminiCli).await.unwrap();
    assert_eq!(project, "proj-onboarded");
    // 内存缓存：第二次调用命中缓存（mock 即使 404 也返回缓存值）
    let cached =
        discover_project(&reqwest::Client::new(), &base, "token-onboard-flow", crate::llm::gemini::Flavor::GeminiCli).await.unwrap();
    assert_eq!(cached, "proj-onboarded");
}

#[tokio::test]
async fn discover_project_errors_without_project_or_default_tier() {
    let base = mock_server(|_| (200, r#"{"currentTier":{"id":"free"}}"#.to_string()));
    let error =
        discover_project(&reqwest::Client::new(), &base, "token-no-project", crate::llm::gemini::Flavor::GeminiCli).await.unwrap_err();
    assert!(error.contains("no project id"), "unexpected error: {error}");
}

/// 对指定请求前缀直接断连（不回响应）的 mock server，其余请求走 respond。
fn dropping_server(drop_prefix: &'static str, respond: impl Fn(&str) -> (u16, String) + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            if request.lines().next().unwrap_or("").starts_with(drop_prefix) {
                drop(stream);
                continue;
            }
            let (status, body) = respond(&request);
            let response = format!(
                "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len(),
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn load_code_assist_http_error_is_reported() {
    let base = mock_server(|_| (500, r#"{"error":{"message":"backend exploded"}}"#.to_string()));
    let error =
        discover_project(&reqwest::Client::new(), &base, "token-load-500", crate::llm::gemini::Flavor::GeminiCli).await.unwrap_err();
    assert!(error.contains("500"), "{error}");
}

#[tokio::test]
async fn load_code_assist_send_failure_does_not_leak_token() {
    let error = discover_project(&reqwest::Client::new(), "http://127.0.0.1:1", "token-send-secret", crate::llm::gemini::Flavor::GeminiCli)
        .await
        .unwrap_err();
    assert!(error.contains("loadCodeAssist failed"), "{error}");
    assert!(!error.contains("token-send-secret"), "凭证不得进入错误串: {error}");
}

#[tokio::test]
async fn onboard_user_http_error_is_reported() {
    let base = mock_server(|request| {
        if request.lines().next().unwrap_or("").starts_with("POST /v1internal:onboardUser") {
            (500, r#"{"error":{"message":"onboard denied"}}"#.to_string())
        } else {
            (200, r#"{"allowedTiers":[{"id":"free-tier","isDefault":true}]}"#.to_string())
        }
    });
    let error =
        discover_project(&reqwest::Client::new(), &base, "token-onboard-500", crate::llm::gemini::Flavor::GeminiCli).await.unwrap_err();
    assert!(error.contains("500"), "{error}");
}

#[tokio::test]
async fn onboard_user_send_failure_is_reported() {
    let base =
        dropping_server("POST /v1internal:onboardUser", |_| (200, r#"{"allowedTiers":[{"id":"free-tier","isDefault":true}]}"#.to_string()));
    let error =
        discover_project(&reqwest::Client::new(), &base, "token-onboard-drop", crate::llm::gemini::Flavor::GeminiCli).await.unwrap_err();
    assert!(error.contains("onboardUser failed"), "{error}");
}

#[tokio::test]
async fn lro_poll_http_error_is_reported() {
    let base = mock_server(|request| {
        let line = request.lines().next().unwrap_or("");
        if line.starts_with("POST /v1internal:onboardUser") {
            (200, r#"{"name":"operations/op1","done":false}"#.to_string())
        } else if line.starts_with("GET /v1internal/operations/op1") {
            (500, r#"{"error":{"message":"poll failed"}}"#.to_string())
        } else {
            (200, r#"{"allowedTiers":[{"id":"free-tier","isDefault":true}]}"#.to_string())
        }
    });
    let error =
        discover_project(&reqwest::Client::new(), &base, "token-poll-500", crate::llm::gemini::Flavor::GeminiCli).await.unwrap_err();
    assert!(error.contains("500"), "{error}");
}

#[tokio::test]
async fn lro_poll_send_failure_is_reported() {
    let base = dropping_server("GET /v1internal/operations", |request| {
        if request.lines().next().unwrap_or("").starts_with("POST /v1internal:onboardUser") {
            (200, r#"{"name":"operations/op1","done":false}"#.to_string())
        } else {
            (200, r#"{"allowedTiers":[{"id":"free-tier","isDefault":true}]}"#.to_string())
        }
    });
    let error =
        discover_project(&reqwest::Client::new(), &base, "token-poll-drop", crate::llm::gemini::Flavor::GeminiCli).await.unwrap_err();
    assert!(error.contains("LRO poll failed"), "{error}");
}

#[tokio::test]
async fn lro_missing_operation_name_is_an_error() {
    let base = mock_server(|request| {
        if request.lines().next().unwrap_or("").starts_with("POST /v1internal:onboardUser") {
            (200, r#"{"done":false}"#.to_string())
        } else {
            (200, r#"{"allowedTiers":[{"id":"free-tier","isDefault":true}]}"#.to_string())
        }
    });
    let error =
        discover_project(&reqwest::Client::new(), &base, "token-no-op-name", crate::llm::gemini::Flavor::GeminiCli).await.unwrap_err();
    assert!(error.contains("missing operation name"), "{error}");
}

#[tokio::test]
async fn lro_that_never_finishes_times_out() {
    let base = mock_server(|request| {
        let line = request.lines().next().unwrap_or("");
        if line.starts_with("POST /v1internal:onboardUser") || line.starts_with("GET /v1internal/operations/op1") {
            (200, r#"{"name":"operations/op1","done":false}"#.to_string())
        } else {
            (200, r#"{"allowedTiers":[{"id":"free-tier","isDefault":true}]}"#.to_string())
        }
    });
    let error =
        discover_project(&reqwest::Client::new(), &base, "token-never-done", crate::llm::gemini::Flavor::GeminiCli).await.unwrap_err();
    assert!(error.contains("did not finish after polling"), "{error}");
}

#[tokio::test]
async fn onboard_finished_without_project_id_is_an_error() {
    let base = mock_server(|request| {
        if request.lines().next().unwrap_or("").starts_with("POST /v1internal:onboardUser") {
            (200, r#"{"name":"operations/op1","done":true,"response":{}}"#.to_string())
        } else {
            (200, r#"{"allowedTiers":[{"id":"free-tier","isDefault":true}]}"#.to_string())
        }
    });
    let error = discover_project(&reqwest::Client::new(), &base, "token-no-final-project", crate::llm::gemini::Flavor::GeminiCli)
        .await
        .unwrap_err();
    assert!(error.contains("without project id"), "{error}");
}
