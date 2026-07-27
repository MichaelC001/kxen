// MCP OAuth 授权流：discovery 双链优先序 / DCR 跳过 / PKCE / 回调解析与超时 / token 落盘往返 /
// 401->refresh->retry->拒则 needs_auth 状态机。全部走 127.0.0.1 mock（std TcpListener），无真实网络。
use kxen_app::mcp::Guard;
use kxen_app::mcp::McpManager;
use kxen_app::mcp::config::{OAuthConfig, RemoteConfig, RemoteKind, ServerConfig};
use kxen_app::mcp::oauth::{self, AuthServerMeta};
use kxen_app::mcp::oauth_flow::{self, TokenGrant};
use kxen_app::mcp::oauth_store::{StoredToken, TokenStore};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// env KXEN_MCP_OAUTH_STORE 是进程全局：凡经 client 建连链读 token 库的测试必须串行。
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Copy, Default, PartialEq)]
enum RefreshOutcome {
    #[default]
    Grant,
    Reject,
}

#[derive(Default)]
struct State {
    hits: Vec<String>,
    token_forms: Vec<String>,
    serve_prm: bool,
    accepted_token: String,
    refresh_outcome: RefreshOutcome,
    refresh_access: String,
}

struct Mock {
    origin: String,
    state: Arc<Mutex<State>>,
}

fn http_response(status: &str, body: &str) -> String {
    format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", body.len(), body)
}

fn route(st: &Arc<Mutex<State>>, port: u16, request_line: &str, headers: &str, body: &str) -> String {
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("").split('?').next().unwrap_or("");
    let (serve_prm, accepted, refresh_outcome, refresh_access) = {
        let s = st.lock().unwrap();
        (s.serve_prm, s.accepted_token.clone(), s.refresh_outcome, s.refresh_access.clone())
    };
    st.lock().unwrap().hits.push(format!("{method} {path}"));
    let meta_prm = json!({
        "authorization_endpoint": format!("http://127.0.0.1:{port}/authorize"),
        "token_endpoint": format!("http://127.0.0.1:{port}/token-prm"),
        "registration_endpoint": format!("http://127.0.0.1:{port}/register"),
    });
    let meta_8414 = json!({
        "authorization_endpoint": format!("http://127.0.0.1:{port}/authorize"),
        "token_endpoint": format!("http://127.0.0.1:{port}/token-8414"),
        "registration_endpoint": format!("http://127.0.0.1:{port}/register"),
    });
    match (method, path) {
        ("GET", "/.well-known/oauth-protected-resource/mcp") | ("GET", "/.well-known/oauth-protected-resource") => {
            if serve_prm {
                let prm = json!({ "authorization_servers": [format!("http://127.0.0.1:{port}/as")] });
                http_response("200 OK", &prm.to_string())
            } else {
                http_response("404 Not Found", "{}")
            }
        }
        ("GET", "/.well-known/oauth-authorization-server/as") => http_response("200 OK", &meta_prm.to_string()),
        ("GET", "/.well-known/oauth-authorization-server/mcp") | ("GET", "/.well-known/oauth-authorization-server") => {
            http_response("200 OK", &meta_8414.to_string())
        }
        ("POST", "/register") => {
            let out = json!({ "client_id": "dcr-client", "client_secret": "dcr-secret" });
            http_response("200 OK", &out.to_string())
        }
        ("POST", "/token-prm") | ("POST", "/token-8414") => {
            st.lock().unwrap().token_forms.push(body.to_string());
            if body.contains("grant_type=refresh_token") && refresh_outcome == RefreshOutcome::Reject {
                return http_response("400 Bad Request", &json!({ "error": "invalid_grant" }).to_string());
            }
            let access = if body.contains("grant_type=refresh_token") { refresh_access } else { "code-access".to_string() };
            let out = json!({ "access_token": access, "refresh_token": "rt2", "expires_in": 3600 });
            http_response("200 OK", &out.to_string())
        }
        ("POST", "/mcp") => {
            let auth = headers
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                .map(|l| l["authorization:".len()..].trim().to_string())
                .unwrap_or_default();
            if auth != format!("Bearer {accepted}") {
                return http_response("401 Unauthorized", "{}");
            }
            let Ok(v) = serde_json::from_str::<Value>(body) else {
                return http_response("400 Bad Request", "{}");
            };
            let id = v.get("id").cloned().unwrap_or(Value::Null);
            let result = match v.get("method").and_then(|m| m.as_str()).unwrap_or("") {
                "initialize" => json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "mock", "version": "0.1" }
                }),
                "tools/list" => json!({ "tools": [ {
                    "name": "echo", "description": "echo",
                    "inputSchema": { "type": "object", "properties": { "text": { "type": "string" } } }
                } ] }),
                "tools/call" => json!({ "content": [ { "type": "text", "text": "pong" } ] }),
                _ => {
                    return http_response(
                        "200 OK",
                        &json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "no method" } }).to_string(),
                    );
                }
            };
            http_response("200 OK", &json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string())
        }
        _ => http_response("404 Not Found", "{}"),
    }
}

