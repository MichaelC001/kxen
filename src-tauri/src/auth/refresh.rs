//! OAuth 主动刷新：快过期走 refresh grant 换新并落盘。
//! 端点契约（多源核实）：anthropic = console.anthropic.com / client_id 9d1c250a（Claude Code 公开值），
//! openai = auth.openai.com / client_id app_EMoamEEZ73f0CkXaXp7hrann（Codex CLI 公开值）。
//! Anthropic 刷新即吊销旧 refresh token：RECENT 跨 clone 去重，绝不重复刷新同一旧凭证。

use crate::auth::credential::{account_id, AuthStore, CredentialKind};
use std::sync::{Mutex, OnceLock};

const BUFFER_MS: u64 = 5 * 60 * 1000;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn token_endpoint(provider: &str) -> Option<(&'static str, &'static str)> {
    match provider {
        "anthropic" => Some(("https://console.anthropic.com/v1/oauth/token", "9d1c250a-e61b-44d9-88ed-5944d1962f5e")),
        "openai" => Some(("https://auth.openai.com/oauth/token", "app_EMoamEEZ73f0CkXaXp7hrann")),
        _ => None, // xai/kimi 无公开刷新端点（官方 CLI 托管）
    }
}

#[derive(Debug, serde::Deserialize)]
struct RefreshResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
}

static REFRESH_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static RECENT: OnceLock<Mutex<std::collections::HashMap<String, CredentialKind>>> = OnceLock::new();

fn recent() -> &'static Mutex<std::collections::HashMap<String, CredentialKind>> {
    RECENT.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// 快过期则刷新（store 更新返回 true）；无凭证/端点不支持/失败保持原样返回 false。
pub async fn ensure_fresh(store: &mut AuthStore, provider: &str, account: Option<&str>) -> bool {
    let Some((url, client_id)) = token_endpoint(provider) else { return false };
    let key = account.map(|a| account_id(provider, a)).unwrap_or_else(|| provider.to_string());
    let Some(cred) = store.get(&key).cloned() else { return false };
    let CredentialKind::Oauth { refresh, account_id: acc_id, .. } = &cred else { return false };
    if refresh.is_empty() || !cred.is_expired_within(BUFFER_MS) {
        return false;
    }
    // 其它 clone 刚刷过：直接采用（旧 refresh 已吊销，再刷必败）
    if let Some(fresh) = recent().lock().expect("recent").get(&key).cloned() {
        if !fresh.is_expired_within(BUFFER_MS) {
            store.insert(key.clone(), fresh);
            let _ = crate::auth::credential::write_auth_file(&crate::core::paths::auth_file(), store);
            return true;
        }
    }
    let _guard = REFRESH_LOCK.get_or_init(|| tokio::sync::Mutex::new(())).lock().await;
    // 锁内复查：等待期间可能已被另一 run 刷新
    let current = store.get(&key).cloned();
    if current.as_ref().is_some_and(|c| !c.is_expired_within(BUFFER_MS)) {
        return false;
    }
    if let Some(fresh) = recent().lock().expect("recent").get(&key).cloned() {
        if !fresh.is_expired_within(BUFFER_MS) {
            store.insert(key.clone(), fresh);
            let _ = crate::auth::credential::write_auth_file(&crate::core::paths::auth_file(), store);
            return true;
        }
    }
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh,
        "client_id": client_id,
    });
    let resp = crate::llm::client::shared_http()
        .post(url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await;
    let Ok(resp) = resp else { return false };
    if !resp.status().is_success() {
        tracing::warn!(provider, status = %resp.status(), "oauth refresh failed");
        return false;
    }
    let Ok(parsed) = resp.json::<RefreshResponse>().await else { return false };
    let new_cred = CredentialKind::Oauth {
        access: parsed.access_token,
        refresh: parsed.refresh_token.unwrap_or_else(|| refresh.clone()),
        expires: now_ms() + parsed.expires_in.unwrap_or(28_800) * 1000,
        account_id: acc_id.clone(),
    };
    recent().lock().expect("recent").insert(key.clone(), new_cred.clone());
    store.insert(key, new_cred);
    let _ = crate::auth::credential::write_auth_file(&crate::core::paths::auth_file(), store);
    tracing::info!(provider, "oauth token refreshed proactively");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_contract() {
        assert!(token_endpoint("anthropic").unwrap().0.contains("anthropic.com"));
        assert!(token_endpoint("openai").unwrap().0.contains("openai.com"));
        assert!(token_endpoint("xai").is_none());
    }

    #[test]
    fn api_key_never_refreshes() {
        let mut store = AuthStore::default();
        store.insert("openai".into(), CredentialKind::Api { key: "k".into(), region: None });
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        assert!(!rt.block_on(ensure_fresh(&mut store, "openai", None)));
    }

    #[test]
    fn unexpired_oauth_skips() {
        let mut store = AuthStore::default();
        store.insert(
            "anthropic".into(),
            CredentialKind::Oauth { access: "a".into(), refresh: "r".into(), expires: now_ms() + 3_600_000, account_id: None },
        );
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        assert!(!rt.block_on(ensure_fresh(&mut store, "anthropic", None)));
    }
}
