//! GCP service account JSON -> OAuth2 access token：RS256 JWT 断言换 token（jwt-bearer grant）。
//! 签名用 ring（rustls 传递引入），不引 jsonwebtoken/rsa。token 进程内缓存，提前 5 分钟视为过期
//! （同 auth/refresh.rs 的 BUFFER 窗口），不落盘。

use base64::Engine;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_LOCATION: &str = "us-central1";
const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
/// 提前过期窗口（同 refresh.rs 的 5 分钟 buffer）
const BUFFER_MS: u64 = 5 * 60 * 1000;
/// 断言有效期 1 小时（GCP 上限）
const ASSERTION_TTL_SECS: u64 = 3600;

/// auth.json Api key 槽里的 service account 文档；location 是 GCP 导出格式没有的附加键（可选）。
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ServiceAccount {
    pub client_email: String,
    pub private_key: String,
    pub project_id: String,
    #[serde(default)]
    pub token_uri: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
}

impl ServiceAccount {
    pub(crate) fn token_uri(&self) -> &str {
        self.token_uri.as_deref().filter(|uri| !uri.trim().is_empty()).unwrap_or(DEFAULT_TOKEN_URI)
    }

    pub(crate) fn location(&self) -> &str {
        self.location.as_deref().filter(|loc| !loc.trim().is_empty()).unwrap_or(DEFAULT_LOCATION)
    }
}

pub(crate) fn parse_service_account(raw: &str) -> Result<ServiceAccount, String> {
    let account: ServiceAccount = serde_json::from_str(raw)
        .map_err(|_| "google-vertex credential must be a service account JSON (client_email / private_key / project_id)".to_string())?;
    if account.client_email.trim().is_empty() || account.project_id.trim().is_empty() {
        return Err("google-vertex credential requires client_email and project_id".into());
    }
    Ok(account)
}

/// PEM（PKCS#8 "BEGIN PRIVATE KEY"）-> DER。
fn pem_der(pem: &str) -> Result<Vec<u8>, String> {
    let body: String = pem.lines().filter(|line| !line.starts_with("-----")).map(str::trim).collect();
    base64::engine::general_purpose::STANDARD.decode(body).map_err(|error| format!("private_key is not valid PEM base64: {error}"))
}