/// 每连接一个请求即关（connection: close），reqwest 按需开新连接；与 tests/mcp_remote.rs 同模式。
fn start_mock(serve_prm: bool) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let state = Arc::new(Mutex::new(State { serve_prm, accepted_token: "initial".into(), ..Default::default() }));
    let st = state.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                continue;
            }
            let mut headers = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                headers.push_str(&line);
            }
            let content_length = headers
                .lines()
                .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
                .unwrap_or(0);
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            let resp = route(&st, port, &request_line, &headers, &String::from_utf8_lossy(&body));
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    Mock { origin: format!("http://127.0.0.1:{port}"), state }
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap()
}

#[tokio::test]
async fn discovery_prefers_prm_over_8414() {
    let mock = start_mock(true);
    let meta = oauth::discover(&http_client(), &format!("{}/mcp", mock.origin), None, Guard::Bypassed).await.expect("PRM 链应发现成功");
    assert!(meta.token_endpoint.ends_with("/token-prm"), "PRM 链的 AS 元数据优先: {meta:?}");
    let hits = mock.state.lock().unwrap().hits.clone();
    assert_eq!(hits[0], "GET /.well-known/oauth-protected-resource/mcp", "path-scoped PRM 必须先探: {hits:?}");
    assert!(hits.contains(&"GET /.well-known/oauth-authorization-server/as".to_string()));
    assert!(
        !hits.contains(&"GET /.well-known/oauth-authorization-server/mcp".to_string())
            && !hits.contains(&"GET /.well-known/oauth-authorization-server".to_string()),
        "PRM 成功后不得回落 8414 直连链: {hits:?}"
    );
}

#[tokio::test]
async fn discovery_falls_back_to_8414() {
    let mock = start_mock(false);
    let meta = oauth::discover(&http_client(), &format!("{}/mcp", mock.origin), None, Guard::Bypassed).await.expect("8414 回落应发现成功");
    assert!(meta.token_endpoint.ends_with("/token-8414"), "PRM 404 后回落 8414: {meta:?}");
    let hits = mock.state.lock().unwrap().hits.clone();
    let last_prm = hits.iter().rposition(|h| h.contains("oauth-protected-resource")).unwrap();
    let first_8414 = hits.iter().position(|h| h.contains("oauth-authorization-server/mcp")).unwrap();
    assert!(last_prm < first_8414, "PRM 全链失败后才允许探 8414: {hits:?}");
}

#[tokio::test]
async fn dcr_skipped_when_client_id_configured() {
    let mock = start_mock(true);
    let url = format!("{}/mcp", mock.origin);
    let with_id = RemoteConfig {
        name: "web".into(),
        url: url.clone(),
        transport: RemoteKind::Http,
        headers: HashMap::new(),
        oauth: Some(OAuthConfig { client_id: Some("cfg-client".into()), ..Default::default() }),
    };
    let session = oauth_flow::prepare_login(&with_id, Guard::Bypassed).await.unwrap();
    assert!(session.authorize_url.contains("client_id=cfg-client"), "配置 clientId 直接用: {}", session.authorize_url);
    assert!(session.authorize_url.contains("code_challenge_method=S256"));
    let register_hits = mock.state.lock().unwrap().hits.iter().filter(|h| *h == "POST /register").count();
    assert_eq!(register_hits, 0, "有 clientId 不得走动态注册");

    let without_id = RemoteConfig { oauth: None, ..with_id };
    let session = oauth_flow::prepare_login(&without_id, Guard::Bypassed).await.unwrap();
    assert!(session.authorize_url.contains("client_id=dcr-client"), "无 clientId 必须 DCR: {}", session.authorize_url);
    let register_hits = mock.state.lock().unwrap().hits.iter().filter(|h| *h == "POST /register").count();
    assert_eq!(register_hits, 1, "无 clientId 走一次动态注册");
    let expected_path = format!("/callback/{}", oauth::callback_id(&url));
    assert_eq!(session.callback_path, expected_path, "回调 path 必须绑 callback_id");
}

