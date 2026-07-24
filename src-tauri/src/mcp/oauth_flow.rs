//! OAuth 授权流编排：127.0.0.1 最小 HTTP 回调 server + token 交换/刷新 + prepare/finish 两段式登录。
//! 回调 path 带 callback_id（见 oauth.rs）绑定 redirect 与 server；等待上限 CALLBACK_TIMEOUT。

use super::config::RemoteConfig;
use super::oauth::{
    AuthServerMeta, CALLBACK_TIMEOUT, authorize_url, callback_id, discover, pkce, random_state,
    register,
};
use super::oauth_store::TokenStore;
use super::remote::Guard;
use serde_json::Value;

#[derive(Debug, Default)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

/// 绑回调端口：配置端口被占回退 :0 随机（固定端口只是便利，不该让授权流起不来）。
pub async fn bind_callback(
    port: Option<u16>,
) -> Result<(tokio::net::TcpListener, u16), String> {
    if let Some(p) = port {
        match tokio::net::TcpListener::bind(("127.0.0.1", p)).await {
            Ok(l) => return Ok((l, p)),
            Err(e) => tracing::warn!(port = p, error = %e, "oauth 回调端口被占，回退随机端口"),
        }
    }
    let l = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| format!("oauth 回调端口绑定失败: {e}"))?;
    let port = l.local_addr().map_err(|e| e.to_string())?.port();
    Ok((l, port))
}

/// 等一个回调：path 精确匹配才消费（错 path 回 404 继续等），整体包 timeout。
pub async fn wait_callback(
    listener: &tokio::net::TcpListener,
    expected_path: &str,
    timeout: std::time::Duration,
) -> Result<CallbackParams, String> {
    let work = async {
        loop {
            let (sock, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let mut reader = tokio::io::BufReader::new(sock);
            use tokio::io::AsyncBufReadExt;
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).await.unwrap_or(0) == 0 {
                continue;
            }
            // header 段读尽（回调是 GET 无 body；不读尽对端可能阻塞在写）
            let mut line = String::new();
            loop {
                line.clear();
                let n = reader.read_line(&mut line).await.unwrap_or(0);
                if n == 0 || line == "\r\n" {
                    break;
                }
            }
            let target = request_line.split_whitespace().nth(1).unwrap_or("").to_string();
            let parsed = reqwest::Url::parse(&format!("http://127.0.0.1{target}"));
            let mut sock = reader.into_inner();
            use tokio::io::AsyncWriteExt;
            let Ok(parsed) = parsed else {
                let _ = sock.write_all(b"HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n").await;
                continue;
            };
            if parsed.path() != expected_path {
                let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n").await;
                continue;
            }
            let mut out = CallbackParams::default();
            for (k, v) in parsed.query_pairs() {
                match k.as_ref() {
                    "code" => out.code = Some(v.into_owned()),
                    "state" => out.state = Some(v.into_owned()),
                    "error" => out.error = Some(v.into_owned()),
                    "error_description" => out.error_description = Some(v.into_owned()),
                    _ => {}
                }
            }
            let html = "<html><body><h3>kxen MCP 认证完成，可以关闭本页面</h3></body></html>";
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\n\r\n{}",
                html.len(),
                html
            );
            let _ = sock.write_all(resp.as_bytes()).await;
            return Ok(out);
        }
    };
    match tokio::time::timeout(timeout, work).await {
        Ok(r) => r,
        Err(_) => Err("oauth 等待回调超时".into()),
    }
}

/// token 端点应答（expires_in 已折算成 expires_at 供落盘）。
pub struct TokenGrant {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
}

