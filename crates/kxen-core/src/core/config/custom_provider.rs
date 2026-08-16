use serde::{Deserialize, Serialize};

/// 自定义类型提供商：base_url + 模型清单 + 协议（openai|anthropic）+ 能力标记（text/vision/audio）。
/// api key 存 auth.json（custom:<name>）。query_params 是 per-request 查询参数（Azure OpenAI 的
/// api-version 等）：base_url 本身禁止携带 query（见 net_security），参数走这个类型化字段独立编码。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomProviderDef {
    pub base_url: String,
    pub models: Vec<String>,
    pub protocol: String,
    pub capabilities: Vec<String>,
    pub query_params: std::collections::BTreeMap<String, String>,
}

impl Default for CustomProviderDef {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            models: vec![],
            protocol: "openai".into(),
            capabilities: vec!["text".into()],
            query_params: Default::default(),
        }
    }
}

impl CustomProviderDef {
    /// 拼 chat/models 等端点：固定路径 + query_params 编码进 query（url crate 负责转义）。
    pub(crate) fn endpoint_url(&self, suffix: &str) -> Result<String, String> {
        let joined = crate::core::net_security::join_base_endpoint(&self.base_url, suffix)?;
        append_query_params(&joined, &self.query_params)
    }
}

/// 把 query_params 编码进已 join 好的 endpoint（url crate 负责转义）；空表原样返回。
pub(crate) fn append_query_params(endpoint: &str, query_params: &std::collections::BTreeMap<String, String>) -> Result<String, String> {
    if query_params.is_empty() {
        return Ok(endpoint.to_string());
    }
    let mut url = reqwest::Url::parse(endpoint).map_err(|_| "endpoint url 无效".to_string())?;
    url.query_pairs_mut().extend_pairs(query_params.iter());
    Ok(url.to_string())
}

/// query_params 键值规则：键非空且不含空白，值不含控制字符。
pub(crate) fn validate_query_params(query_params: &std::collections::BTreeMap<String, String>) -> Result<(), String> {
    for (key, value) in query_params {
        if key.trim().is_empty() {
            return Err("query_params 的键不能为空".into());
        }
        if key.chars().any(char::is_whitespace) {
            return Err(format!("query_params 键 {key:?} 不能含空白字符"));
        }
        if value.chars().any(|c| c.is_control()) {
            return Err(format!("query_params[{key}] 的值不能含控制字符"));
        }
    }
    Ok(())
}

/// base URL 必须能直接交给 reqwest 构造请求。携带 API key 的远程请求只允许
/// HTTPS；HTTP 只放行明确的 localhost 或 loopback IP，不发起 DNS 或网络连接。
pub fn validate_custom_provider_endpoint(base_url: &str) -> Result<(), String> {
    let url = crate::core::net_security::validate_base_endpoint(base_url)?;
    let host = url.host_str().ok_or("必须包含 host")?;
    let bare = host.strip_prefix('[').and_then(|value| value.strip_suffix(']')).unwrap_or(host);
    if let Ok(address) = bare.parse::<std::net::IpAddr>()
        && crate::tools::net_guard::is_blocked_ip(&address)
        && !is_loopback_host(host)
    {
        return Err("远程地址不能指向 private、link-local、CGNAT 或 unspecified IP".into());
    }
    if url.scheme() == "http" && !is_loopback_host(host) {
        return Err("远程地址必须使用 https://；http:// 仅允许 localhost 或 loopback IP".into());
    }
    Ok(())
}

pub(crate) fn validate_custom_provider_definition(definition: &CustomProviderDef) -> Result<(), String> {
    validate_custom_provider_endpoint(&definition.base_url).map_err(|error| format!("base_url {error}"))?;
    if !matches!(definition.protocol.as_str(), "openai" | "anthropic") {
        return Err("protocol must be openai or anthropic".into());
    }
    if definition.models.is_empty() {
        return Err("models must contain at least one model identity".into());
    }
    for (index, model) in definition.models.iter().enumerate() {
        crate::auth::credential::validate_identity(model, "model").map_err(|error| format!("models[{index}] {error}"))?;
    }
    if definition.capabilities.is_empty() {
        return Err("capabilities must contain at least one supported capability".into());
    }
    for (index, capability) in definition.capabilities.iter().enumerate() {
        if !matches!(capability.as_str(), "text" | "vision" | "audio") {
            return Err(format!("capabilities[{index}] must be text, vision, or audio"));
        }
    }
    validate_query_params(&definition.query_params)?;
    Ok(())
}

pub(crate) fn endpoint_is_explicit_loopback(base_url: &str) -> bool {
    reqwest::Url::parse(base_url).ok().and_then(|url| url.host_str().map(is_loopback_host)).unwrap_or(false)
}

