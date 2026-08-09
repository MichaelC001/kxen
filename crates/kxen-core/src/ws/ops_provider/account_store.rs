//! Provider 账号和自定义 Provider 的持久化事务。该模块不依赖 Tauri host。

use serde_json::{Value, json};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[cfg(test)]
use super::transaction::transact_custom_provider;
use super::transaction::transact_custom_provider_with_runtime;
use kxen_core::auth::credential::{AuthStore, CredentialKind};

type SharedStoreMutex = Mutex<Arc<AuthStore>>;

fn commit_auth_transaction(
    store: &SharedStoreMutex,
    transaction: impl FnOnce() -> Result<kxen_core::auth::credential::AuthUpdate, String>,
) -> Result<Arc<AuthStore>, String> {
    let mut memory = store.lock().map_err(|error| error.to_string())?;
    let (persisted, warning) = transaction()?.into_snapshot_and_warning();
    let persisted = Arc::new(persisted);
    *memory = Arc::clone(&persisted);
    match warning {
        Some(error) => Err(error),
        None => Ok(persisted),
    }
}

fn commit_custom_transaction(
    store: &SharedStoreMutex,
    transaction: impl FnOnce() -> Result<(AuthStore, Option<String>), String>,
) -> Result<Arc<AuthStore>, String> {
    let mut memory = store.lock().map_err(|error| error.to_string())?;
    let (persisted, warning) = transaction()?;
    let persisted = Arc::new(persisted);
    *memory = Arc::clone(&persisted);
    match warning {
        Some(error) => Err(error),
        None => Ok(persisted),
    }
}

