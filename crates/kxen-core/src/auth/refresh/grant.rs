use super::{AuthStore, CredentialKind, GrantStyle, RefreshResponse};

fn refresh_http() -> Result<reqwest::Client, String> {
    static CLIENT: std::sync::OnceLock<Result<reqwest::Client, String>> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            crate::tools::net_guard::guarded_client_builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|error| format!("create OAuth refresh client: {error}"))
        })
        .clone()
}

/// refresh grant 参数包：端点、客户端身份、body 风格与凭证槽。
/// client_secret 仅 Google 桌面公开凭证需要（exchange/refresh 都带）。
pub(super) struct GrantParams<'a> {
    pub url: &'a str,
    pub client_id: &'a str,
    pub client_secret: Option<&'a str>,
    pub style: GrantStyle,
    pub refresh: &'a str,
    pub acc_id: &'a Option<String>,
}

/// 执行 refresh grant：POST token 端点 -> 解析 -> 持久化 -> 发布内存。
pub(super) async fn run_grant(store: &mut AuthStore, key: &str, params: GrantParams<'_>) -> Result<(), String> {
    run_grant_to(store, key, params, &crate::core::paths::KxenPaths::user().auth_file()).await
}

pub(super) async fn run_grant_to(
    store: &mut AuthStore,
    key: &str,
    params: GrantParams<'_>,
    auth_file: &std::path::Path,
) -> Result<(), String> {
    let GrantParams { url, client_id, client_secret, style, refresh, acc_id } = params;
    let request = refresh_http()?.post(url).timeout(std::time::Duration::from_secs(15));
    let response = match style {
        GrantStyle::Json => {
            let mut body = serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh,
                "client_id": client_id,
            });
            if let Some(secret) = client_secret {
                body["client_secret"] = serde_json::json!(secret);
            }
            request.json(&body).send().await
        }
        GrantStyle::Form => {
            let mut form: Vec<(&str, &str)> = vec![("grant_type", "refresh_token"), ("refresh_token", refresh), ("client_id", client_id)];
            if let Some(secret) = client_secret {
                form.push(("client_secret", secret));
            }
            request.form(&form).send().await
        }
    }
    .map_err(|error| format!("OAuth refresh request failed: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        tracing::warn!(%status, "oauth refresh grant failed");
        return Err(format!("OAuth refresh endpoint returned HTTP {status}"));
    }
    let parsed = crate::net_response::json::<RefreshResponse>(response, crate::net_response::JSON_BODY_LIMIT, "OAuth refresh response")
        .await
        .map_err(|error| format!("OAuth refresh response was invalid: {error}"))?;
    apply_refresh_to(store, key, parsed, refresh, acc_id.as_deref(), auth_file).map_err(|error| {
        tracing::error!(%error, "oauth credential persistence failed");
        format!("OAuth refreshed credential could not be persisted: {error}")
    })?;
    Ok(())
}

/// Copilot 刷新：refresh 槽的 GitHub OAuth token 重新换短命 Copilot JWT（无二方 refresh grant）。
pub(super) async fn run_copilot_exchange(store: &mut AuthStore, key: &str, github_token: &str) -> Result<(), String> {
    let (jwt, expires_at) = crate::auth::oauth_login::copilot_exchange_token(github_token).await?;
    let now_secs = crate::core::shared::now_ms() / 1000;
    let parsed = RefreshResponse { access_token: jwt, refresh_token: None, expires_in: Some(expires_at.saturating_sub(now_secs)) };
    apply_refresh_to(store, key, parsed, github_token, None, &crate::core::paths::KxenPaths::user().auth_file()).map_err(|error| {
        tracing::error!(%error, "copilot credential persistence failed");
        format!("Copilot refreshed credential could not be persisted: {error}")
    })
}

/// grant 响应先持久化，再发布到 RECENT 与各内存 store。rename 前失败不发布；
/// rename 后目录 sync 失败时，新凭证已可见，必须发布并向调用方报告持久性不确定。
pub(super) fn apply_refresh_to(
    store: &mut AuthStore,
    key: &str,
    parsed: RefreshResponse,
    old_refresh: &str,
    acc_id: Option<&str>,
    auth_file: &std::path::Path,
) -> crate::core::Result<()> {
    if parsed.access_token.trim().is_empty() {
        return Err(crate::core::Error::Custom("OAuth refresh response contained an empty access token".into()));
    }
    let refresh = parsed.refresh_token.filter(|value| !value.is_empty()).unwrap_or_else(|| old_refresh.to_string());
    let expires_in_ms = parsed.expires_in.unwrap_or(28_800).saturating_mul(1000);
    let new_cred = CredentialKind::Oauth {
        access: parsed.access_token,
        refresh,
        expires: crate::core::shared::now_ms().saturating_add(expires_in_ms),
        account_id: acc_id.map(str::to_string),
    };
    match crate::auth::credential::write_auth_entry_committed(auth_file, key, Some(&new_cred)) {
        Ok(()) => {}
        Err(failure) if failure.committed() => {
            publish_refresh(store, key, new_cred);
            return Err(crate::core::Error::Custom(failure.to_string()));
        }
        Err(failure) => return Err(crate::core::Error::Custom(failure.to_string())),
    }
    publish_refresh(store, key, new_cred);
    Ok(())
}

fn publish_refresh(store: &mut AuthStore, key: &str, credential: CredentialKind) {
    crate::core::shared::lock(super::recent()).insert(key.to_string(), credential.clone());
    crate::auth::shared_store::propagate(key, &credential);
    store.insert(key.to_string(), credential);
}

#[cfg(test)]
mod tests;
