use serde_json::Value;

pub(super) fn tool_result(response: &Value) -> Result<String, String> {
    let content =
        response.pointer("/result/content").and_then(Value::as_array).ok_or("tools/call response missing result.content array")?;
    let mut rendered = String::new();
    for item in content {
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        match item.get("type").and_then(Value::as_str) {
            Some("text") => rendered
                .push_str(item.get("text").and_then(Value::as_str).ok_or_else(|| "tools/call text content missing text".to_string())?),
            _ => rendered.push_str(&serde_json::to_string(item).map_err(|error| format!("serialize tools/call content: {error}"))?),
        }
    }
    let rendered = if rendered.is_empty() { "(empty result)".to_string() } else { rendered };
    if response.pointer("/result/isError").and_then(Value::as_bool).unwrap_or(false) {
        Err(format!("tools/call failed: {rendered}"))
    } else {
        Ok(rendered)
    }
}

pub(super) fn resource_result(response: &Value) -> Result<String, String> {
    let contents =
        response.pointer("/result/contents").and_then(Value::as_array).ok_or("resources/read response missing result.contents array")?;
    let mut rendered = String::new();
    for item in contents {
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        if let Some(text) = item.get("text").and_then(Value::as_str) {
            rendered.push_str(text);
        } else if item.get("blob").is_some() {
            rendered.push_str("[binary resource content omitted]");
        } else {
            rendered.push_str(&serde_json::to_string(item).map_err(|error| format!("serialize resources/read content: {error}"))?);
        }
    }
    Ok(if rendered.is_empty() { "(empty resource)".into() } else { rendered })
}
