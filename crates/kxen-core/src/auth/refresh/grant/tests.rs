use super::*;
use std::io::{Read, Write};

#[test]
fn empty_access_is_rejected_before_persisting() {
    let key = format!("test:empty-access-{}", std::process::id());
    let path = std::env::temp_dir().join(format!("kxen-empty-refresh-{}.json", uuid::Uuid::new_v4()));
    let mut store = AuthStore::default();
    let error = apply_refresh_to(
        &mut store,
        &key,
        super::super::RefreshResponse { access_token: "  ".into(), refresh_token: None, expires_in: Some(u64::MAX) },
        "r1",
        None,
        &path,
    )
    .expect_err("empty access token must fail closed");
    assert!(error.to_string().contains("empty access token"));
    assert!(!path.exists());
    assert!(!store.contains_key(&key));
}

#[test]
fn post_commit_sync_failure_publishes_visible_refresh_and_reports_indeterminate() {
    let key = format!("test:refresh-indeterminate-{}", uuid::Uuid::new_v4());
    let path = std::env::temp_dir().join(format!("kxen-refresh-indeterminate-{}.json", uuid::Uuid::new_v4()));
    let old = CredentialKind::Oauth { access: "old-access".into(), refresh: "old-refresh".into(), expires: 1, account_id: None };
    let mut store = AuthStore::from([(key.clone(), old.clone())]);
    let shared = std::sync::Arc::new(std::sync::Mutex::new(std::sync::Arc::new(AuthStore::from([(key.clone(), old)]))));
    crate::auth::shared_store::register_shared_store(&shared);
    crate::auth::credential::write_auth_file(&path, &store).unwrap();
    crate::auth::credential::fail_next_auth_dir_sync();

    let error = apply_refresh_to(
        &mut store,
        &key,
        super::super::RefreshResponse {
            access_token: "new-access".into(),
            refresh_token: Some("new-refresh".into()),
            expires_in: Some(3600),
        },
        "old-refresh",
        None,
        &path,
    )
    .expect_err("post-commit directory sync failure must be reported as indeterminate");

    assert!(error.to_string().contains("durability is indeterminate"), "{error}");
    let on_disk = crate::auth::credential::read_auth_file(&path).unwrap();
    for snapshot in [&store, &on_disk, &*crate::core::shared::lock(&shared)] {
        assert!(matches!(snapshot.get(&key), Some(CredentialKind::Oauth { access, .. }) if access == "new-access"));
    }
    assert!(
        matches!(crate::core::shared::lock(super::super::recent()).get(&key), Some(CredentialKind::Oauth { access, .. }) if access == "new-access")
    );
    crate::core::shared::lock(super::super::recent()).remove(&key);
    std::fs::remove_file(path).ok();
}

#[tokio::test]
async fn refresh_token_post_does_not_follow_redirects() {
    let sink = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    sink.set_nonblocking(true).unwrap();
    let sink_addr = sink.local_addr().unwrap();
    let sink_thread = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while std::time::Instant::now() < deadline {
            match sink.accept() {
                Ok(_) => return true,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(_) => return false,
            }
        }
        false
    });
    let redirect = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let redirect_addr = redirect.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = redirect.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        let response = format!(
            "HTTP/1.1 307 Temporary Redirect\r\nlocation: http://{sink_addr}/token\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    let mut store = AuthStore::default();
    let error = run_grant_to(
        &mut store,
        "openai",
        GrantParams {
            url: &format!("http://{redirect_addr}"),
            client_id: "client",
            client_secret: None,
            style: GrantStyle::Json,
            refresh: "refresh-secret",
            acc_id: &None,
        },
        &std::env::temp_dir().join(format!("kxen-unused-auth-{}", uuid::Uuid::new_v4())),
    )
    .await
    .expect_err("redirect response must not be followed");
    assert!(error.contains("307"));
    assert!(!sink_thread.join().unwrap(), "refresh token request leaked to redirect target");
}

/// 记录原始请求文本的 mock server：固定状态行与 JSON body 循环应答。
fn recording_server(status: &'static str, body: &'static str) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_in = std::sync::Arc::clone(&seen);
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = Vec::new();
            let mut chunk = [0_u8; 8192];
            let header_end = loop {
                if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break buffer.len();
                }
                buffer.extend_from_slice(&chunk[..n]);
            };
            let header = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
            let content_length = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase().starts_with("content-length:").then(|| line["content-length:".len()..].trim().to_string())
                })
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            while buffer.len() < header_end + content_length {
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buffer.extend_from_slice(&chunk[..n]);
            }
            crate::core::shared::lock(&seen_in).push(String::from_utf8_lossy(&buffer).into_owned());
            let reply = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            if stream.write_all(reply.as_bytes()).is_err() {
                break;
            }
        }
    });
    (format!("http://{address}"), seen)
}

