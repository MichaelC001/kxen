//! 领域 RPC 分组：voice / knowledge / provider / mrm / test_dispatch（rpc.rs 的分流层）。

use serde_json::{json, Value};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::AppState;

const METHODS: &[&str] = &[
    "mrm.stats",
    "agent.test_dispatch",
    "knowledge.list",
    "knowledge.add",
    "knowledge.remove",
    "knowledge.set_enabled",
    "knowledge.move",
    "knowledge.injection_preview",
    "schedule.list",
    "schedule.add",
    "schedule.remove",
    "diagnostics.export",
    "notifications.list",
    "notifications.clear",
    "voice.engines",
    "voice.transcribe_file",
    "voice.set_provider_key",
    "voice.set_engine",
    "voice.start",
    "voice.stop",
];

/// 返回 Some(result) 表示命中；None 表示不是本组方法。
pub(super) async fn try_handle(method: &str, params: &Value, app: &AppHandle) -> Option<Result<Value, String>> {
    if super::ops_provider::METHODS.contains(&method) {
        return Some(super::ops_provider::handle(method, params, app).await);
    }
    if !METHODS.contains(&method) {
        return None;
    }
    Some(handle(method, params, app).await)
}

async fn handle(method: &str, params: &Value, app: &AppHandle) -> Result<Value, String> {
    match method {
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
            let scope = kxen_app::knowledge::Scope::parse(params.get("scope").and_then(Value::as_str).unwrap_or("personal"))?;
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
            let scope = kxen_app::knowledge::Scope::parse(params.get("scope").and_then(Value::as_str).ok_or("missing scope")?)?;
            let slug = params.get("slug").and_then(Value::as_str).ok_or("missing slug")?;
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            kxen_app::knowledge::remove(scope, &dir, slug)?;
            Ok(json!({ "removed": true }))
        }
        "knowledge.set_enabled" => {
            let scope = kxen_app::knowledge::Scope::parse(params.get("scope").and_then(Value::as_str).ok_or("missing scope")?)?;
            let slug = params.get("slug").and_then(Value::as_str).ok_or("missing slug")?;
            let enabled = params.get("enabled").and_then(Value::as_bool).ok_or("missing enabled")?;
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            kxen_app::knowledge::set_enabled(scope, &dir, slug, enabled)?;
            Ok(json!({ "scope": scope.as_str(), "slug": slug, "enabled": enabled }))
        }
        "knowledge.move" => {
            let scope = kxen_app::knowledge::Scope::parse(params.get("scope").and_then(Value::as_str).ok_or("missing scope")?)?;
            let slug = params.get("slug").and_then(Value::as_str).ok_or("missing slug")?;
            let to = kxen_app::knowledge::Scope::parse(params.get("to").and_then(Value::as_str).ok_or("missing to")?)?;
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            let path = kxen_app::knowledge::move_entry(scope, &dir, slug, to)?;
            Ok(json!({ "path": path }))
        }
        "knowledge.injection_preview" => {
            let state = app.state::<Arc<AppState>>();
            let dir = state.active_workspace.read().expect("workspace").clone();
            let block = kxen_app::knowledge::render(&dir, &[]);
            Ok(json!({ "block": block }))
        }
        "schedule.list" => Ok(serde_json::to_value(kxen_app::core::schedule::list()).map_err(|e| e.to_string())?),
        "schedule.add" => {
            let cron = params.get("cron").and_then(Value::as_str).ok_or("missing cron")?;
            let prompt = params.get("prompt").and_then(Value::as_str).ok_or("missing prompt")?;
            let session_id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
            let once = params.get("once").and_then(Value::as_bool).unwrap_or(false);
            let job = kxen_app::core::schedule::add(cron, prompt, session_id, once)?;
            Ok(serde_json::to_value(job).map_err(|e| e.to_string())?)
        }
        "schedule.remove" => {
            let id = params.get("id").and_then(Value::as_str).ok_or("missing id")?;
            Ok(json!(kxen_app::core::schedule::remove(id)))
        }
        "diagnostics.export" => {
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?.clone();
            let report = crate::doctor::doctor_report(&store);
            let config_text = std::fs::read_to_string(kxen_app::core::paths::config_dir().join("config.toml")).unwrap_or_default();
            let mrm = state.mrm.read().expect("mrm").clone();
            let describe = mrm.describe().await;
            let mut md = format!("# kxen diagnostics\n\n- version: {}\n- at: {:?}\n\n", env!("CARGO_PKG_VERSION"), std::time::SystemTime::now());
            md.push_str("## providers\n\n");
            for e in &report.entries {
                md.push_str(&format!("- {} [{}]: {} ({})\n", e.display, e.provider, e.status, e.detail));
            }
            md.push_str(&format!("\n## mrm\n\n```\n{describe}\n```\n\n## config.toml\n\n```toml\n{config_text}\n```\n"));
            let path = dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                .join("Downloads")
                .join(format!("kxen-diagnostics-{}.md", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)));
            std::fs::write(&path, md).map_err(|e| e.to_string())?;
            Ok(json!({ "path": path.to_string_lossy() }))
        }
        "notifications.list" => {
            let state = app.state::<Arc<AppState>>();
            let buf = state.notifications.lock().map_err(|e| e.to_string())?;
            Ok(json!(buf.iter().map(|(at, text)| json!({ "at": at, "text": text })).collect::<Vec<_>>()))
        }
        "notifications.clear" => {
            let state = app.state::<Arc<AppState>>();
            state.notifications.lock().map_err(|e| e.to_string())?.clear();
            Ok(json!(true))
        }
        "voice.engines" => {
            let config = load_config()?;
            let state = app.state::<Arc<AppState>>();
            let store = state.auth_store.lock().map_err(|e| e.to_string())?;
            Ok(json!({
                "engine": config.voice.engine,
                "fallback": config.voice.fallback,
                "locale": config.voice.locale,
                "engines": kxen_app::voice::engines(&config, &store),
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
            let text = kxen_app::voice::transcribe_file(&config, &store, engine, path, locale).await?;
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
            let mut doc = read_toml(&path)?;
            let mut voice = toml::map::Map::new();
            voice.insert("engine".into(), toml::Value::String(engine.into()));
            if !fallback.is_empty() {
                voice.insert("fallback".into(), toml::Value::Array(fallback.into_iter().map(toml::Value::String).collect()));
            }
            doc.insert("voice".into(), toml::Value::Table(voice));
            write_toml(&path, &doc)?;
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
            let text = kxen_app::voice::stop(&config, &store).await?;
            Ok(json!({ "text": text }))
        }
        other => Err(format!("unknown method: {other}")),
    }
}

fn load_config() -> Result<kxen_app::core::config::Config, String> {
    kxen_app::core::config::Config::load(&kxen_app::core::paths::config_dir().join("config.toml"), None).map_err(|e| e.to_string())
}

/// toml 1.x 文档读（Value::from_str 解析的是值不是文档）。
pub(super) fn read_toml(path: &std::path::Path) -> Result<toml::Table, String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(toml::Table::new());
    }
    toml::from_str(&text).map_err(|e| format!("config.toml parse: {e}"))
}

/// 原子写回（tmp + rename）。
pub(super) fn write_toml(path: &std::path::Path, doc: &toml::Table) -> Result<(), String> {
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, toml::to_string(doc).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
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
