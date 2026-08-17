//! 可选 embedding 语义召回（缺省关闭，未配置/调用失败静默回落纯 BM25）。
//! 三档 provider：openai（text-embedding-3-small）、openrouter（同 OpenAI 协议换 base URL）、
//! ollama（/api/embed，nomic-embed-text，本地无鉴权）。
//! 设计：检索路径永远同步、永不阻塞网络。只读磁盘缓存算 cosine；缓存未命中的文本
//! 后台 spawn 预热（本轮 BM25，下轮融合生效）。凭证复用 auth.json 的同 provider 账号。

use super::embedding_cache::EmbeddingCache;
use crate::auth::credential::{AuthStore, CredentialKind, credential_for};
use crate::core::config::EmbeddingConfig;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

mod warm;
pub use warm::EmbeddingRuntime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    OpenAi,
    Ollama,
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub provider: &'static str,
    pub account: Option<String>,
    pub url: String,
    pub key: Option<String>,
    pub model: String,
    pub protocol: Protocol,
    /// 明确的 localhost/loopback endpoint 走 net_guard 例外（来源是已验证的用户 config）。
    pub allow_loopback: bool,
}

/// 端点解析（纯函数，可测）：缺省 provider 或未知 provider -> None（= 功能关闭）。
/// 自定义 base_url 已在 Config load 阶段限制为远程 HTTPS 或显式 loopback HTTP。
pub fn resolve_endpoint_with(cfg: &EmbeddingConfig, store: &AuthStore) -> Option<Endpoint> {
    let custom_base = cfg.base_url.trim().trim_end_matches('/');
    let custom_loopback = !custom_base.is_empty() && crate::core::config::endpoint_is_explicit_loopback(custom_base);
    match cfg.provider.as_str() {
        "" => None,
        "openai" => {
            let base = if custom_base.is_empty() { "https://api.openai.com/v1" } else { custom_base };
            Some(Endpoint {
                provider: "openai",
                account: crate::auth::credential::effective_account_name(store, "openai", None),
                url: crate::core::net_security::join_base_endpoint(base, "embeddings").ok()?,
                key: Some(api_key_of(store, "openai")?),
                model: model_or(cfg, "text-embedding-3-small"),
                protocol: Protocol::OpenAi,
                allow_loopback: custom_loopback,
            })
        }
        "openrouter" => {
            let base = if custom_base.is_empty() { "https://openrouter.ai/api/v1" } else { custom_base };
            Some(Endpoint {
                provider: "openrouter",
                account: crate::auth::credential::effective_account_name(store, "openrouter", None),
                url: crate::core::net_security::join_base_endpoint(base, "embeddings").ok()?,
                key: Some(api_key_of(store, "openrouter")?),
                // OpenRouter 的模型 id 带 provider 前缀
                model: model_or(cfg, "openai/text-embedding-3-small"),
                protocol: Protocol::OpenAi,
                allow_loopback: custom_loopback,
            })
        }
        "ollama" => {
            let base = if custom_base.is_empty() { "http://localhost:11434" } else { custom_base };
            Some(Endpoint {
                provider: "ollama",
                account: None,
                url: crate::core::net_security::join_base_endpoint(base, "api/embed").ok()?,
                key: None,
                model: model_or(cfg, "nomic-embed-text"),
                protocol: Protocol::Ollama,
                allow_loopback: custom_base.is_empty() || custom_loopback,
            })
        }
        // 配置写错 provider 名按关闭处理：检索不能因配置笔误挂掉
        _ => None,
    }
}

/// 读盘装配：config 只读用户级（~/.agents/kxen/config.toml），与 llm client 读
/// custom_providers 同路径；召回偏好跟人走，项目级 config 入 git 不放这个。
pub fn resolve_endpoint() -> Option<Endpoint> {
    let config = match crate::core::config_cache::cached_user_config_result() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(%error, "embedding config unavailable");
            return None;
        }
    };
    if config.embedding.provider.is_empty() {
        return None;
    }
    let store = match crate::auth::credential::read_auth_file(&crate::core::paths::KxenPaths::user().auth_file()) {
        Ok(store) => store,
        Err(error) => {
            tracing::error!(%error, "embedding credential store unavailable");
            return None;
        }
    };
    resolve_endpoint_with(&config.embedding, &store)
}

