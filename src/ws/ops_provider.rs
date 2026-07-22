//! provider 域 RPC：verify / accounts / 多账号导入删除 / 自定义提供商 CRUD / reprobe。

use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use super::ops::read_toml;
use crate::AppState;

pub(super) const METHODS: &[&str] = &[
    "provider.verify",
    "provider.reprobe",
    "provider.import_account",
    "provider.remove_account",
    "provider.add_custom",
    "provider.remove_custom",
    "provider.accounts",
];

pub(super) async fn handle(method: &str, params: &Value, app: &AppHandle) -> Result<Value, String> {
    match method {
        "provider.verify" => {
            let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
            let account = params.get("account").and_then(Value::as_str);
            let model = params.get("model").and_then(Value::as_str);
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?.clone();
            serde_json::to_value(kxen_app::llm::verify::verify_provider(&store, provider, account, model).await).map_err(|e| e.to_string())
        }
        "provider.accounts" => {
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?.clone();
            let mut out: Vec<Value> = ["anthropic", "openai", "xai", "kimi-for-coding"]
                .iter()
                .flat_map(|p| {
                    kxen_app::auth::credential::accounts_of(&store, p).into_iter().map(|key| {
                        let expired = store.get(&key).is_some_and(|c| c.is_expired());
                        json!({ "provider": p, "account": key.strip_prefix(&format!("{p}:")).map(String::from).unwrap_or_else(|| "default".to_string()), "id": key, "expired": expired })
                    }).collect::<Vec<_>>()
                })
                .collect();
            let cfg = kxen_app::core::config::Config::load(&kxen_app::core::paths::config_dir().join("config.toml"), None).unwrap_or_default();
            for (name, def) in &cfg.custom_providers {
                let id = format!("custom:{name}");
                out.push(json!({ "provider": id, "account": "default", "id": id, "expired": false, "custom": true, "base_url": def.base_url, "models": def.models, "protocol": def.protocol, "capabilities": def.capabilities }));
            }
            Ok(json!(out))
        }
        "provider.import_account" => {
            let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
            let account = params.get("account").and_then(Value::as_str).ok_or("missing account")?;
            let kind = params.get("kind").and_then(Value::as_str).unwrap_or("oauth");
            let access = params.get("access").and_then(Value::as_str).ok_or("missing access token")?;
            let key = kxen_app::auth::credential::account_id(provider, account);
            let state = app.state::<Arc<AppState>>();
            let mut store = state.auth_store.lock().map_err(|e| e.to_string())?;
            let cred = if kind == "api" {
                kxen_app::auth::credential::CredentialKind::Api { key: access.to_string() }
            } else {
                kxen_app::auth::credential::CredentialKind::Oauth {
                    access: access.to_string(),
                    refresh: params.get("refresh").and_then(Value::as_str).unwrap_or("").to_string(),
                    expires: params.get("expires").and_then(Value::as_u64).unwrap_or(0),
                    account_id: params.get("account_id").and_then(Value::as_str).map(String::from),
                }
            };
            store.insert(key.clone(), cred);
            kxen_app::auth::credential::write_auth_file(&kxen_app::core::paths::auth_file(), &store).map_err(|e| e.to_string())?;
            Ok(json!({ "id": key }))
        }
        "provider.remove_account" => {
            let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
            let account = params.get("account").and_then(Value::as_str).ok_or("missing account")?;
            if account == "default" {
                return Err("默认账号由官方 CLI 导入管理，不可在此删除".into());
            }
            let key = kxen_app::auth::credential::account_id(provider, account);
            let state = app.state::<Arc<AppState>>();
            let mut store = state.auth_store.lock().map_err(|e| e.to_string())?;
            if store.remove(&key).is_none() {
                return Err(format!("账号不存在: {key}"));
            }
            kxen_app::auth::credential::write_auth_file(&kxen_app::core::paths::auth_file(), &store).map_err(|e| e.to_string())?;
            Ok(json!({ "removed": key }))
        }
        "provider.add_custom" => {
            let name = params.get("name").and_then(Value::as_str).ok_or("missing name")?;
            let base_url = params.get("base_url").and_then(Value::as_str).ok_or("missing base_url")?;
            if !(base_url.starts_with("https://") || base_url.starts_with("http://")) {
                return Err("base_url 必须带 https:// 或 http:// 协议头".into());
            }
            let models: Vec<String> = params
                .get("models")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            if models.is_empty() {
                return Err("models 至少一个".into());
            }
            let api_key = params.get("api_key").and_then(Value::as_str).ok_or("missing api_key")?;
            let protocol = params.get("protocol").and_then(Value::as_str).unwrap_or("openai");
            if !["openai", "anthropic"].contains(&protocol) {
                return Err("protocol 只支持 openai / anthropic".into());
            }
            let capabilities: Vec<String> = params
                .get("capabilities")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_else(|| vec!["text".into()]);
            let path = kxen_app::core::paths::config_dir().join("config.toml");
            let mut doc = read_toml(&path)?;
            let customs = doc.entry(String::from("custom_providers")).or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
            let table = customs.as_table_mut().ok_or("custom_providers is not a table")?;
            let mut def = toml::map::Map::new();
            def.insert("base_url".into(), toml::Value::String(base_url.into()));
            def.insert("models".into(), toml::Value::Array(models.into_iter().map(toml::Value::String).collect()));
            def.insert("protocol".into(), toml::Value::String(protocol.into()));
            def.insert("capabilities".into(), toml::Value::Array(capabilities.into_iter().map(toml::Value::String).collect()));
            table.insert(name.into(), toml::Value::Table(def));
            super::ops::write_toml(&path, &doc)?;
            let state = app.state::<Arc<AppState>>();
            let mut store = state.auth_store.lock().map_err(|e| e.to_string())?;
            store.insert(format!("custom:{name}"), kxen_app::auth::credential::CredentialKind::Api { key: api_key.to_string() });
            kxen_app::auth::credential::write_auth_file(&kxen_app::core::paths::auth_file(), &store).map_err(|e| e.to_string())?;
            Ok(json!({ "id": format!("custom:{name}") }))
        }
        "provider.remove_custom" => {
            let name = params.get("name").and_then(Value::as_str).ok_or("missing name")?;
            let path = kxen_app::core::paths::config_dir().join("config.toml");
            let mut doc = read_toml(&path)?;
            if let Some(toml::Value::Table(table)) = doc.get_mut("custom_providers") {
                table.remove(name);
            }
            super::ops::write_toml(&path, &doc)?;
            let state = app.state::<Arc<AppState>>();
            let mut store = state.auth_store.lock().map_err(|e| e.to_string())?;
            store.remove(&format!("custom:{name}"));
            kxen_app::auth::credential::write_auth_file(&kxen_app::core::paths::auth_file(), &store).map_err(|e| e.to_string())?;
            Ok(json!({ "removed": name }))
        }
        "provider.reprobe" => reprobe(app).await,
        other => Err(format!("unknown provider method: {other}")),
    }
}

async fn reprobe(app: &AppHandle) -> Result<Value, String> {
    let state = app.state::<Arc<AppState>>();
    let probed = tokio::task::spawn_blocking(|| {
        let path = kxen_app::core::paths::auth_file();
        let mut store = kxen_app::auth::credential::read_auth_file(&path);
        let outcomes = kxen_app::auth::probe_all(&mut store, true);
        let _ = kxen_app::auth::credential::write_auth_file(&path, &store);
        (store, outcomes)
    })
    .await
    .map_err(|e| e.to_string())?;
    let (store, outcomes) = probed;
    *state.auth_store.lock().map_err(|e| e.to_string())? = store.clone();
    let report = crate::doctor::doctor_report(&store);
    Ok(json!({ "report": report, "outcomes": outcomes.iter().map(|(p, o, _)| format!("{p}: {o:?}")).collect::<Vec<_>>() }))
}