fn grant_auth_file(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("kxen-grant-{tag}-{}.json", uuid::Uuid::new_v4()))
}

fn unpublish(key: &str, path: &std::path::Path) {
    crate::core::shared::lock(super::super::recent()).remove(key);
    std::fs::remove_file(path).ok();
}

#[tokio::test]
async fn form_grant_with_client_secret_posts_urlencoded_body() {
    let (base, seen) = recording_server("200 OK", r#"{"access_token":"a1","refresh_token":"r2","expires_in":3600}"#);
    let key = format!("test:grant-form-{}", uuid::Uuid::new_v4());
    let path = grant_auth_file("form");
    let mut store = AuthStore::default();
    run_grant_to(
        &mut store,
        &key,
        GrantParams {
            url: &format!("{base}/token"),
            client_id: "cid",
            client_secret: Some("sec ret"),
            style: GrantStyle::Form,
            refresh: "refresh tok",
            acc_id: &None,
        },
        &path,
    )
    .await
    .unwrap();
    let request = crate::core::shared::lock(&seen).pop().expect("request recorded");
    assert!(request.starts_with("POST /token "), "{request}");
    assert!(request.to_ascii_lowercase().contains("content-type: application/x-www-form-urlencoded"), "{request}");
    let body = request.rsplit("\r\n\r\n").next().unwrap_or("");
    for field in ["grant_type=refresh_token", "refresh_token=refresh+tok", "client_id=cid", "client_secret=sec+ret"] {
        assert!(body.contains(field), "body 缺 {field}: {body}");
    }
    assert!(matches!(store.get(&key), Some(CredentialKind::Oauth { access, refresh, .. }) if access == "a1" && refresh == "r2"));
    unpublish(&key, &path);
}

#[tokio::test]
async fn json_grant_includes_client_secret_when_present() {
    let (base, seen) = recording_server("200 OK", r#"{"access_token":"a2"}"#);
    let key = format!("test:grant-json-{}", uuid::Uuid::new_v4());
    let path = grant_auth_file("json");
    let mut store = AuthStore::default();
    run_grant_to(
        &mut store,
        &key,
        GrantParams {
            url: &format!("{base}/token"),
            client_id: "cid",
            client_secret: Some("json-secret"),
            style: GrantStyle::Json,
            refresh: "old-refresh",
            acc_id: &None,
        },
        &path,
    )
    .await
    .unwrap();
    let request = crate::core::shared::lock(&seen).pop().expect("request recorded");
    let body: serde_json::Value = serde_json::from_str(request.rsplit("\r\n\r\n").next().unwrap_or("")).expect("json body");
    assert_eq!(
        body,
        serde_json::json!({"grant_type": "refresh_token", "refresh_token": "old-refresh", "client_id": "cid", "client_secret": "json-secret"})
    );
    // 响应缺 refresh_token：保留旧 refresh 槽。
    assert!(matches!(store.get(&key), Some(CredentialKind::Oauth { refresh, .. }) if refresh == "old-refresh"));
    unpublish(&key, &path);
}

#[tokio::test]
async fn non_success_status_is_an_error_without_persisting() {
    let (base, _) = recording_server("500 Internal Server Error", r#"{"error":"server exploded"}"#);
    let key = format!("test:grant-500-{}", uuid::Uuid::new_v4());
    let path = grant_auth_file("500");
    let mut store = AuthStore::default();
    let error = run_grant_to(
        &mut store,
        &key,
        GrantParams {
            url: &format!("{base}/token"),
            client_id: "cid",
            client_secret: None,
            style: GrantStyle::Json,
            refresh: "r",
            acc_id: &None,
        },
        &path,
    )
    .await
    .expect_err("http 500 must fail");
    assert!(error.contains("HTTP 500"), "{error}");
    assert!(!store.contains_key(&key));
    assert!(!path.exists());
}

#[tokio::test]
async fn non_json_success_body_is_an_error_without_persisting() {
    let (base, _) = recording_server("200 OK", "not-json{");
    let key = format!("test:grant-badjson-{}", uuid::Uuid::new_v4());
    let path = grant_auth_file("badjson");
    let mut store = AuthStore::default();
    let error = run_grant_to(
        &mut store,
        &key,
        GrantParams {
            url: &format!("{base}/token"),
            client_id: "cid",
            client_secret: None,
            style: GrantStyle::Form,
            refresh: "r",
            acc_id: &None,
        },
        &path,
    )
    .await
    .expect_err("non-json 200 must fail");
    assert!(error.contains("invalid"), "{error}");
    assert!(!store.contains_key(&key));
    assert!(!path.exists());
}
