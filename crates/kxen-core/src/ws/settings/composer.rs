use serde_json::{Value, json};
use std::sync::Arc;

use crate::AppState;

pub(in crate::ws) fn set_composer_suggestions(params: &Value, state: &Arc<AppState>) -> Result<Value, String> {
    let key = params.get("key").and_then(Value::as_str).ok_or("missing key")?;
    if !matches!(key, "enabled" | "semantic" | "llm") {
        return Err("unknown composer suggestion setting".into());
    }
    let enabled = params.get("enabled").and_then(Value::as_bool).ok_or("missing enabled")?;
    let path = kxen_core::core::paths::config_dir().join("config.toml");
    super::super::ops::update_toml_with_runtime(&path, &state.workspace_runtimes, |doc| {
        let section = doc.entry("composer_suggestions").or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if !section.is_table() {
            *section = toml::Value::Table(toml::Table::new());
        }
        section.as_table_mut().ok_or("composer_suggestions is not a table")?.insert(key.into(), toml::Value::Boolean(enabled));
        Ok(())
    })?;
    Ok(json!({ "key": key, "enabled": enabled }))
}

pub(in crate::ws) fn set_embedding(params: &Value, state: &Arc<AppState>) -> Result<Value, String> {
    let provider = params.get("provider").and_then(Value::as_str).unwrap_or("");
    if !matches!(provider, "" | "openai" | "openrouter" | "ollama") {
        return Err("embedding provider must be empty, openai, openrouter, or ollama".into());
    }
    let model = params.get("model").and_then(Value::as_str).unwrap_or("");
    let base_url = params.get("base_url").and_then(Value::as_str).unwrap_or("");
    let path = kxen_core::core::paths::config_dir().join("config.toml");
    super::super::ops::update_toml_with_runtime(&path, &state.workspace_runtimes, |doc| {
        let section = doc.entry("embedding").or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if !section.is_table() {
            *section = toml::Value::Table(toml::Table::new());
        }
        let table = section.as_table_mut().ok_or("embedding is not a table")?;
        table.insert("provider".into(), toml::Value::String(provider.into()));
        table.insert("model".into(), toml::Value::String(model.into()));
        table.insert("base_url".into(), toml::Value::String(base_url.into()));
        Ok(())
    })?;
    Ok(json!({ "provider": provider, "model": model, "base_url": base_url }))
}
