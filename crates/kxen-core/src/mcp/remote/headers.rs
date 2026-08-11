use std::collections::HashMap;

/// 配置头必须可安全复制到每条 MCP 请求；传输层路由/成帧头不可被用户配置覆盖。
pub(crate) fn validate_headers(headers: &HashMap<String, String>) -> Result<Vec<(String, String)>, String> {
    let mut output = Vec::new();
    for (name, value) in headers {
        if ["accept", "content-type", "content-length", "host", "connection", "transfer-encoding", "mcp-session-id"]
            .iter()
            .any(|reserved| name.eq_ignore_ascii_case(reserved))
        {
            return Err(format!("reserved MCP transport header cannot be configured: {name}"));
        }
        reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|error| format!("invalid mcp header name {name}: {error}"))?;
        reqwest::header::HeaderValue::from_str(value).map_err(|error| format!("invalid mcp header value for {name}: {error}"))?;
        output.push((name.clone(), value.clone()));
    }
    Ok(output)
}