async fn post_token(
    http: &reqwest::Client,
    endpoint: &str,
    form: Vec<(&str, &str)>,
    guard: Guard,
) -> Result<TokenGrant, String> {
    if guard == Guard::Enforced {
        crate::tools::net_guard::check_url(endpoint).await?;
    }
    let resp = http
        .post(endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("oauth token {endpoint}: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text: String = resp.text().await.unwrap_or_default().chars().take(200).collect();
        return Err(format!("oauth token http {status}: {text}"));
    }
    let v = resp.json::<Value>().await.map_err(|e| format!("oauth token bad json: {e}"))?;
    let access_token = v
        .get("access_token")
        .and_then(|s| s.as_str())
        .ok_or("oauth token response missing access_token")?
        .to_string();
    let expires_at = v.get("expires_in").and_then(|n| n.as_u64()).map(|secs| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() + secs)
            .unwrap_or(0)
    });
    Ok(TokenGrant {
        access_token,
        refresh_token: v.get("refresh_token").and_then(|s| s.as_str()).map(String::from),
        expires_at,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn exchange_code(
    http: &reqwest::Client,
    token_endpoint: &str,
    code: &str,
    redirect_uri: &str,
    client_id: &str,
    client_secret: Option<&str>,
    verifier: &str,
    guard: Guard,
) -> Result<TokenGrant, String> {
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", verifier),
    ];
    if let Some(s) = client_secret {
        form.push(("client_secret", s));
    }
    post_token(http, token_endpoint, form, guard).await
}

pub async fn refresh_grant(
    http: &reqwest::Client,
    token_endpoint: &str,
    refresh_token: &str,
    client_id: &str,
    client_secret: Option<&str>,
    guard: Guard,
) -> Result<TokenGrant, String> {
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ];
    if let Some(s) = client_secret {
        form.push(("client_secret", s));
    }
    post_token(http, token_endpoint, form, guard).await
}

/// prepare_login 的产出：授权 URL 已可展示/开浏览器；finish_login 消费本结构完成换票。
pub struct LoginSession {
    pub server: String,
    pub authorize_url: String,
    pub callback_path: String,
    pub expected_state: String,
    pub redirect_uri: String,
    pub verifier: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub token_endpoint: String,
    pub listener: tokio::net::TcpListener,
    /// prepare 时的守卫（测试 Bypassed 过 loopback mock）；finish 的换票沿用同一档
    pub guard: Guard,
}

/// discovery -> (DCR) -> 绑回调 -> PKCE 授权 URL。config 有 client_id 时跳过动态注册。
pub async fn prepare_login(cfg: &RemoteConfig, guard: Guard) -> Result<LoginSession, String> {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let oauth = cfg.oauth.clone().unwrap_or_default();
    let meta: AuthServerMeta =
        discover(&http, &cfg.url, oauth.auth_server_metadata_url.as_deref(), guard).await?;
    let (listener, port) = bind_callback(oauth.callback_port).await?;
    let callback_path = format!("/callback/{}", callback_id(&cfg.url));
    let redirect_uri = format!("http://127.0.0.1:{port}{callback_path}");
    let (client_id, client_secret) = match oauth.client_id {
        Some(id) => (id, oauth.client_secret),
        None => register(&http, &meta, &redirect_uri, guard).await?,
    };
    let pkce = pkce();
    let state = random_state();
    let url = authorize_url(
        &meta,
        &client_id,
        &redirect_uri,
        &state,
        &pkce.challenge,
        oauth.scopes.as_deref(),
    )?;
    Ok(LoginSession {
        server: cfg.name.clone(),
        authorize_url: url,
        callback_path,
        expected_state: state,
        redirect_uri,
        verifier: pkce.verifier,
        client_id,
        client_secret,
        token_endpoint: meta.token_endpoint,
        listener,
        guard,
    })
}

/// 等回调 -> 验 state -> 换 token -> 落盘。state 不符直接拒（防 CSRF 混流）。
pub async fn finish_login(
    session: &LoginSession,
    store: &TokenStore,
) -> Result<TokenGrant, String> {
    let cb = wait_callback(&session.listener, &session.callback_path, CALLBACK_TIMEOUT).await?;
    if let Some(err) = cb.error {
        let desc = cb.error_description.unwrap_or_default();
        return Err(format!("oauth 授权被拒: {err} {desc}"));
    }
    if cb.state.as_deref() != Some(session.expected_state.as_str()) {
        return Err("oauth 回调 state 不匹配（疑似跨流混淆，已丢弃）".into());
    }
    let code = cb.code.ok_or("oauth 回调缺 code")?;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let grant = exchange_code(
        &http,
        &session.token_endpoint,
        &code,
        &session.redirect_uri,
        &session.client_id,
        session.client_secret.as_deref(),
        &session.verifier,
        session.guard,
    )
    .await?;
    store.save(&session.server, session, &grant)?;
    Ok(grant)
}
