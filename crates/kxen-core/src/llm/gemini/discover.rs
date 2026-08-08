//! project 发现：loadCodeAssist ->（新用户）onboardUser -> LRO 轮询，进程内缓存一次。

use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// onboardUser LRO 轮询：生产 5s x 12；测试压到 10ms 避免拖慢套件
#[cfg(not(test))]
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(test)]
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(10);
const POLL_ATTEMPTS: u32 = 12;

fn project_cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 缓存 key 用 token 的 sha256 前 16 hex，凭证本身不进内存 key。
fn cache_key(token: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(token.as_bytes());
    digest[..8].iter().map(|byte| format!("{byte:02x}")).collect()
}

fn cache_get(key: &str) -> Option<String> {
    project_cache().lock().ok()?.get(key).cloned()
}

fn cache_put(key: &str, project: &str) {
    if let Ok(mut cache) = project_cache().lock() {
        cache.insert(key.to_string(), project.to_string());
    }
}

/// cloudaicompanionProject 两种形态：裸 string 或 {"id": ...} object。
fn project_id_of(value: &Value) -> Option<String> {
    value.as_str().map(String::from).or_else(|| value.get("id").and_then(Value::as_str).map(String::from))
}

#[derive(Deserialize)]
struct LoadCodeAssistResponse {
    #[serde(rename = "cloudaicompanionProject")]
    project: Option<Value>,
    #[serde(rename = "currentTier")]
    current_tier: Option<Value>,
    #[serde(rename = "allowedTiers", default)]
    allowed_tiers: Vec<AllowedTier>,
}

#[derive(Deserialize)]
struct AllowedTier {
    id: Option<String>,
    #[serde(rename = "isDefault", default)]
    is_default: bool,
}

#[derive(Deserialize)]
struct Lro {
    name: Option<String>,
    #[serde(default)]
    done: bool,
    response: Option<Value>,
}

fn client_metadata(flavor: super::Flavor) -> Value {
    json!({"ideType": flavor.ide_type(), "platform": "PLATFORM_UNSPECIFIED", "pluginType": "GEMINI"})
}

/// 登录后/首用前发现 project id，带内存缓存（进程内一次）；flavor 决定身份头与 metadata。
pub async fn discover_project(http: &reqwest::Client, base: &str, token: &str, flavor: super::Flavor) -> Result<String, String> {
    let key = cache_key(token);
    if let Some(cached) = cache_get(&key) {
        return Ok(cached);
    }
    let base = base.trim_end_matches('/');
    let metadata = client_metadata(flavor);
    let resp = super::gemini_headers(http.post(format!("{base}/v1internal:loadCodeAssist")), token, flavor)
        .json(&json!({ "metadata": metadata }))
        .send()
        .await
        .map_err(|error| {
            format!("gemini loadCodeAssist failed: {}", crate::core::net_security::sanitize_authenticated_error(&error, &[token]))
        })?;
    if !resp.status().is_success() {
        return Err(crate::llm::client::bounded_http_error("gemini", resp, &[token]).await);
    }
    let body: LoadCodeAssistResponse =
        crate::net_response::json(resp, crate::net_response::JSON_BODY_LIMIT, "gemini loadCodeAssist").await?;
    if let Some(project) = body.project.as_ref().and_then(project_id_of) {
        cache_put(&key, &project);
        return Ok(project);
    }
    // 无 currentTier 但有默认 tier：新用户需先 onboardUser 领取项目
    let tier_id = match (body.current_tier, body.allowed_tiers.iter().find(|tier| tier.is_default).and_then(|tier| tier.id.clone())) {
        (None, Some(id)) => id,
        _ => return Err("gemini loadCodeAssist returned no project id".to_string()),
    };
    let project = onboard_user(http, base, token, &tier_id, &metadata, flavor).await?;
    cache_put(&key, &project);
    Ok(project)
}

async fn onboard_user(
    http: &reqwest::Client,
    base: &str,
    token: &str,
    tier_id: &str,
    metadata: &Value,
    flavor: super::Flavor,
) -> Result<String, String> {
    let resp = super::gemini_headers(http.post(format!("{base}/v1internal:onboardUser")), token, flavor)
        .json(&json!({ "tierId": tier_id, "metadata": metadata }))
        .send()
        .await
        .map_err(|error| {
            format!("gemini onboardUser failed: {}", crate::core::net_security::sanitize_authenticated_error(&error, &[token]))
        })?;
    if !resp.status().is_success() {
        return Err(crate::llm::client::bounded_http_error("gemini", resp, &[token]).await);
    }
    let mut lro: Lro = crate::net_response::json(resp, crate::net_response::JSON_BODY_LIMIT, "gemini onboardUser").await?;
    for _ in 0..POLL_ATTEMPTS {
        if lro.done {
            break;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
        let name = lro.name.clone().ok_or_else(|| "gemini onboardUser LRO missing operation name".to_string())?;
        let resp = super::gemini_headers(http.get(format!("{base}/v1internal/{name}")), token, flavor).send().await.map_err(|error| {
            format!("gemini LRO poll failed: {}", crate::core::net_security::sanitize_authenticated_error(&error, &[token]))
        })?;
        if !resp.status().is_success() {
            return Err(crate::llm::client::bounded_http_error("gemini", resp, &[token]).await);
        }
        lro = crate::net_response::json(resp, crate::net_response::JSON_BODY_LIMIT, "gemini LRO poll").await?;
    }
    if !lro.done {
        return Err("gemini onboardUser did not finish after polling".to_string());
    }
    lro.response
        .as_ref()
        .and_then(|response| response.get("cloudaicompanionProject"))
        .and_then(project_id_of)
        .ok_or_else(|| "gemini onboardUser finished without project id".to_string())
}

#[cfg(test)]
mod tests;
