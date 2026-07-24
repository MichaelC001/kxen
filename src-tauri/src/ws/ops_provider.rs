//! provider 域 RPC：verify / accounts / 多账号导入删除 / 自定义提供商 CRUD / reprobe。

use serde_json::{Value, json};
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
    "provider.models",
    "provider.list",
    "models.catalog",
    "models.refresh",
];

pub(super) async fn handle(method: &str, params: &Value, app: &AppHandle) -> Result<Value, String> {
    match method {
        "provider.list" => {
            // 设置页 provider 下拉/区域选择的唯一数据源（前端不再硬编码清单）
            let out: Vec<Value> = kxen_app::providers::all()
                .iter()
                .map(|s| {
                    json!({
                        "key": s.key,
                        "display": s.display,
                        "protocol": s.protocol,
                        "auth": s.auth,
                        "regions": s.regions.iter().map(|r| json!({ "key": r.key, "display": r.display, "base_url": r.base_url })).collect::<Vec<_>>(),
                        "models_endpoint": s.models_endpoint,
                        "default_model": s.default_model,
                        "doc_url": s.doc_url,
                    })
                })
                .collect();
            Ok(json!(out))
        }
        "provider.verify" => {
            let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
            let account = params.get("account").and_then(Value::as_str);
            let model = params.get("model").and_then(Value::as_str);
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?.clone();
            serde_json::to_value(kxen_app::llm::verify::verify_provider(&store, provider, account, model).await).map_err(|e| e.to_string())
        }
        "provider.models" => {
            let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
            let account = params.get("account").and_then(Value::as_str);
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?.clone();
            let out = kxen_app::llm::models::fetch_models(&store, provider, account, 15).await;
            Ok(json!({ "models": out.models, "source": out.source, "detail": out.detail }))
        }
        "models.catalog" => {
            let snapshot = kxen_app::llm::catalog::catalog();
            Ok(serde_json::to_value(snapshot).map_err(|e| e.to_string())?)
        }
        "models.refresh" => {
            kxen_app::llm::catalog::refresh_async();
            Ok(json!({ "refreshing": true }))
        }
        "provider.accounts" => {
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?.clone();
            // registry 驱动（本地免鉴权的 ollama 无凭证概念，不进账号列表）；region 从凭证读出
            let mut out: Vec<Value> = kxen_app::providers::all()
                .iter()
                .filter(|s| s.auth != kxen_app::providers::AuthKind::LocalFree)
                .flat_map(|s| {
                    kxen_app::auth::credential::accounts_of(&store, s.key).into_iter().map(|key| {
                        let cred = store.get(&key);
                        let expired = cred.is_some_and(|c| c.is_expired());
                        let region = cred.and_then(|c| c.region());
                        json!({ "provider": s.key, "account": key.strip_prefix(&format!("{}:", s.key)).map(String::from).unwrap_or_else(|| "default".to_string()), "id": key, "expired": expired, "region": region })
                    }).collect::<Vec<_>>()
                })
                .collect();
            let cfg =
                kxen_app::core::config::Config::load(&kxen_app::core::paths::config_dir().join("config.toml"), None).unwrap_or_default();
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
            // region 可选（多区域厂商的账号变体，如 kimi 的 cn/intl）；必须是 registry 声明的合法区域
            let region = params.get("region").and_then(Value::as_str);
            if let Some(r) = region {
                let valid = kxen_app::providers::find(provider).is_some_and(|s| s.regions.iter().any(|x| x.key == r));
                if !valid {
                    return Err(format!("provider {provider} 无区域 {r}"));
                }
            }
            let key = kxen_app::auth::credential::account_id(provider, account);
            let state = app.state::<Arc<AppState>>();
            let mut store = state.auth_store.lock().map_err(|e| e.to_string())?;
            let cred = if kind == "api" {
                kxen_app::auth::credential::CredentialKind::Api { key: access.to_string(), region: region.map(String::from) }
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
            store.insert(
                format!("custom:{name}"),
                kxen_app::auth::credential::CredentialKind::Api { key: api_key.to_string(), region: None },
            );
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