fn model_or(cfg: &EmbeddingConfig, default: &str) -> String {
    let m = cfg.model.trim();
    if m.is_empty() { default.to_string() } else { m.to_string() }
}

fn api_key_of(store: &AuthStore, provider: &str) -> Option<String> {
    match credential_for(store, provider, None) {
        Some(CredentialKind::Api { key, .. }) => Some(key.clone()),
        // openai 订阅 OAuth 的 access token 同样走 bearer
        Some(CredentialKind::Oauth { access, .. }) => Some(access.clone()),
        None => None,
    }
}

/// embedding 输入文本：description + content 前 1000 字符。长尾内容对相似度贡献递减，
/// 截断控制预热批量请求的 payload 体积。
pub fn doc_text(description: &str, content: &str) -> String {
    let cap: String = content.chars().take(1000).collect();
    format!("{description}\n{cap}")
}

/// 缓存键：文本 sha256 hex。内容变 -> 键变 -> 旧向量自然冷掉被 LRU 淘汰，无需主动失效。
pub fn content_hash(text: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(text.as_bytes());
    crate::core::shared::hex_lower(&digest)
}

/// 向量 cache identity 必须包含 endpoint 和 model，避免配置切换后复用维度或语义空间不兼容的旧向量。
pub(super) fn cache_key(endpoint: &Endpoint, text: &str) -> String {
    content_hash(&format!("{}\0{}\0{}\0{text}", endpoint.provider, endpoint.model, endpoint.url))
}

pub fn cache_path() -> std::path::PathBuf {
    crate::core::paths::KxenPaths::user().embedding_cache_file()
}

/// 请求构造（OpenAI 及兼容协议共用）：{"model": ..., "input": [...]}
pub fn build_openai_request(model: &str, texts: &[String]) -> serde_json::Value {
    serde_json::json!({ "model": model, "input": texts })
}

/// OpenAI /embeddings 响应：{"data": [{"embedding": [...]}, ...]}，按 input 序。
pub fn parse_openai_response(body: &str) -> Option<Vec<Vec<f32>>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    parse_openai_value(&v)
}

pub(crate) fn parse_openai_value(v: &serde_json::Value) -> Option<Vec<Vec<f32>>> {
    let data = v.get("data")?.as_array()?;
    data.iter().map(|d| f32_array(d.get("embedding")?)).collect()
}

/// Ollama /api/embed 请求：{"model": ..., "input": [...]}（input 接受数组，批量一次完成）。
pub fn build_ollama_request(model: &str, texts: &[String]) -> serde_json::Value {
    serde_json::json!({ "model": model, "input": texts })
}

/// Composer 等显式 opt-in 调用的同步语义入口。网络、MRM admission、取消与 durable
/// usage accounting 全复用知识检索的统一实现，调用方自行维护独立缓存。
pub async fn embed_managed(ep: &Endpoint, texts: &[String], runtime: &EmbeddingRuntime) -> Result<Vec<Vec<f32>>, String> {
    warm::fetch_managed(ep, texts, runtime).await
}

/// Ollama /api/embed 响应：{"embeddings": [[...], ...]}
pub fn parse_ollama_response(body: &str) -> Option<Vec<Vec<f32>>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    parse_ollama_value(&v)
}

pub(crate) fn parse_ollama_value(v: &serde_json::Value) -> Option<Vec<Vec<f32>>> {
    let arr = v.get("embeddings")?.as_array()?;
    arr.iter().map(f32_array).collect()
}

fn f32_array(v: &serde_json::Value) -> Option<Vec<f32>> {
    v.as_array()?.iter().map(|x| x.as_f64().map(|f| f as f32)).collect()
}