#[test]
fn pkce_s256_state_and_callback_id() {
    use base64::Engine;
    use sha2::Digest;
    let pkce = oauth::pkce();
    assert_eq!(pkce.verifier.len(), 43, "32 字节 base64url 必为 43 字符");
    let expect = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(pkce.verifier.as_bytes()));
    assert_eq!(pkce.challenge, expect, "challenge 必须是 verifier 的 S256");
    assert_eq!(oauth::random_state().len(), 22, "16 字节 base64url 必为 22 字符");
    assert_eq!(oauth::callback_id("https://x.example/mcp").len(), 12, "9 字节 base64url 必为 12 字符");
    let meta = AuthServerMeta {
        authorization_endpoint: "https://as.example/authorize".into(),
        token_endpoint: "https://as.example/token".into(),
        registration_endpoint: None,
    };
    let url = oauth::authorize_url(&meta, "cid", "http://127.0.0.1:9/callback/ab", "st", "ch", Some("mcp read")).unwrap();
    for needle in ["response_type=code", "client_id=cid", "state=st", "code_challenge=ch", "code_challenge_method=S256", "scope=mcp+read"] {
        assert!(url.contains(needle), "授权 URL 缺 {needle}: {url}");
    }
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A9%2Fcallback%2Fab"), "redirect_uri 必须编码: {url}");
}

// multi_thread：本测试用 std 阻塞 IO 当客户端，单线程运行时会把 wait_callback 任务一起卡死
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn callback_exact_path_code_state_then_404() {
    let (listener, port) = oauth_flow::bind_callback(None).await.unwrap();
    let task = tokio::spawn(async move { oauth_flow::wait_callback(&listener, "/callback/abc", std::time::Duration::from_secs(5)).await });
    // 错 path：404 且继续等
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(b"GET /wrong HTTP/1.1\r\nhost: x\r\n\r\n").unwrap();
    let mut buf = String::new();
    BufReader::new(s.try_clone().unwrap()).read_line(&mut buf).unwrap();
    assert!(buf.contains("404"), "错 path 必须 404: {buf}");
    drop(s);
    // 正 path：解析 code+state 并 200
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(b"GET /callback/abc?code=xyz&state=s1 HTTP/1.1\r\nhost: x\r\n\r\n").unwrap();
    buf.clear();
    BufReader::new(s.try_clone().unwrap()).read_line(&mut buf).unwrap();
    assert!(buf.contains("200"), "正 path 必须 200: {buf}");
    let cb = task.await.unwrap().unwrap();
    assert_eq!(cb.code.as_deref(), Some("xyz"));
    assert_eq!(cb.state.as_deref(), Some("s1"));
}

// multi_thread：同上（std 阻塞客户端 + 运行时内 server task）
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn callback_error_params_and_timeout() {
    let (listener, port) = oauth_flow::bind_callback(None).await.unwrap();
    let task = tokio::spawn(async move { oauth_flow::wait_callback(&listener, "/callback/abc", std::time::Duration::from_secs(5)).await });
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(b"GET /callback/abc?error=access_denied&error_description=nope HTTP/1.1\r\nhost: x\r\n\r\n").unwrap();
    let cb = task.await.unwrap().unwrap();
    assert_eq!(cb.error.as_deref(), Some("access_denied"));
    assert_eq!(cb.error_description.as_deref(), Some("nope"));
    assert!(cb.code.is_none());

    let (listener, _) = oauth_flow::bind_callback(None).await.unwrap();
    let err = oauth_flow::wait_callback(&listener, "/callback/abc", std::time::Duration::from_millis(50)).await.unwrap_err();
    assert!(err.contains("超时"), "短超时必须报超时: {err}");
}

#[test]
fn token_store_roundtrip_0600() {
    let dir = std::env::temp_dir().join(format!("kxen-oauth-store-{}", std::process::id()));
    let path = dir.join("mcp-oauth.json");
    let store = TokenStore::new(path.clone());
    let token = StoredToken {
        access_token: "at".into(),
        refresh_token: Some("rt".into()),
        expires_at: Some(1_900_000_000),
        client_id: "cid".into(),
        client_secret: None,
        token_endpoint: "https://as.example/token".into(),
    };
    store.save_token("web", &token).unwrap();
    let loaded = store.load("web").expect("落盘必须能读回");
    assert_eq!(loaded.access_token, "at");
    assert_eq!(loaded.refresh_token.as_deref(), Some("rt"));
    assert_eq!(loaded.expires_at, Some(1_900_000_000));
    assert_eq!(loaded.client_id, "cid");
    assert_eq!(loaded.token_endpoint, "https://as.example/token");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "token 库必须 0600: {mode:o}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn exchange_code_posts_expected_form() {
    let mock = start_mock(true);
    let endpoint = format!("{}/token-8414", mock.origin);
    let grant: TokenGrant = oauth_flow::exchange_code(
        &http_client(),
        &endpoint,
        "code-1",
        "http://127.0.0.1:9/callback/ab",
        "cid",
        None,
        "verifier-1",
        Guard::Bypassed,
    )
    .await
    .unwrap();
    assert_eq!(grant.access_token, "code-access");
    assert_eq!(grant.refresh_token.as_deref(), Some("rt2"));
    assert!(grant.expires_at.is_some(), "expires_in 必须折算 expires_at");
    let forms = mock.state.lock().unwrap().token_forms.clone();
    let body = &forms[0];
    for needle in [
        "grant_type=authorization_code",
        "code=code-1",
        "client_id=cid",
        "code_verifier=verifier-1",
        "redirect_uri=http%3A%2F%2F127.0.0.1%3A9%2Fcallback%2Fab",
    ] {
        assert!(body.contains(needle), "token 请求缺 {needle}: {body}");
    }
}