/// RS256 JWT 断言（纯函数，单测钉死 iat/exp）。PKCS#1 v1.5 是确定性签名，输出可复现。
fn build_assertion(account: &ServiceAccount, iat_secs: u64, exp_secs: u64) -> Result<String, String> {
    let b64 = |bytes: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let header = b64(r#"{"alg":"RS256","typ":"JWT"}"#.as_bytes());
    let claims = b64(serde_json::json!({
        "iss": account.client_email,
        "scope": SCOPE,
        "aud": account.token_uri(),
        "iat": iat_secs,
        "exp": exp_secs,
    })
    .to_string()
    .as_bytes());
    let input = format!("{header}.{claims}");
    let der = pem_der(&account.private_key)?;
    let key = ring::signature::RsaKeyPair::from_pkcs8(&der).map_err(|_| "private_key is not a valid PKCS#8 RSA key".to_string())?;
    let rng = ring::rand::SystemRandom::new();
    let mut signature = vec![0u8; key.public().modulus_len()];
    key.sign(&ring::signature::RSA_PKCS1_SHA256, &rng, input.as_bytes(), &mut signature)
        .map_err(|_| "failed to sign jwt assertion".to_string())?;
    Ok(format!("{input}.{}", b64(&signature)))
}

struct CachedToken {
    token: String,
    expires_ms: u64,
}

fn cache() -> &'static Mutex<HashMap<String, CachedToken>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedToken>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 缓存命中（含 buffer 窗口）直接返回；否则走 jwt-bearer grant 换新并缓存。
pub(crate) async fn access_token(http: &reqwest::Client, account: &ServiceAccount) -> Result<String, String> {
    let cache_key = format!("{}|{}", account.client_email, account.token_uri());
    let now = crate::core::shared::now_ms();
    if let Some(cached) = crate::core::shared::lock(cache()).get(&cache_key)
        && cached.expires_ms > now + BUFFER_MS
    {
        return Ok(cached.token.clone());
    }
    let iat = now / 1000;
    let assertion = build_assertion(account, iat, iat + ASSERTION_TTL_SECS)?;
    let response = http
        .post(account.token_uri())
        .form(&[("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"), ("assertion", assertion.as_str())])
        .send()
        .await
        .map_err(|error| {
            format!("google-vertex token request failed: {}", crate::core::net_security::sanitize_authenticated_error(&error, &[]))
        })?;
    if !response.status().is_success() {
        return Err(crate::llm::client::bounded_http_error("google-vertex", response, &[&account.private_key]).await);
    }
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        expires_in: Option<u64>,
    }
    let body: TokenResponse = crate::net_response::json(response, crate::net_response::JSON_BODY_LIMIT, "vertex token")
        .await
        .map_err(|error| format!("google-vertex token response parse failed: {error}"))?;
    let expires_ms = now + body.expires_in.unwrap_or(ASSERTION_TTL_SECS) * 1000;
    crate::core::shared::lock(cache()).insert(cache_key, CachedToken { token: body.access_token.clone(), expires_ms });
    Ok(body.access_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 测试专用 fixture key（openssl genrsa 2048 | openssl pkcs8 -topk8 -nocrypt），非真实凭证
    const TEST_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDfvblPW37Ti4Tc\nk+kAu30lOvS/7KJqdI0u8JhkA/V+qV0y7QCgReDGIlXcQJPqquhEGHTx1LKoXURC\nr09OtH5R+0HqvAVgtBmM5/Eyp5DLjC/k111pae4ehhZBAjVp+Ldb7GuAlwhPxL3P\n3Yww3aDoiSUzAaHP9yLh57Sf7J3lbsE562NQ4ka9ZTLnLP5ZqGJ1I5q5QaelIozV\nqKQ4SotcRRLgVTBlmaeHN42d71SpJFj+JCYugUzCyS++1chvjsVn2cAXwcsI1V5i\n/798RRCyEsHdpJ+dkQ3QytHpIBTXXajUyj1mqTgYWdvM7qWnfkaarPHpADosg0s7\nkVGwLcvNAgMBAAECggEABnvaoVCAqspVEO9YX6UPScNQatKfG+an9yXekXqfF3Fh\nBtct0kQA8SNdlmk1eb92+HVpx8UaLrbezry116bZyLAEIQFOrP0kugI4T/d7KR4b\nsgE4iJPnsQrBtyPEchjuRCoXYD3NB9xaHWpi/aeIJwHFo8zDghTvXD/F6qLr9t7i\nhBJMmGY66dUIwUevVKsTdIqRE0KiOzKm3YvMykCVabqH4jiBFOG+trxJzs1Waj3B\n6rVq0IEGYJIaGCzTAo3Gpzlk6wJxj1MUByOiLwus/KAS05Av3vlsX0yGKvXZifCi\nSVwkiZOYc6uY9IEiuY3pyiLE8SkdgsM/ID5WHTE8IQKBgQD1Z/FymK7RNWb4uzyc\nMf8vKVIymNuwAo4dNeFDOLyDEYg6o8d1HAAwFZQYJbWLjPRRRKUKRWTxAHAntrwE\nISJV95D8c5j/xZtlIV1GyLSXnHk5kgQivOnK1q10ATfqmWOPeE6aDl8kW0RO7Xwx\nZHzazvVqMUWQpRyBFkFfaJ7yDwKBgQDpZlrm3cctfTd2WuVNS5Gs/Elsu5yne4hL\ny9j274SWibiHlSRBWudFIMn9km/cyvb21urYr6gMLXiXXFuMUOfZ89Yd2DjI3hlj\nESeTuvhQSD89nWQRlknbW7E59WJGTrjUFAOT6I30kMHk2jRx9UdHV0mpsfPHMzAv\nr+YO61bQYwKBgHYsiql7etuF0sM+Ls6siMzmIg35a/kTEephEsuzG5wmKirKyXbM\neA6vpXQHXKgJxXEJFEyg5B3l7xxAs8NtTUKGh8v5wpiQSOdnLKq0ZbqIgBvIA7PS\nsuaZgEdXety+5qGk9nzbJNe2F2vGksKaOEGJ3cY4Wd5wlAxZcjuGftvjAoGBALxs\ncX5oFOBYcmoOP4gDNfvdQLmTTIi5ZfMrAmF4RCXz0RFEChzo8kQQkIExszUgdfgY\n0UdVp+kM3In1ecLxnfuKqpU1dcJT61LbCoRtMQI/ES8A3USFe+KnR5Bu1YPFEdnE\nvo6t88w5AQ9sSWKmjYC+qy1gCFoMrR4SdzwcKd3ZAoGBANNhIEI5AJD3V2v27xDb\n6AessJeBjbXA7ileamaCIDEKpuLFuHDIVuSiAikdgPaOwLrsiaZQpwpVd5sFsOFy\nQ2IgkYgmT7ZnvS+AdK6+a6U5pOgmx76LYv0jK1FGGB3blsx58VtbYGVs8gRdd1ly\nuaZoSAvJ8nlLnQV2AX89s4mp\n-----END PRIVATE KEY-----";

    fn account() -> ServiceAccount {
        parse_service_account(
            &serde_json::json!({
                "client_email": "sa@proj.iam.gserviceaccount.com",
                "private_key": TEST_KEY,
                "project_id": "proj-1",
            })
            .to_string(),
        )
        .unwrap()
    }

    #[test]
    fn parse_requires_core_fields_and_defaults() {
        let account = account();
        assert_eq!(account.token_uri(), DEFAULT_TOKEN_URI);
        assert_eq!(account.location(), DEFAULT_LOCATION);
        for bad in ["not-json", r#"{"client_email":"a@b","private_key":"k"}"#, r#"{"client_email":" ","private_key":"k","project_id":"p"}"#]
        {
            assert!(parse_service_account(bad).is_err(), "{bad}");
        }
        let custom = parse_service_account(
            &serde_json::json!({
                "client_email": "a@b", "private_key": "k", "project_id": "p",
                "token_uri": "https://oauth2.example.test/token", "location": "europe-west4",
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(custom.token_uri(), "https://oauth2.example.test/token");
        assert_eq!(custom.location(), "europe-west4");
    }

    #[test]
    fn assertion_is_signed_rs256_with_service_account_claims() {
        let account = account();
        let assertion = build_assertion(&account, 1_700_000_000, 1_700_003_600).unwrap();
        let parts: Vec<&str> = assertion.split('.').collect();
        assert_eq!(parts.len(), 3);
        let decode = |part: &str| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(part)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .unwrap()
        };
        assert_eq!(decode(parts[0]), serde_json::json!({"alg": "RS256", "typ": "JWT"}));
        let claims = decode(parts[1]);
        assert_eq!(claims["iss"], "sa@proj.iam.gserviceaccount.com");
        assert_eq!(claims["scope"], SCOPE);
        assert_eq!(claims["aud"], DEFAULT_TOKEN_URI);
        assert_eq!(claims["iat"], 1_700_000_000);
        assert_eq!(claims["exp"], 1_700_003_600);
        // 签名必须能被同一把 key 的公钥验证（PKCS#1 v1.5 确定性，验签即钉死答案）
        let der = pem_der(TEST_KEY).unwrap();
        let key = ring::signature::RsaKeyPair::from_pkcs8(&der).unwrap();
        let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        let verifier = ring::signature::UnparsedPublicKey::new(&ring::signature::RSA_PKCS1_2048_8192_SHA256, key.public().as_ref());
        let input = format!("{}.{}", parts[0], parts[1]);
        verifier.verify(input.as_bytes(), &signature).expect("assertion signature must verify");
        // 确定性签名：同输入重签必须逐字节一致
        assert_eq!(build_assertion(&account, 1_700_000_000, 1_700_003_600).unwrap(), assertion);
    }

    #[test]
    fn bad_private_key_is_a_local_error() {
        let mut account = account();
        account.private_key = "not-a-pem".into();
        assert!(build_assertion(&account, 0, 1).unwrap_err().contains("PEM"));
        account.private_key = "-----BEGIN PRIVATE KEY-----\naGVsbG8=\n-----END PRIVATE KEY-----".into();
        assert!(build_assertion(&account, 0, 1).unwrap_err().contains("PKCS#8"));
    }
}
