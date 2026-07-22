//! 领域 RPC 分组：voice / knowledge / provider / mrm / test_dispatch（rpc.rs 的分流层）。

use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::AppState;

const METHODS: &[&str] = &[
    "provider.verify",
    "provider.reprobe",
    "provider.import_account",
    "provider.remove_account",
    "provider.accounts",
    "mrm.stats",
    "agent.test_dispatch",
    "knowledge.list",
    "knowledge.add",
    "knowledge.remove",
    "knowledge.set_enabled",
    "knowledge.move",
    "knowledge.injection_preview",
    "voice.engines",
    "voice.transcribe_file",
    "voice.set_provider_key",
    "voice.set_engine",
    "voice.start",
    "voice.stop",
];

/// 返回 Some(result) 表示命中；None 表示不是本组方法。
pub(super) async fn try_handle(method: &str, params: &Value, app: &AppHandle) -> Option<Result<Value, String>> {
    if !METHODS.contains(&method) {
        return None;
    }
    Some(handle(method, params, app).await)
}

async fn handle(method: &str, params: &Value, app: &AppHandle) -> Result<Value, String> {
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
            let out: Vec<Value> = ["anthropic", "openai", "xai", "kimi-for-coding"]
                .iter()
                .flat_map(|p| {
                    kxen_app::auth::credential::accounts_of(&store, p).into_iter().map(|key| {
                        let expired = store.get(&key).is_some_and(|c| c.is_expired());
                        json!({ "provider": p, "account": key.strip_prefix(&format!("{p}:")).map(String::from).unwrap_or_else(|| "default".to_string()), "id": key, "expired": expired })
                    }).collect::<Vec<_>>()
                })
                .collect();
            Ok(json!(out))
        }
        "provider.import_account" => {
            let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
            let account = params.get("account").and_then(Value::as_str).ok_or("missing account")?;
            let access = params.get("access").and_then(Value::as_str).ok_or("missing access token")?;
            let refresh = params.get("refresh").and_then(Value::as_str).unwrap_or("");
            let expires = params.get("expires").and_then(Value::as_u64).unwrap_or(0);
            let key = kxen_app::auth::credential::account_id(provider, account);
            let state = app.state::<Arc<AppState>>();
            let mut store = state.auth_store.lock().map_err(|e| e.to_string())?;
            store.insert(
                key.clone(),
                kxen_app::auth::credential::CredentialKind::Oauth {
                    access: access.to_string(),
                    refresh: refresh.to_string(),
                    expires,
                    account_id: params.get("account_id").and_then(Value::as_str).map(String::from),
                },
            );
            let path = kxen_app::core::paths::auth_file();
            kxen_app::auth::credential::write_auth_file(&path, &store).map_err(|e| e.to_string())?;
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
            let path = kxen_app::core::paths::auth_file();
            kxen_app::auth::credential::write_auth_file(&path, &store).map_err(|e| e.to_string())?;
            Ok(json!({ "removed": key }))
        }
        "provider.reprobe" => reprobe(app).await,
        "mrm.stats" => {
            let state = app.state::<Arc<AppState>>();
            let mrm = state.mrm.read().expect("mrm").clone();
            Ok(json!({
                "describe": mrm.describe().await,
                "history": mrm.history().await,
            }))
        }
        "agent.test_dispatch" => test_dispatch(app, params).await,
        "knowledge.list" => {
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            serde_json::to_value(kxen_app::knowledge::list(&dir)).map_err(|e| e.to_string())
        }
        "knowledge.add" => {
            let scope = params.get("scope").and_then(Value::as_str).unwrap_or("memory");
            let slug = params.get("slug").and_then(Value::as_str);
            let kind = params.get("type").and_then(Value::as_str).unwrap_or("note");
            let description = params.get("description").and_then(Value::as_str).ok_or("missing description")?;
            let content = params.get("content").and_then(Value::as_str).ok_or("missing content")?;
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            let path = kxen_app::knowledge::add(scope, &dir, slug, kind, description, content)?;
            Ok(json!({ "path": path }))
        }
        "knowledge.remove" => {
            let scope = params.get("scope").and_then(Value::as_str).ok_or("missing scope")?;
            let slug = params.get("slug").and_then(Value::as_str).ok_or("missing slug")?;
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            kxen_app::knowledge::remove(scope, &dir, slug)?;
            Ok(json!({ "removed": true }))
        }
        "knowledge.set_enabled" => {
            let scope = params.get("scope").and_then(Value::as_str).ok_or("missing scope")?;
            let slug = params.get("slug").and_then(Value::as_str).ok_or("missing slug")?;
            let enabled = params.get("enabled").and_then(Value::as_bool).ok_or("missing enabled")?;
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            kxen_app::knowledge::set_enabled(scope, &dir, slug, enabled)?;
            Ok(json!({ "scope": scope, "slug": slug, "enabled": enabled }))
        }
        "knowledge.move" => {
            let scope = params.get("scope").and_then(Value::as_str).ok_or("missing scope")?;
            let slug = params.get("slug").and_then(Value::as_str).ok_or("missing slug")?;
            let to = params.get("to").and_then(Value::as_str).ok_or("missing to")?;
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            let path = kxen_app::knowledge::move_entry(scope, &dir, slug, to)?;
            Ok(json!({ "path": path }))
        }
        "knowledge.injection_preview" => {
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            let project = kxen_app::agent::okf::render_context(&dir, &[]);
            let extra = kxen_app::knowledge::render_extra(&dir);
            Ok(json!({ "project": project, "extra": extra }))
        }
        "voice.engines" => {
            let config = load_config()?;
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?;
            Ok(json!({
                "engine": config.voice.engine,
                "fallback": config.voice.fallback,
                "locale": config.voice.locale,
                "engines": kxen_app::voice::engines(&config.voice, &store),
            }))
        }
        "voice.transcribe_file" => {
            let engine = params.get("engine").and_then(Value::as_str);
            let path = params.get("path").and_then(Value::as_str).ok_or("missing path")?;
            let locale = params.get("locale").and_then(Value::as_str);
            let config = load_config()?;
            let locale = locale.unwrap_or(&config.voice.locale);
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?.clone();
            let text = kxen_app::voice::transcribe_file(&config.voice, &store, engine, path, locale).await?;
            Ok(json!({ "text": text }))
        }
        "voice.set_provider_key" => {
            let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
            let key = params.get("key").and_then(Value::as_str).ok_or("missing key")?;
            let state = app.state::<Arc<AppState>>();
            let mut store = state.auth_store.lock().map_err(|e| e.to_string())?;
            let path = kxen_app::core::paths::auth_file();
            kxen_app::voice::provider::set_key(&mut store, provider, key, &path)?;
            Ok(json!({ "provider": provider, "configured": true }))
        }
        "voice.set_engine" => {
            let engine = params.get("engine").and_then(Value::as_str).ok_or("missing engine")?;
            let fallback: Vec<String> = params
                .get("fallback")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let path = kxen_app::core::paths::config_dir().join("config.toml");
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let mut doc: toml::Table = if text.trim().is_empty() { toml::Table::new() } else { toml::from_str(&text).map_err(|e| format!("config.toml parse: {e}"))? };
            let mut voice = toml::map::Map::new();
            voice.insert("engine".into(), toml::Value::String(engine.into()));
            if !fallback.is_empty() {
                voice.insert("fallback".into(), toml::Value::Array(fallback.into_iter().map(toml::Value::String).collect()));
            }
            doc.insert("voice".into(), toml::Value::Table(voice));
            let tmp = path.with_extension("toml.tmp");
            std::fs::write(&tmp, toml::to_string(&doc).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
            Ok(json!({ "engine": engine }))
        }
        "voice.start" => {
            let config = load_config()?;
            let locale = params.get("locale").and_then(Value::as_str).unwrap_or(&config.voice.locale);
            let engine_override = params.get("engine").and_then(Value::as_str);
            let mut voice = config.voice.clone();
            if let Some(e) = engine_override {
                voice.engine = e.to_string();
            }
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?.clone();
            let started = kxen_app::voice::start(&voice, &store, locale, state.bus.clone())?;
            Ok(json!({ "engine": started, "recording": true }))
        }
        "voice.stop" => {
            let config = load_config()?;
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?.clone();
            let text = kxen_app::voice::stop(&config.voice, &store).await?;
            Ok(json!({ "text": text }))
        }
        other => Err(format!("unknown method: {other}")),
    }
}

