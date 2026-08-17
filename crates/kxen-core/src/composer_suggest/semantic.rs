use crate::knowledge::embedding::{EmbeddingRuntime, Endpoint};
use crate::knowledge::embedding_cache::EmbeddingCache;
use std::collections::HashMap;
use std::path::Path;

static CACHE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn semantic_scores(
    workspace: &Path,
    endpoint: &Endpoint,
    query: &str,
    docs: &[String],
    runtime: &EmbeddingRuntime,
) -> Result<Vec<Option<f64>>, String> {
    let _guard = CACHE_LOCK.lock().await;
    let path = cache_path(workspace);
    let mut cache = EmbeddingCache::load(&path)?;
    let prefix = format!("{}:{}:", endpoint.provider, endpoint.model);
    let query_hash = namespaced_hash(&prefix, query);
    let doc_hashes: Vec<String> = docs.iter().map(|doc| namespaced_hash(&prefix, doc)).collect();
    let mut missing = HashMap::new();
    if !cache.contains(&query_hash) {
        missing.insert(query_hash.clone(), query.to_string());
    }
    for (hash, doc) in doc_hashes.iter().zip(docs) {
        if !cache.contains(hash) {
            missing.entry(hash.clone()).or_insert_with(|| doc.clone());
        }
    }
    if !missing.is_empty() {
        let mut missing: Vec<(String, String)> = missing.into_iter().collect();
        missing.sort_by(|a, b| a.0.cmp(&b.0));
        let texts: Vec<String> = missing.iter().map(|(_, text)| text.clone()).collect();
        let vectors = crate::knowledge::embedding::embed_managed(endpoint, &texts, runtime).await?;
        if vectors.len() != missing.len() {
            return Err("composer embedding response count mismatch".into());
        }
        for ((hash, _), vector) in missing.into_iter().zip(vectors) {
            cache.insert(hash, vector);
        }
        cache.save()?;
    }
    cache.cosine_scores(&query_hash, &doc_hashes).ok_or_else(|| "composer embedding cache is incomplete".into())
}

fn cache_path(workspace: &Path) -> std::path::PathBuf {
    use sha2::Digest;
    let root = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    let digest = sha2::Sha256::digest(root.to_string_lossy().as_bytes());
    crate::core::paths::KxenPaths::user().composer_suggestion_cache(&crate::core::shared::hex_lower(&digest))
}

fn namespaced_hash(prefix: &str, text: &str) -> String {
    crate::knowledge::embedding::content_hash(&format!("{prefix}{text}"))
}
