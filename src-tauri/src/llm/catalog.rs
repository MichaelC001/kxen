//! 模型目录（ModelCatalog）：models.dev 快照为主 + providers registry 静态兜底。
//! picker / 路由配置 / 状态栏的单一数据源：内存 -> 磁盘 -> 静态表；24h TTL 惰性后台刷新，
//! 静默失败留旧缓存（models.dev 不可达不阻塞任何功能）。

use crate::core::session::now_ms;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

const MODELS_DEV_URL: &str = "https://models.dev/api.json";
const TTL_MS: u64 = 24 * 3600 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub tool_call: bool,
    #[serde(default)]
    pub attachment: bool,
    #[serde(default)]
    pub modalities_in: Vec<String>,
    #[serde(default)]
    pub context: u64,
    #[serde(default)]
    pub output: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCatalog {
    pub provider: String,
    pub provider_name: String,
    pub models: Vec<ModelInfo>,
    pub fetched_at: u64,
    pub source: String, // "models.dev" | "static"
}

static CACHE: OnceLock<Mutex<Option<Vec<ProviderCatalog>>>> = OnceLock::new();

fn cache_file() -> std::path::PathBuf {
    crate::core::paths::data_dir().join("models-catalog.json")
}

/// 目录读取：内存 -> 磁盘 -> 静态兜底；磁盘过期/缺失时后台刷新（不阻塞调用方）。
pub fn catalog() -> Vec<ProviderCatalog> {
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Some(c) = crate::core::shared::lock(cache).as_ref() {
        return c.clone();
    }
    let disk = match read_disk_cache() {
        Ok(cache) => cache,
        Err(error) => {
            tracing::warn!(%error, "models.dev cache unavailable; using static catalog until refresh");
            None
        }
    };
    let (out, stale) = match disk {
        Some(c) if !c.is_empty() => {
            let stale = now_ms().saturating_sub(c[0].fetched_at) > TTL_MS;
            (c, stale)
        }
        _ => (static_catalog(), true),
    };
    *crate::core::shared::lock(cache) = Some(out.clone());
    if stale {
        refresh_async();
    }
    out
}

/// 后台刷新（TTL 到期或首次）：成功则落盘 + 换内存；失败静默。
pub fn refresh_async() {
    static REFRESHING: OnceLock<Mutex<bool>> = OnceLock::new();
    let flag = REFRESHING.get_or_init(|| Mutex::new(false));
    {
        let mut running = crate::core::shared::lock(flag);
        if *running {
            return;
        }
        *running = true;
    }
    // 纯同步上下文（如同步单测）没有 reactor：tokio::spawn 会 panic，跳过本次后台刷新
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        *crate::core::shared::lock(flag) = false;
        return;
    };
    handle.spawn(async move {
        let result = async {
            let resp = crate::llm::client::shared_http()
                .get(MODELS_DEV_URL)
                .timeout(std::time::Duration::from_secs(20))
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            let text = crate::net_response::text(resp, crate::net_response::CATALOG_BODY_LIMIT, "models.dev catalog").await?;
            parse_models_dev(&text).ok_or_else(|| "parse failed".to_string())
        }
        .await;
        match result {
            Ok(c) => {
                if let Err(error) = write_disk_cache(&c) {
                    tracing::warn!(%error, "models.dev cache persistence failed; keeping refreshed in-memory catalog");
                }
                let cache = CACHE.get_or_init(|| Mutex::new(None));
                *crate::core::shared::lock(cache) = Some(c);
                tracing::info!("models.dev catalog refreshed");
            }
            Err(e) => tracing::warn!(error = %e, "models.dev refresh failed (keep old cache)"),
        }
        *crate::core::shared::lock(flag) = false;
    });
}

fn read_disk_cache() -> Result<Option<Vec<ProviderCatalog>>, String> {
    let path = cache_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("read {}: {error}", path.display())),
    };
    let cache = serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))?;
    Ok(Some(cache))
}

fn write_disk_cache(catalog: &[ProviderCatalog]) -> Result<(), String> {
    let path = cache_file();
    let parent = path.parent().ok_or_else(|| format!("cache path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(catalog).map_err(|error| format!("serialize models.dev cache: {error}"))?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)
        .map_err(|error| format!("open {}: {error}", tmp.display()))?;
    file.write_all(&bytes).map_err(|error| format!("write {}: {error}", tmp.display()))?;
    file.sync_all().map_err(|error| format!("sync {}: {error}", tmp.display()))?;
    drop(file);
    std::fs::rename(&tmp, &path).map_err(|error| {
        std::fs::remove_file(&tmp).ok();
        format!("replace {}: {error}", path.display())
    })?;
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("sync {}: {error}", parent.display()))?;
    Ok(())
}

/// models.dev api.json 解析：按 registry 的 models_dev 键映射提取（api.json 全量 ~200 provider，只收 registry 覆盖的）。
fn parse_models_dev(text: &str) -> Option<Vec<ProviderCatalog>> {
    let root: serde_json::Value = serde_json::from_str(text).ok()?;
    let ts = now_ms();
    let mut out = Vec::new();
    for spec in crate::providers::all() {
        let Some(dev_id) = spec.models_dev else { continue };
        let Some(prov) = root.get(dev_id) else { continue };
        let provider_name = prov.get("name").and_then(|n| n.as_str()).unwrap_or(spec.display).to_string();
        let mut models: Vec<ModelInfo> = prov
            .get("models")?
            .as_object()?
            .iter()
            .map(|(mid, m)| ModelInfo {
                id: mid.clone(),
                name: m.get("name").and_then(|n| n.as_str()).unwrap_or(mid).to_string(),
                family: m.get("family").and_then(|f| f.as_str()).unwrap_or_default().to_string(),
                reasoning: m.get("reasoning").and_then(|v| v.as_bool()).unwrap_or(false),
                tool_call: m.get("tool_call").and_then(|v| v.as_bool()).unwrap_or(false),
                attachment: m.get("attachment").and_then(|v| v.as_bool()).unwrap_or(false),
                modalities_in: m
                    .get("modalities")
                    .and_then(|mo| mo.get("input"))
                    .and_then(|i| i.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                context: m.get("limit").and_then(|l| l.get("context")).and_then(|c| c.as_u64()).unwrap_or(0),
                output: m.get("limit").and_then(|l| l.get("output")).and_then(|c| c.as_u64()).unwrap_or(0),
            })
            .collect();
        models.sort_by_key(|m| std::cmp::Reverse(m.context));
        out.push(ProviderCatalog { provider: spec.key.to_string(), provider_name, models, fetched_at: ts, source: "models.dev".into() });
    }
    if out.is_empty() { None } else { Some(out) }
}

/// 静态兜底：models.dev 首次不可达时的最小可用集（registry 全表，种子见 providers/seeds.rs）。
fn static_catalog() -> Vec<ProviderCatalog> {
    let ts = now_ms();
    crate::providers::all()
        .iter()
        .map(|spec| {
            let models = spec
                .static_models
                .iter()
                .map(|s| ModelInfo {
                    id: s.id.into(),
                    name: s.name.into(),
                    family: String::new(),
                    reasoning: s.reasoning,
                    tool_call: true,
                    attachment: s.attachment,
                    modalities_in: if s.attachment { vec!["text".into(), "image".into()] } else { vec!["text".into()] },
                    context: s.context,
                    output: 0,
                })
                .collect();
            ProviderCatalog {
                provider: spec.key.into(),
                provider_name: spec.display.into(),
                models,
                fetched_at: ts,
                source: "static".into(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