fn load_config() -> Result<kxen_app::core::config::Config, String> {
    kxen_app::core::config::Config::load(&kxen_app::core::paths::config_dir().join("config.toml"), None).map_err(|e| e.to_string())
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

async fn test_dispatch(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let role = params.get("role").and_then(Value::as_str).ok_or("missing role")?;
    let state = app.state::<Arc<AppState>>();
    let store = state.auth_store.lock().map_err(|e| e.to_string())?.clone();
    let mrm = state.mrm.read().expect("mrm").clone();
    let resolved = mrm.resolve(role, &store).await.ok_or_else(|| format!("no available model for role {role}"))?;
    let degraded = resolved.degraded_from.clone();
    let deps = kxen_app::agent::subagent::SubagentDeps {
        registry: state.registry.clone(),
        workdir: state.workdir.clone(),
        store,
        mrm,
        hooks: Some(state.hooks.clone()),
        cancel: None,
        agents: state.agents.clone(),
        session_id: None,
        bus: state.bus.clone(),
    };
    let answer = kxen_app::agent::subagent::dispatch(role, "Reply with exactly: PONG".to_string(), &deps, kxen_app::agent::activity::AgentKind::Subagent).await?;
    Ok(json!({
        "role": role,
        "provider": resolved.provider,
        "model": resolved.model,
        "account": resolved.account,
        "degraded_from": degraded,
        "answer": answer.chars().take(200).collect::<String>(),
    }))
}
