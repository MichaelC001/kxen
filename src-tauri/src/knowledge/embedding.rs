//! 可选 embedding 语义召回（缺省关闭，未配置/调用失败静默回落纯 BM25）。
//! 三档 provider：openai（text-embedding-3-small）、openrouter（同 OpenAI 协议换 base URL）、
//! ollama（/api/embed，nomic-embed-text，本地无鉴权）。
//! 设计：检索路径永远同步、永不阻塞网络——只读磁盘缓存算 cosine；缓存未命中的文本
//! 后台 spawn 预热（本轮 BM25，下轮融合生效）。凭证复用 auth.json 的同 provider 账号。

use super::embedding_cache::EmbeddingCache;
use crate::auth::credential::{credential_for, AuthStore, CredentialKind};
use crate::core::config::EmbeddingConfig;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    OpenAi,
    Ollama,
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub url: String,
    pub key: Option<String>,
    pub model: String,
    pub protocol: Protocol,
    /// ollama 只监听 loopback，走 net_guard 的显式例外（端点来自用户 config，非页面诱导）
    pub allow_loopback: bool,
}

/// 端点解析（纯函数，可测）：缺省 provider 或未知 provider -> None（= 功能关闭）。
/// openai/openrouter 的自定义 base_url 不给 loopback 例外：本地 OpenAI 兼容服务请用 ollama 档。
pub fn resolve_endpoint_with(cfg: &EmbeddingConfig, store: &AuthStore) -> Option<Endpoint> {
    let custom_base = cfg.base_url.trim().trim_end_matches('/');
    match cfg.provider.as_str() {
        "" => None,
        "openai" => {
            let base = if custom_base.is_empty() { "https://api.openai.com/v1" } else { custom_base };
            Some(Endpoint {
                url: format!("{base}/embeddings"),
                key: Some(api_key_of(store, "openai")?),
                model: model_or(cfg, "text-embedding-3-small"),
                protocol: Protocol::OpenAi,
                allow_loopback: false,
            })
        }
        "openrouter" => {
            let base = if custom_base.is_empty() { "https://openrouter.ai/api/v1" } else { custom_base };
            Some(Endpoint {
                url: format!("{base}/embeddings"),
                key: Some(api_key_of(store, "openrouter")?),
                // OpenRouter 的模型 id 带 provider 前缀
                model: model_or(cfg, "openai/text-embedding-3-small"),
                protocol: Protocol::OpenAi,
                allow_loopback: false,
            })
        }
        "ollama" => {
            let base = if custom_base.is_empty() { "http://localhost:11434" } else { custom_base };
            Some(Endpoint {
                url: format!("{base}/api/embed"),
                key: None,
                model: model_or(cfg, "nomic-embed-text"),
                protocol: Protocol::Ollama,
                allow_loopback: true,
            })
        }
        // 配置写错 provider 名按关闭处理：检索不能因配置笔误挂掉
        _ => None,
    }
}

/// 读盘装配：config 只读用户级（~/.config/kxen/config.toml）——与 llm client 读
/// custom_providers 同路径；召回偏好跟人走，项目级 config 入 git 不放这个。
pub fn resolve_endpoint() -> Option<Endpoint> {
    let cfg = crate::core::config::Config::load(&crate::core::paths::config_dir().join("config.toml"), None).ok()?.embedding;
    if cfg.provider.is_empty() {
        return None;
    }
    let store = crate::auth::credential::read_auth_file(&crate::core::paths::auth_file());
    resolve_endpoint_with(&cfg, &store)
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
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn cache_path() -> std::path::PathBuf {
    crate::core::paths::data_dir().join("embedding-cache.json")
}

/// 请求构造（OpenAI 及兼容协议共用）：{"model": ..., "input": [...]}
pub fn build_openai_request(model: &str, texts: &[String]) -> serde_json::Value {
    serde_json::json!({ "model": model, "input": texts })
}

/// OpenAI /embeddings 响应：{"data": [{"embedding": [...]}, ...]}，按 input 序。
pub fn parse_openai_response(body: &str) -> Option<Vec<Vec<f32>>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let data = v.get("data")?.as_array()?;
    data.iter().map(|d| f32_array(d.get("embedding")?)).collect()
}

/// Ollama /api/embed 请求：{"model": ..., "input": [...]}（input 接受数组，批量一次完成）。
pub fn build_ollama_request(model: &str, texts: &[String]) -> serde_json::Value {
    serde_json::json!({ "model": model, "input": texts })
}