fn is_loopback_host(host: &str) -> bool {
    let normalized = host.strip_suffix('.').unwrap_or(host);
    if normalized.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let host = normalized.strip_prefix('[').and_then(|value| value.strip_suffix(']')).unwrap_or(normalized);
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(address)) => address.is_loopback(),
        Ok(std::net::IpAddr::V6(address)) => address.is_loopback() || address.to_ipv4_mapped().is_some_and(|mapped| mapped.is_loopback()),
        Err(_) => false,
    }
}

/// 校验最终实际下发的鉴权 header，而不是只校验原始 key。
/// openai 兼容协议发送 Authorization: Bearer，anthropic 发送 x-api-key。
pub fn validate_custom_provider_auth(protocol: &str, api_key: &str) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("api_key 不能为空".into());
    }
    let (name, value) = match protocol {
        "openai" => ("authorization", format!("Bearer {api_key}")),
        "anthropic" => ("x-api-key", api_key.to_string()),
        _ => return Err("protocol 只支持 openai / anthropic".into()),
    };
    reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|error| format!("header name {name} 无效: {error}"))?;
    reqwest::header::HeaderValue::from_str(&value).map_err(|error| format!("header value for {name} 无效: {error}"))?;
    Ok(())
}

/// custom Provider 路由的端点定义。请求热路径保留配置解析错误，避免把坏配置
/// 误报成 Provider 不存在，同时继续复用 mtime cache。
pub(crate) fn custom_provider_def_checked(name: &str) -> Result<Option<CustomProviderDef>, String> {
    Ok(crate::core::config_cache::cached_user_config_result()?.custom_providers.get(name).cloned())
}

#[cfg(test)]
mod endpoint_tests {
    use super::{CustomProviderDef, validate_custom_provider_definition, validate_custom_provider_endpoint};

    #[test]
    fn rejects_private_ip_even_over_https() {
        for url in ["https://10.0.0.8/v1", "https://169.254.169.254/v1", "https://[fd00::1]/v1", "https://100.100.100.100/v1"] {
            let error = validate_custom_provider_endpoint(url).unwrap_err();
            assert!(error.contains("不能指向"), "{url}: {error}");
        }
    }

    fn azure_def() -> CustomProviderDef {
        CustomProviderDef {
            base_url: "https://myres.openai.azure.com/openai/deployments/gpt-4o".into(),
            models: vec!["gpt-4o".into()],
            query_params: [("api-version".to_string(), "2025-01-01-preview".to_string())].into_iter().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn endpoint_url_appends_query_params_after_path_join() {
        let def = azure_def();
        assert_eq!(
            def.endpoint_url("chat/completions").unwrap(),
            "https://myres.openai.azure.com/openai/deployments/gpt-4o/chat/completions?api-version=2025-01-01-preview"
        );
        assert_eq!(
            def.endpoint_url("models").unwrap(),
            "https://myres.openai.azure.com/openai/deployments/gpt-4o/models?api-version=2025-01-01-preview"
        );
    }

    #[test]
    fn endpoint_url_without_query_params_keeps_legacy_shape() {
        let def = CustomProviderDef { base_url: "https://api.example.com/v1".into(), ..Default::default() };
        assert_eq!(def.endpoint_url("chat/completions").unwrap(), "https://api.example.com/v1/chat/completions");
        // 存量 config（无 query_params 键）反序列化为空表，URL 逐字符不变
        let legacy: CustomProviderDef =
            toml::from_str("base_url = \"https://api.example.com/v1\"\nmodels = [\"m\"]\nprotocol = \"openai\"\n").unwrap();
        assert!(legacy.query_params.is_empty());
        assert_eq!(legacy.endpoint_url("chat/completions").unwrap(), "https://api.example.com/v1/chat/completions");
    }

    #[test]
    fn endpoint_url_encodes_param_values() {
        let def = CustomProviderDef {
            base_url: "https://api.example.com/v1".into(),
            query_params: [("q".to_string(), "a b&c".to_string())].into_iter().collect(),
            ..Default::default()
        };
        assert_eq!(def.endpoint_url("models").unwrap(), "https://api.example.com/v1/models?q=a+b%26c");
    }

    #[test]
    fn query_param_validation_rejects_bad_keys_and_control_values() {
        let mut def = azure_def();
        validate_custom_provider_definition(&def).expect("azure def valid");
        def.query_params.insert(" ".into(), "v".into());
        let error = validate_custom_provider_definition(&def).unwrap_err();
        assert!(error.contains("query_params"), "{error}");
        def.query_params.clear();
        def.query_params.insert("api-version".into(), "x\r\ny".into());
        let error = validate_custom_provider_definition(&def).unwrap_err();
        assert!(error.contains("query_params"), "{error}");
    }
}
