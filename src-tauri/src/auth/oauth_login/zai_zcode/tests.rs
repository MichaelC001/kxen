use super::*;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

type Seen = Arc<Mutex<Vec<(String, String, String, String)>>>;

/// 顺序应答的 mock server：记录 (method, path, authorization, body)，按 path 路由响应。
fn mock_server(handler: impl Fn(&str, &str) -> (u16, String) + Send + 'static) -> (String, Seen) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let seen_in = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let (method, path, authorization, body) = read_request(&mut stream);
            let (status, response) = handler(&method, &path);
            crate::core::shared::lock(&seen_in).push((method, path, authorization, body));
            let reply = format!(
                "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{response}",
                response.len()
            );
            if stream.write_all(reply.as_bytes()).is_err() {
                break;
            }
        }
    });
    (format!("http://{address}"), seen)
}

fn read_request(stream: &mut std::net::TcpStream) -> (String, String, String, String) {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
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
    let header_value = |name: &str| {
        header.lines().find_map(|line| line.to_ascii_lowercase().starts_with(name).then(|| line[name.len()..].trim().to_string()))
    };
    let content_length = header_value("content-length:").and_then(|value| value.parse::<usize>().ok()).unwrap_or(0);
    while buffer.len() < header_end + content_length {
        let n = stream.read(&mut chunk).unwrap_or(0);
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&buffer[header_end..]).into_owned();
    let mut parts = header.split_whitespace();
    (
        parts.next().unwrap_or("").to_string(),
        parts.next().unwrap_or("").to_string(),
        header_value("authorization:").unwrap_or_default(),
        body,
    )
}

fn endpoints(base: &str) -> Endpoints {
    Endpoints { broker: format!("{base}/oauth/token"), z_login: format!("{base}/api/auth/z/login"), api_base: base.to_string() }
}

const CUSTOMER_INFO: &str =
    r#"{"data":{"organizations":[{"organizationId":"org-1","isDefault":true,"projects":[{"projectId":"proj-1","isDefault":true}]}]}}"#;

#[test]
fn spec_registered_as_zai_zcode_exchange() {
    let Some(super::super::spec::FlowSpec::Code(spec)) = super::super::spec::spec_for("zhipu-coding") else {
        panic!("zhipu-coding spec missing")
    };
    assert_eq!(spec.exchange_kind, super::super::spec::ExchangeKind::ZaiZcode);
    assert!(!spec.pkce);
    assert!(spec.use_state && spec.manual_paste);
    assert_eq!(spec.token_url, "https://zcode.z.ai/api/v1/oauth/token");
}

#[test]
fn envelope_accepts_documented_success_shapes() {
    for ok in [json!({"code": 0}), json!({"code": 200}), json!({"code": "0"}), json!({"data": {}}), json!({"code": null})] {
        assert!(envelope_error(&ok).is_none(), "{ok}");
    }
    assert_eq!(envelope_error(&json!({"code": 401, "msg": "authorization code expired"})).as_deref(), Some("authorization code expired"));
    assert_eq!(envelope_error(&json!({"code": 0, "success": false, "msg": "oauth required"})).as_deref(), Some("oauth required"));
}

#[test]
fn envelope_rejects_unknown_code_types_and_falls_back_to_default_msg() {
    // 非数字非字符串的 code（bool/数组）一律视为失败；msg 缺失时用默认措辞。
    assert_eq!(envelope_error(&json!({"code": true})).as_deref(), Some("未知错误"));
    assert_eq!(envelope_error(&json!({"code": ["0"]})).as_deref(), Some("未知错误"));
    assert_eq!(envelope_error(&json!({"code": 500})).as_deref(), Some("未知错误"));
    assert_eq!(envelope_error(&json!({"success": false})).as_deref(), Some("未知错误"));
}

#[test]
fn pick_default_org_project_prefers_default_and_falls_back_to_first() {
    let (org, project) = pick_default_org_project(&json!({"data":{"organizations":[
        {"organizationId":"org-a","projects":[{"projectId":"proj-a"}]},
        {"organizationId":"org-b","isDefault":true,"projects":[{"projectId":"p1"},{"projectId":"p2","isDefault":true}]}
    ]}}))
    .unwrap();
    assert_eq!((org.as_str(), project.as_str()), ("org-b", "p2"));
    assert!(pick_default_org_project(&json!({"data": {"organizations": []}})).is_err());
    assert!(pick_default_org_project(&json!({"data": {}})).is_err());
}