pub(super) fn accounts(store: &SharedStoreMutex, config_path: &Path) -> Result<Value, String> {
    let store = store.lock().map_err(|error| error.to_string())?.clone();
    let mut out: Vec<Value> = kxen_core::providers::all()
        .iter()
        .filter(|spec| spec.auth != kxen_core::providers::AuthKind::LocalFree)
        .flat_map(|spec| {
            kxen_core::auth::credential::accounts_of(&store, spec.key)
                .into_iter()
                .map(|key| {
                    let credential = store.get(&key);
                    json!({
                        "provider": spec.key,
                        "account": key.strip_prefix(&format!("{}:", spec.key)).map(String::from).unwrap_or_else(|| "default".into()),
                        "id": key,
                        "expired": credential.is_some_and(|credential| credential.is_expired()),
                        "region": credential.and_then(|credential| credential.region()),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let config = kxen_core::core::config::Config::load(config_path, None)
        .map_err(|error| format!("config load {}: {error}", config_path.display()))?;
    for (name, definition) in &config.custom_providers {
        let id = format!("custom:{name}");
        out.push(json!({
            "provider": id,
            "account": "default",
            "id": id,
            "expired": false,
            "custom": true,
            "base_url": definition.base_url,
            "models": definition.models,
            "protocol": definition.protocol,
            "capabilities": definition.capabilities,
        }));
    }
    Ok(json!(out))
}

pub(super) fn import_account(params: &Value, store: &SharedStoreMutex, auth_path: &Path) -> Result<Value, String> {
    let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
    let account = params.get("account").and_then(Value::as_str).ok_or("missing account")?;
    kxen_core::auth::credential::validate_named_account(account)?;
    let kind = params.get("kind").and_then(Value::as_str).unwrap_or("oauth");
    let access = params.get("access").and_then(Value::as_str).ok_or("missing access token")?;
    let region = params.get("region").and_then(Value::as_str);
    if let Some(region) = region {
        let valid = kxen_core::providers::find(provider).is_some_and(|spec| spec.regions.iter().any(|candidate| candidate.key == region));
        if !valid {
            return Err(format!("provider {provider} 无区域 {region}"));
        }
    }
    let key = kxen_core::auth::credential::account_id(provider, account);
    let credential = if kind == "api" {
        CredentialKind::Api { key: access.into(), region: region.map(String::from) }
    } else {
        CredentialKind::Oauth {
            access: access.into(),
            refresh: params.get("refresh").and_then(Value::as_str).unwrap_or("").into(),
            expires: params.get("expires").and_then(Value::as_u64).unwrap_or(0),
            account_id: params.get("account_id").and_then(Value::as_str).map(String::from),
        }
    };
    commit_auth_transaction(store, || {
        kxen_core::auth::credential::update_auth_file_committed(auth_path, |disk| {
            disk.insert(key.clone(), credential);
            Ok(())
        })
        .map_err(|error| error.to_string())
    })?;
    Ok(json!({ "id": key }))
}

pub(super) fn remove_account(params: &Value, store: &SharedStoreMutex, auth_path: &Path) -> Result<Value, String> {
    let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
    let account = params.get("account").and_then(Value::as_str).ok_or("missing account")?;
    kxen_core::auth::credential::validate_named_account(account)?;
    let key = kxen_core::auth::credential::account_id(provider, account);
    commit_auth_transaction(store, || {
        kxen_core::auth::credential::update_auth_file_committed(auth_path, |disk| {
            disk.remove(&key).map(|_| ()).ok_or_else(|| format!("账号不存在: {key}"))
        })
        .map_err(|error| error.to_string())
    })?;
    Ok(json!({ "removed": key }))
}

pub(super) fn update_region(params: &Value, store: &SharedStoreMutex, auth_path: &Path) -> Result<Value, String> {
    let provider = params.get("provider").and_then(Value::as_str).ok_or("missing provider")?;
    let account = params.get("account").and_then(Value::as_str).ok_or("missing account")?;
    kxen_core::auth::credential::validate_account_selector(account)?;
    let region = params.get("region").and_then(Value::as_str);
    commit_auth_transaction(store, || {
        kxen_core::auth::credential::update_auth_file_committed(auth_path, |disk| set_region(disk, provider, account, region))
            .map_err(|error| error.to_string())
    })?;
    Ok(json!({ "updated": kxen_core::auth::credential::account_id(provider, account) }))
}

pub(super) fn add_custom_with_runtime(
    params: &Value,
    store: &SharedStoreMutex,
    config_path: &Path,
    auth_path: &Path,
    runtimes: &kxen_core::workspace_runtime::WorkspaceRuntimeRegistry,
) -> Result<Value, String> {
    let name = params.get("name").and_then(Value::as_str).ok_or("missing name")?;
    kxen_core::auth::credential::validate_custom_name(name)?;
    let base_url = params.get("base_url").and_then(Value::as_str).ok_or("missing base_url")?;
    kxen_core::core::config::validate_custom_provider_endpoint(base_url).map_err(|error| format!("base_url {error}"))?;
    let models: Vec<String> = params
        .get("models")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(|value| value.as_str().map(String::from)).collect())
        .unwrap_or_default();
    if models.is_empty() {
        return Err("models 至少一个".into());
    }
    let api_key = params.get("api_key").and_then(Value::as_str).ok_or("missing api_key")?;
    let protocol = params.get("protocol").and_then(Value::as_str).unwrap_or("openai");
    kxen_core::core::config::validate_custom_provider_auth(protocol, api_key)?;
    let capabilities: Vec<String> = params
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(|value| value.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec!["text".into()]);
    let provider = format!("custom:{name}");
    commit_custom_transaction(store, || {
        transact_custom_provider_with_runtime(
            config_path,
            auth_path,
            &provider,
            runtimes,
            |document| {
                let custom = document.entry(String::from("custom_providers")).or_insert_with(|| toml::Value::Table(toml::Table::new()));
                let table = custom.as_table_mut().ok_or("custom_providers is not a table")?;
                let mut definition = toml::Table::new();
                definition.insert("base_url".into(), toml::Value::String(base_url.into()));
                definition.insert("models".into(), toml::Value::Array(models.into_iter().map(toml::Value::String).collect()));
                definition.insert("protocol".into(), toml::Value::String(protocol.into()));
                definition.insert("capabilities".into(), toml::Value::Array(capabilities.into_iter().map(toml::Value::String).collect()));
                table.insert(name.into(), toml::Value::Table(definition));
                Ok(())
            },
            |disk| {
                disk.insert(provider.clone(), CredentialKind::Api { key: api_key.into(), region: None });
                Ok(())
            },
        )
    })?;
    Ok(json!({ "id": provider }))
}

pub(super) fn remove_custom_with_runtime(
    params: &Value,
    store: &SharedStoreMutex,
    config_path: &Path,
    auth_path: &Path,
    runtimes: &kxen_core::workspace_runtime::WorkspaceRuntimeRegistry,
) -> Result<Value, String> {
    let name = params.get("name").and_then(Value::as_str).ok_or("missing name")?;
    kxen_core::auth::credential::validate_custom_name(name)?;
    let provider = format!("custom:{name}");
    commit_custom_transaction(store, || {
        transact_custom_provider_with_runtime(
            config_path,
            auth_path,
            &provider,
            runtimes,
            |document| {
                if let Some(toml::Value::Table(table)) = document.get_mut("custom_providers") {
                    table.remove(name);
                }
                Ok(())
            },
            |disk| {
                for key in kxen_core::auth::credential::accounts_of(disk, &provider) {
                    disk.remove(&key);
                }
                Ok(())
            },
        )
    })?;
    Ok(json!({ "removed": name }))
}

#[cfg(test)]
pub(super) fn add_custom(params: &Value, store: &SharedStoreMutex, config_path: &Path, auth_path: &Path) -> Result<Value, String> {
    let runtimes = kxen_core::workspace_runtime::WorkspaceRuntimeRegistry::with_user_config(config_path.to_path_buf())?;
    add_custom_with_runtime(params, store, config_path, auth_path, &runtimes)
}

#[cfg(test)]
pub(super) fn remove_custom(params: &Value, store: &SharedStoreMutex, config_path: &Path, auth_path: &Path) -> Result<Value, String> {
    let runtimes = kxen_core::workspace_runtime::WorkspaceRuntimeRegistry::with_user_config(config_path.to_path_buf())?;
    remove_custom_with_runtime(params, store, config_path, auth_path, &runtimes)
}

pub(super) fn commit_reprobe(
    store: &SharedStoreMutex,
    auth_path: &Path,
    baseline: &AuthStore,
    probed: &AuthStore,
) -> Result<Arc<AuthStore>, String> {
    commit_auth_transaction(store, || {
        kxen_core::auth::credential::update_auth_file_committed(auth_path, |disk| {
            kxen_core::auth::probe::merge_probe_delta(baseline, probed, disk);
            Ok(())
        })
        .map_err(|error| error.to_string())
    })
}

pub(super) fn summarize_reprobe(outcomes: &[(&'static str, kxen_core::auth::ProbeOutcome, &'static str)]) -> (Vec<String>, Vec<Value>) {
    let text = |outcome: &kxen_core::auth::ProbeOutcome| match outcome {
        kxen_core::auth::ProbeOutcome::Imported => "已从官方源导入",
        kxen_core::auth::ProbeOutcome::Fresh => "已是最新",
        kxen_core::auth::ProbeOutcome::Missing => "未找到官方凭证",
        kxen_core::auth::ProbeOutcome::NeedsApproval => "首次读取未获批准，已跳过",
    };
    let lines = outcomes.iter().map(|(_, outcome, display)| format!("{display}：{}", text(outcome))).collect();
    let issues = outcomes
        .iter()
        .filter(|(_, outcome, _)| matches!(outcome, kxen_core::auth::ProbeOutcome::Missing | kxen_core::auth::ProbeOutcome::NeedsApproval))
        .map(|(provider, outcome, display)| {
            let hint = kxen_core::auth::probe::RULES.iter().find(|rule| rule.provider == *provider).map(|rule| rule.source).unwrap_or("");
            json!({ "text": format!("{display}：{}", text(outcome)), "hint": hint })
        })
        .collect();
    (lines, issues)
}

fn set_region(store: &mut AuthStore, provider: &str, account: &str, region: Option<&str>) -> Result<(), String> {
    let spec = kxen_core::providers::find(provider).ok_or_else(|| format!("未知 provider: {provider}"))?;
    if let Some(region) = region
        && !spec.regions.iter().any(|candidate| candidate.key == region)
    {
        return Err(format!("provider {provider} 无区域 {region}"));
    }
    let key = kxen_core::auth::credential::account_id(provider, account);
    let credential = store.get_mut(&key).ok_or_else(|| format!("账号不存在: {key}"))?;
    match credential {
        CredentialKind::Api { region: slot, .. } => {
            *slot = region.map(String::from);
            Ok(())
        }
        _ => Err("仅 API Key 账号支持改区域".into()),
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
