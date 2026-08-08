use super::*;
use serde_json::json;
use std::io::{Read, Write};

/// 固定状态行与 JSON body 的单发 mock server，返回 base URL。
fn one_shot_server(status: &'static str, body: &'static str) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            let reply = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    format!("http://{address}")
}

fn rfc_spec(token_url: String) -> DeviceSpec {
    DeviceSpec {
        device_url: "https://example.invalid/device",
        token_url: Box::leak(token_url.into_boxed_str()),
        client_id: "test-client",
        scope: None,
        extra_device: &[],
        flavor: DeviceFlavor::Rfc8628 { pkce: false },
        copilot_exchange: false,
        extra_headers: &[],
    }
}

#[tokio::test]
async fn poll_five_xx_keeps_polling_instead_of_failing() {
    let base = one_shot_server("503 Service Unavailable", r#"{"error":"temporarily_unavailable"}"#);
    let spec = rfc_spec(format!("{base}/token"));
    let outcome = poll_once(&spec, "dc", None).await.expect("5xx must not abort the login");
    assert!(matches!(outcome, PollOutcome::Pending));
}

#[tokio::test]
async fn poll_network_failure_keeps_polling_instead_of_failing() {
    // 拿一个刚释放的端口制造 connection refused。
    let address = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let spec = rfc_spec(format!("http://{address}/token"));
    let outcome = poll_once(&spec, "dc", None).await.expect("network error must not abort the login");
    assert!(matches!(outcome, PollOutcome::Pending));
}

#[tokio::test]
async fn poll_four_xx_auth_error_fails_fast() {
    let base = one_shot_server("400 Bad Request", r#"{"error":"access_denied"}"#);
    let spec = rfc_spec(format!("{base}/token"));
    let error = poll_once(&spec, "dc", None).await.err().expect("4xx auth error must abort the login");
    assert!(error.contains("拒绝"), "{error}");
}

#[tokio::test]
async fn poll_success_returns_grant() {
    let base = one_shot_server("200 OK", r#"{"access_token":"a1","refresh_token":"r1","expires_in":3600}"#);
    let spec = rfc_spec(format!("{base}/token"));
    let outcome = poll_once(&spec, "dc", None).await.expect("200 must grant");
    assert!(
        matches!(outcome, PollOutcome::Granted(ref grant) if grant.access_token == "a1" && grant.refresh_token.as_deref() == Some("r1"))
    );
}

#[test]
fn copilot_credential_shape_keeps_github_token_in_refresh_slot() {
    let credential = CredentialKind::Oauth { access: "jwt".into(), refresh: "ghu_x".into(), expires: 1, account_id: None };
    assert!(matches!(credential, CredentialKind::Oauth { ref refresh, .. } if refresh == "ghu_x"));
}

#[test]
fn expired_in_dual_semantics_resolves_to_remaining_secs() {
    // TTL 秒：远小于毫秒时间戳，原样返回。
    assert_eq!(expired_in_to_secs(900), 900);
    // 毫秒时间戳：now + 300s -> 剩余约 300 秒。
    let ts = crate::core::shared::now_ms() + 300_000;
    let secs = expired_in_to_secs(ts);
    assert!((290..=300).contains(&secs), "expected ~300s, got {secs}");
    // 已过期的毫秒时间戳 -> 0。
    assert_eq!(expired_in_to_secs(crate::core::shared::now_ms() - 1_000), 0);
}

#[test]
fn minimax_device_parse_uses_user_code_and_ms_interval() {
    let value = json!({
        "user_code": "UCODE-1",
        "verification_uri": "https://api.minimax.io/oauth/authorize",
        "expired_in": 600,
        "interval": 2000,
        "state": "s1",
    });
    let parsed = parse_minimax_device(&value, Some("s1")).expect("valid response");
    assert_eq!(parsed.device_code, "UCODE-1");
    assert_eq!(parsed.user_code, "UCODE-1");
    assert_eq!(parsed.interval, 2);
    assert_eq!(parsed.expires_in, 600);
}

#[test]
fn minimax_device_parse_rejects_state_mismatch_and_bad_url() {
    let value = json!({ "user_code": "U", "verification_uri": "https://api.minimax.io/x", "state": "other" });
    assert!(parse_minimax_device(&value, Some("s1")).expect_err("state echo mismatch must fail").contains("state"));
    let value = json!({ "user_code": "U", "verification_uri": "http://insecure.example/x" });
    assert!(parse_minimax_device(&value, None).expect_err("non-https uri must fail").contains("https"));
}