#[test]
fn pick_default_org_project_reports_each_missing_piece() {
    let cases = [
        (json!({"organizations": [{"projects": []}]}), "organizationId"),
        (json!({"organizations": [{"organizationId": "org-1"}]}), "projects"),
        (json!({"organizations": [{"organizationId": "org-1", "projects": []}]}), "projects 为空"),
        (json!({"organizations": [{"organizationId": "org-1", "projects": [{}]}]}), "projectId"),
    ];
    for (payload, expected) in cases {
        let error = pick_default_org_project(&payload).expect_err("必须失败");
        assert!(error.contains(expected), "{error} 应含 {expected}");
    }
}

#[test]
fn api_key_id_accepts_alias_and_rejects_blank() {
    assert_eq!(api_key_id(&json!({"apiKey": " k1 "})).as_deref(), Some("k1"));
    assert_eq!(api_key_id(&json!({"id": "k2"})).as_deref(), Some("k2"), "兼容 id 别名");
    assert!(api_key_id(&json!({"apiKey": "  " })).is_none());
    assert!(api_key_id(&json!({})).is_none());
}

#[tokio::test]
async fn three_stage_flow_reuses_existing_api_key() {
    let (base, seen) = mock_server(|method, path| match (method, path) {
        ("POST", "/oauth/token") => (
            200,
            json!({"code":0,"data":{"token":"zcode-jwt","zai":{"access_token":"upstream-token"},"user":{"user_id":"u1"}}}).to_string(),
        ),
        ("POST", "/api/auth/z/login") => (200, json!({"code":0,"data":{"access_token":"business-token"}}).to_string()),
        ("GET", "/api/biz/customer/getCustomerInfo") => (200, CUSTOMER_INFO.to_string()),
        ("GET", "/api/biz/v1/organization/org-1/projects/proj-1/api_keys") => {
            (200, json!({"data":[{"name":"other","apiKey":"k0"},{"name":"zcode-api-key","apiKey":"key-id"}]}).to_string())
        }
        ("GET", "/api/biz/v1/organization/org-1/projects/proj-1/api_keys/copy/key-id") => {
            (200, json!({"data":{"secretKey":"secret"}}).to_string())
        }
        _ => (404, json!({"msg":"unexpected"}).to_string()),
    });
    let credential = run(&reqwest::Client::new(), &endpoints(&base), "the-code", "http://localhost:1/cb", "the-state").await.unwrap();
    assert!(matches!(credential, CredentialKind::Api { ref key, region: None } if key == "key-id.secret"));
    let seen = crate::core::shared::lock(&seen);
    assert_eq!(seen.len(), 5, "{seen:?}");
    let broker_body: Value = serde_json::from_str(&seen[0].3).unwrap();
    assert_eq!(broker_body, json!({"provider":"zai","code":"the-code","redirect_uri":"http://localhost:1/cb","state":"the-state"}));
    let login_body: Value = serde_json::from_str(&seen[1].3).unwrap();
    assert_eq!(login_body, json!({"token":"upstream-token"}));
    assert!(seen[2..].iter().all(|(_, _, auth, _)| auth == "Bearer business-token"), "{seen:?}");
}

#[tokio::test]
async fn three_stage_flow_creates_key_when_missing() {
    let (base, seen) = mock_server(|method, path| match (method, path) {
        ("POST", "/oauth/token") => (200, json!({"data":{"token":"jwt","zai":{"access_token":"up"}}}).to_string()),
        ("POST", "/api/auth/z/login") => (200, json!({"data":{"access_token":"biz"}}).to_string()),
        ("GET", "/api/biz/customer/getCustomerInfo") => (200, CUSTOMER_INFO.to_string()),
        ("GET", "/api/biz/v1/organization/org-1/projects/proj-1/api_keys") => (200, json!({"data":[]}).to_string()),
        ("POST", "/api/biz/v1/organization/org-1/projects/proj-1/api_keys") => (200, json!({"data":{"apiKey":"new-id"}}).to_string()),
        ("GET", "/api/biz/v1/organization/org-1/projects/proj-1/api_keys/copy/new-id") => {
            (200, json!({"data":{"secretKey":"new-secret"}}).to_string())
        }
        _ => (404, json!({"msg":"unexpected"}).to_string()),
    });
    let credential = run(&reqwest::Client::new(), &endpoints(&base), "c", "r", "s").await.unwrap();
    assert!(matches!(credential, CredentialKind::Api { ref key, .. } if key == "new-id.new-secret"));
    let seen = crate::core::shared::lock(&seen);
    let create_body: Value = serde_json::from_str(&seen[4].3).unwrap();
    assert_eq!(create_body, json!({"name":"zcode-api-key"}));
}

#[tokio::test]
async fn broker_failures_surface_loudly() {
    let (expired, _) = mock_server(|_, _| (200, json!({"code":401,"msg":"authorization code expired"}).to_string()));
    let error = run(&reqwest::Client::new(), &endpoints(&expired), "c", "r", "s").await.unwrap_err();
    assert!(error.contains("authorization code expired"), "{error}");
    let (missing, _) = mock_server(|_, _| (200, json!({"code":0,"data":{"token":"jwt"}}).to_string()));
    let error = run(&reqwest::Client::new(), &endpoints(&missing), "c", "r", "s").await.unwrap_err();
    assert!(error.contains("data.zai.access_token"), "{error}");
}