/// 状态机：存旧 token -> 401 -> refresh 成功 -> 重试通过（running）；
/// refresh 也被拒 -> AUTH_REQUIRED -> needs_auth 且连接被丢弃。
#[tokio::test]
async fn http_401_refresh_retry_then_needs_auth() {
    let _env = ENV_LOCK.lock().await;
    let dir = std::env::temp_dir().join(format!("kxen-oauth-flow-{}", std::process::id()));
    let store_path = dir.join("mcp-oauth.json");
    // WHY unsafe：env 是进程全局，本文件内此类测试由 ENV_LOCK 串行
    unsafe { std::env::set_var("KXEN_MCP_OAUTH_STORE", &store_path) };
    let mock = start_mock(true);
    let endpoint = format!("{}/token-8414", mock.origin);
    let store = TokenStore::new(store_path.clone());
    store
        .save_token(
            "web",
            &StoredToken {
                access_token: "stale-1".into(),
                refresh_token: Some("rt1".into()),
                expires_at: None,
                client_id: "cid".into(),
                client_secret: None,
                token_endpoint: endpoint,
            },
        )
        .unwrap();
    {
        let mut s = mock.state.lock().unwrap();
        s.accepted_token = "good-2".into();
        s.refresh_access = "good-2".into();
    }
    let cfg = ServerConfig::Remote(RemoteConfig {
        name: "web".into(),
        url: format!("{}/mcp", mock.origin),
        transport: RemoteKind::Http,
        headers: HashMap::new(),
        oauth: None,
    });
    let mgr = McpManager::new();
    mgr.start_bypassing_guard_for_test(vec![cfg]).await;
    let status = mgr.status();
    assert_eq!(status[0].status, "running", "refresh 成功后重试必须建连: {status:?}");
    let saved = store.load("web").unwrap();
    assert_eq!(saved.access_token, "good-2", "refresh 结果必须落盘");
    assert_eq!(saved.refresh_token.as_deref(), Some("rt2"), "新 refresh_token 必须替换旧的");
    assert_eq!(mock.state.lock().unwrap().token_forms.len(), 1, "只 refresh 一次");

    // refresh 被拒：call -> 401 -> refresh 400 -> needs_auth，连接丢弃
    {
        let mut s = mock.state.lock().unwrap();
        s.accepted_token = "never".into();
        s.refresh_outcome = RefreshOutcome::Reject;
    }
    let err = mgr.call("web", "echo", &json!({ "text": "hi" })).await.unwrap_err();
    assert!(oauth::is_auth_required(&err), "refresh 被拒必须 AUTH_REQUIRED: {err}");
    let status = mgr.status();
    assert_eq!(status[0].status, "needs_auth", "refresh 被拒后必须 needs_auth: {status:?}");
    assert_eq!(status[0].tools, 0, "连接已丢弃不得保留工具缓存");
    unsafe { std::env::remove_var("KXEN_MCP_OAUTH_STORE") };
    std::fs::remove_dir_all(&dir).ok();
}

/// config 显式 Authorization 被 401：报失败且不回落 OAuth（不标 needs_auth、不试 refresh）。
#[tokio::test]
async fn explicit_authorization_rejected_no_oauth_fallback() {
    let _env = ENV_LOCK.lock().await;
    let mock = start_mock(true);
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), "Bearer wrong".to_string());
    let cfg = ServerConfig::Remote(RemoteConfig {
        name: "web".into(),
        url: format!("{}/mcp", mock.origin),
        transport: RemoteKind::Http,
        headers,
        oauth: None,
    });
    let mgr = McpManager::new();
    mgr.start_bypassing_guard_for_test(vec![cfg]).await;
    let status = mgr.status();
    assert_eq!(status[0].status, "down", "显式 Authorization 被拒只报失败，不得标 needs_auth: {status:?}");
    assert!(mock.state.lock().unwrap().token_forms.is_empty(), "显式 Authorization 不得触发 refresh");
}
