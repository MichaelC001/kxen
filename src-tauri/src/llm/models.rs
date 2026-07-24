//! 端点模型清单拉取：自定义双协议 + openai/xAI（api-key 或 OAuth Bearer）。
//! 订阅型官方端点不保证支持，失败由调用方静默回退手填。

use crate::auth::credential::AuthStore;

pub struct ModelsOutcome {
    pub models: Vec<String>,
    pub source: String,
    pub detail: String,
}

fn bearer_of(store: &AuthStore, provider: &str, account: Option<&str>) -> Option<String> {
    crate::auth::credential::credential_for(store, provider, account).map(|c| c.bearer().to_string())
}

/// GET {base}/models（openai 形态）或 {base}/v1/models（anthropic 形态），解析 data[].id。
pub async fn fetch_models(store: &AuthStore, provider: &str, account: Option<&str>, timeout_s: u64) -> ModelsOutcome {
    let (url, api_key_header) = if let Some(name) = provider.strip_prefix("custom:") {
        let cfg = crate::core::config::Config::load(&crate::core::paths::config_dir().join("config.toml"), None).unwrap_or_default();
        let Some(def) = cfg.custom_providers.get(name) else {
            return ModelsOutcome { models: vec![], source: "error".into(), detail: format!("custom provider not configured: {name}") };
        };
        let root = def.base_url.trim_end_matches('/');
        if def.protocol == "anthropic" { (format!("{root}/v1/models"), true) } else { (format!("{root}/models"), false) }
    } else {
        let Some(spec) = crate::providers::find(provider) else {
            return ModelsOutcome {
                models: vec![], source: "unsupported".into(), detail: format!("{provider} 订阅端点不支持 /models")
            };
        };
        let region = crate::auth::credential::credential_for(store, provider, account).and_then(|c| c.region());
        match spec.models_url(region) {
            Some(url) => (url, matches!(spec.protocol, crate::providers::Protocol::Anthropic)),
            None => {
                return ModelsOutcome {
                    models: vec![],
                    source: "unsupported".into(),
                    detail: format!("{provider} 端点未暴露 /models（用内置目录）"),
                };
            }
        }
    };
    // 本地免鉴权端点（ollama）无凭证要求，其余必须有凭证
    let local_free = crate::providers::find(provider).is_some_and(|s| s.auth == crate::providers::AuthKind::LocalFree);
    let bearer = bearer_of(store, provider, account);
    if !local_free && bearer.is_none() {
        return ModelsOutcome { models: vec![], source: "error".into(), detail: "无凭证".into() };
    }
    let mut req = crate::llm::client::shared_http().get(&url).timeout(std::time::Duration::from_secs(timeout_s));
    req = match (api_key_header, &bearer) {
        (true, Some(b)) => req.header("x-api-key", b).header("anthropic-version", "2023-06-01"),
        (false, Some(b)) => req.bearer_auth(b),
        _ => req,
    };
    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = match resp.json().await {
                Ok(v) => v,
                Err(e) => return ModelsOutcome { models: vec![], source: "error".into(), detail: format!("响应解析失败: {e}") },
            };
            let models = body
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from)).collect::<Vec<_>>())
                .unwrap_or_default();
            if models.is_empty() {
                ModelsOutcome { models, source: "error".into(), detail: "清单为空（端点不兼容）".into() }
            } else {
                ModelsOutcome { models, source: "endpoint".into(), detail: String::new() }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            ModelsOutcome {
                models: vec![],
                source: "error".into(),
                detail: format!("HTTP {status}: {}", &body[..body.floor_char_boundary(200)]),
            }
        }
        Err(e) => ModelsOutcome { models: vec![], source: "error".into(), detail: format!("请求失败: {e}") },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// 一次性 mock HTTP server：返回固定 /models JSON。
    fn mock_server(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(resp.as_bytes()).unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn parses_openai_shape() {
        let base = mock_server(r#"{"data":[{"id":"m1"},{"id":"m2"},{"no_id":true}]}"#);
        let mut store = AuthStore::new();
        store.insert("custom:t".into(), crate::auth::credential::CredentialKind::Api { key: "k".into(), region: None });
        // 直接测内部路径：手工构造同形状请求
        let resp = crate::llm::client::shared_http().get(format!("{base}/models")).bearer_auth("k").send().await.unwrap();
        let v: serde_json::Value = resp.json().await.unwrap();
        let models = v
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| arr.iter().filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from)).collect::<Vec<_>>())
            .unwrap_or_default();
        assert_eq!(models, vec!["m1", "m2"]);
    }
}