/// Ollama /api/embed 响应：{"embeddings": [[...], ...]}
pub fn parse_ollama_response(body: &str) -> Option<Vec<Vec<f32>>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let arr = v.get("embeddings")?.as_array()?;
    arr.iter().map(f32_array).collect()
}

fn f32_array(v: &serde_json::Value) -> Option<Vec<f32>> {
    v.as_array()?.iter().map(|x| x.as_f64().map(|f| f as f32)).collect()
}

/// 检索侧语义分（同步、零网络）：只读磁盘缓存。返回 None = 本轮无语义（未配置或 query
/// 向量未缓存）；Vec 内逐条 Option = 该条目是否有缓存向量。未命中的文本触发后台预热。
pub fn recall(query: &str, docs: &[String]) -> Option<Vec<Option<f64>>> {
    let ep = resolve_endpoint()?;
    let mut cache = EmbeddingCache::load(&cache_path());
    let qvec = cache.get(&content_hash(query)).cloned();
    let mut missing: Vec<String> = Vec::new();
    if qvec.is_none() {
        missing.push(query.to_string());
    }
    let mut out: Vec<Option<f64>> = Vec::with_capacity(docs.len());
    for d in docs {
        match cache.get(&content_hash(d)) {
            Some(v) => out.push(qvec.as_ref().map(|q| super::retrieval::cosine(q, v))),
            None => {
                out.push(None);
                missing.push(d.clone());
            }
        }
    }
    if !missing.is_empty() {
        // 同文重复（同 slug 变体、query 与条目同文）只预热一次
        let mut seen = std::collections::HashSet::new();
        missing.retain(|t| seen.insert(t.clone()));
        spawn_warm(ep, missing);
    }
    qvec?;
    Some(out)
}

/// 后台预热：静态门防并发 stampede（一次最多一个预热任务）；无 tokio runtime
/// （测试/同步上下文）直接跳过——预热是优化不是功能。
fn spawn_warm(ep: Endpoint, texts: Vec<String>) {
    static WARMING: AtomicBool = AtomicBool::new(false);
    let Ok(handle) = tokio::runtime::Handle::try_current() else { return };
    if WARMING.swap(true, Ordering::SeqCst) {
        return;
    }
    handle.spawn(async move {
        if let Err(e) = warm(&ep, &texts).await {
            log_failure_once(&e);
        }
        WARMING.store(false, Ordering::SeqCst);
    });
}

/// 失败只记一次日志：render 每轮都跑，逐轮 warn 会刷屏；静默回落 BM25 是设计行为。
fn log_failure_once(msg: &str) {
    static LOGGED: AtomicBool = AtomicBool::new(false);
    if !LOGGED.swap(true, Ordering::SeqCst) {
        tracing::warn!("embedding recall unavailable, fallback to BM25: {msg}");
    }
}

async fn warm(ep: &Endpoint, texts: &[String]) -> Result<(), String> {
    let mut cache = EmbeddingCache::load(&cache_path());
    // 批量上限：记忆条目几十到几百通常一批就完，chunk 只防极端量级的单请求过大
    for chunk in texts.chunks(96) {
        let vecs = fetch_embeddings(ep, chunk).await?;
        for (t, v) in chunk.iter().zip(vecs) {
            cache.insert(content_hash(t), v);
        }
    }
    cache.save()
}

async fn fetch_embeddings(ep: &Endpoint, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
    if ep.allow_loopback {
        crate::tools::net_guard::check_url_allow_loopback(&ep.url).await?;
    } else {
        crate::tools::net_guard::check_url(&ep.url).await?;
    }
    let body = match ep.protocol {
        Protocol::OpenAi => build_openai_request(&ep.model, texts),
        Protocol::Ollama => build_ollama_request(&ep.model, texts),
    };
    let mut req = crate::llm::client::shared_http()
        .post(&ep.url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(30));
    if let Some(k) = &ep.key {
        req = req.bearer_auth(k);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("embedding http {}", resp.status()));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let vecs = match ep.protocol {
        Protocol::OpenAi => parse_openai_response(&text),
        Protocol::Ollama => parse_ollama_response(&text),
    }
    .ok_or_else(|| "embedding response parse failed".to_string())?;
    if vecs.len() != texts.len() {
        return Err(format!("embedding count mismatch: {} for {} texts", vecs.len(), texts.len()));
    }
    Ok(vecs)
}