#[tokio::test]
async fn exchange_entrypoint_surfaces_broker_error_before_production_endpoints() {
    // exchange() 的 z/login 与 api_base 是生产常量：broker 直接失败，流程在触达生产端点前返回。
    let (broker, seen) = mock_server(|_, _| (200, json!({"code":401,"msg":"state mismatch"}).to_string()));
    let error = exchange(&reqwest::Client::new(), &format!("{broker}/oauth/token"), "c", "r", "s").await.unwrap_err();
    assert!(error.contains("state mismatch"), "{error}");
    assert_eq!(crate::core::shared::lock(&seen).len(), 1, "broker 失败后不得再请求下游");
}

#[tokio::test]
async fn broker_http_error_status_is_reported_with_detail() {
    let (base, _) = mock_server(|_, _| (500, json!({"msg":"backend exploded"}).to_string()));
    let error = run(&reqwest::Client::new(), &endpoints(&base), "c", "r", "s").await.unwrap_err();
    assert!(error.contains("http 500"), "{error}");
    assert!(error.contains("backend exploded"), "{error}");
}

#[tokio::test]
async fn z_login_envelope_failure_and_missing_token_surface_loudly() {
    let (failed, _) = mock_server(|_, path| match path {
        "/oauth/token" => (200, json!({"data":{"token":"jwt","zai":{"access_token":"up"}}}).to_string()),
        "/api/auth/z/login" => (200, json!({"code":401,"msg":"oauth required"}).to_string()),
        _ => (404, json!({"msg":"unexpected"}).to_string()),
    });
    let error = run(&reqwest::Client::new(), &endpoints(&failed), "c", "r", "s").await.unwrap_err();
    assert!(error.contains("z/login 失败：oauth required"), "{error}");

    let (missing, _) = mock_server(|_, path| match path {
        "/oauth/token" => (200, json!({"data":{"token":"jwt","zai":{"access_token":"up"}}}).to_string()),
        "/api/auth/z/login" => (200, json!({"code":0,"data":{}}).to_string()),
        _ => (404, json!({"msg":"unexpected"}).to_string()),
    });
    let error = run(&reqwest::Client::new(), &endpoints(&missing), "c", "r", "s").await.unwrap_err();
    assert!(error.contains("data.access_token"), "{error}");
}

#[tokio::test]
async fn create_key_response_missing_id_is_an_error() {
    let (base, _) = mock_server(|method, path| match (method, path) {
        ("POST", "/oauth/token") => (200, json!({"data":{"token":"jwt","zai":{"access_token":"up"}}}).to_string()),
        ("POST", "/api/auth/z/login") => (200, json!({"data":{"access_token":"biz"}}).to_string()),
        ("GET", "/api/biz/customer/getCustomerInfo") => (200, CUSTOMER_INFO.to_string()),
        ("GET", "/api/biz/v1/organization/org-1/projects/proj-1/api_keys") => (200, json!({"data":[]}).to_string()),
        ("POST", "/api/biz/v1/organization/org-1/projects/proj-1/api_keys") => (200, json!({"data":{}}).to_string()),
        _ => (404, json!({"msg":"unexpected"}).to_string()),
    });
    let error = run(&reqwest::Client::new(), &endpoints(&base), "c", "r", "s").await.unwrap_err();
    assert!(error.contains("apiKey id"), "{error}");
}

#[tokio::test]
async fn copy_response_missing_secret_key_is_an_error() {
    let (base, _) = mock_server(|method, path| match (method, path) {
        ("POST", "/oauth/token") => (200, json!({"data":{"token":"jwt","zai":{"access_token":"up"}}}).to_string()),
        ("POST", "/api/auth/z/login") => (200, json!({"data":{"access_token":"biz"}}).to_string()),
        ("GET", "/api/biz/customer/getCustomerInfo") => (200, CUSTOMER_INFO.to_string()),
        ("GET", "/api/biz/v1/organization/org-1/projects/proj-1/api_keys") => {
            (200, json!({"data":[{"name":"zcode-api-key","apiKey":"key-id"}]}).to_string())
        }
        ("GET", "/api/biz/v1/organization/org-1/projects/proj-1/api_keys/copy/key-id") => (200, json!({"data":{}}).to_string()),
        _ => (404, json!({"msg":"unexpected"}).to_string()),
    });
    let error = run(&reqwest::Client::new(), &endpoints(&base), "c", "r", "s").await.unwrap_err();
    assert!(error.contains("secretKey"), "{error}");
}