/// 检索侧语义分（同步、零网络）：只读磁盘缓存。返回 None = 本轮无语义（未配置或 query
/// 向量未缓存）；Vec 内逐条 Option = 该条目是否有缓存向量。未命中的文本触发后台预热。
pub fn recall(query: &str, docs: &[String]) -> Option<Vec<Option<f64>>> {
    let endpoint = resolve_endpoint()?;
    let hashes: Vec<String> = docs.iter().map(|doc| cache_key(&endpoint, doc)).collect();
    let (query_present, _, scores) = lookup_cached(&cache_key(&endpoint, query), &hashes)?;
    query_present.then_some(scores)
}

pub(crate) fn recall_lazy(query: &str, runtime: Option<&EmbeddingRuntime>, docs: impl FnOnce() -> Vec<String>) -> Option<Vec<Option<f64>>> {
    let ep = match runtime {
        Some(runtime) => runtime.endpoint.clone()?,
        None => Arc::new(resolve_endpoint()?),
    };
    let docs = docs();
    let query_hash = cache_key(&ep, query);
    let hashes: Vec<String> = docs.iter().map(|doc| cache_key(&ep, doc)).collect();
    let (query_present, present, scores) = lookup_cached(&query_hash, &hashes)?;
    let mut missing: Vec<String> = Vec::new();
    if !query_present {
        missing.push(query.to_string());
    }
    for (doc, present) in docs.into_iter().zip(present) {
        if !present {
            missing.push(doc);
        }
    }
    if !missing.is_empty()
        && let Some(runtime) = runtime
    {
        // 同文重复（同 slug 变体、query 与条目同文）只预热一次
        missing.sort_unstable();
        missing.dedup();
        warm::spawn(ep, missing, runtime.clone());
    }
    query_present.then_some(scores)
}

struct RecallCache {
    path: PathBuf,
    stamp: crate::core::shared::FileStamp,
    cache: EmbeddingCache,
}

static RECALL_CACHE: Mutex<Option<RecallCache>> = Mutex::new(None);

fn lookup_cached(query_hash: &str, doc_hashes: &[String]) -> Option<(bool, Vec<bool>, Vec<Option<f64>>)> {
    let path = cache_path();
    let stamp = match crate::core::shared::file_stamp(&path) {
        Ok(stamp) => stamp,
        Err(error) => {
            tracing::error!(path = %path.display(), %error, "embedding cache unavailable; using BM25 only");
            return None;
        }
    };
    let mut guard = crate::core::shared::lock(&RECALL_CACHE);
    let reload = guard.as_ref().is_none_or(|cached| cached.path != path || cached.stamp != stamp);
    if reload {
        let cache = match EmbeddingCache::load(&path) {
            Ok(cache) => cache,
            Err(error) => {
                tracing::error!(%error, "embedding cache unavailable; using BM25 only");
                return None;
            }
        };
        *guard = Some(RecallCache { path, stamp, cache });
    }
    let cache = &mut guard.as_mut().expect("embedding cache initialized").cache;
    let query_present = cache.contains(query_hash);
    let present: Vec<bool> = doc_hashes.iter().map(|hash| cache.contains(hash)).collect();
    let scores = cache.cosine_scores(query_hash, doc_hashes).unwrap_or_else(|| vec![None; doc_hashes.len()]);
    Some((query_present, present, scores))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(model: &str, url: &str) -> Endpoint {
        Endpoint {
            provider: "openai",
            account: None,
            url: url.to_string(),
            key: None,
            model: model.to_string(),
            protocol: Protocol::OpenAi,
            allow_loopback: false,
        }
    }

    #[test]
    fn cache_key_is_stable_but_namespaced_by_endpoint_and_model() {
        let primary = endpoint("embed-v1", "https://api.example.com/v1/embeddings");
        assert_eq!(cache_key(&primary, "same text"), cache_key(&primary, "same text"));
        assert_ne!(cache_key(&primary, "same text"), cache_key(&endpoint("embed-v2", &primary.url), "same text"));
        assert_ne!(
            cache_key(&primary, "same text"),
            cache_key(&endpoint("embed-v1", "https://other.example.com/v1/embeddings"), "same text")
        );
    }
}
