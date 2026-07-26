//! websearch 工具：引擎链调度。优先级：第三方搜索 API（tavily/brave/exa/jina/serper/serpapi/
//! google/firecrawl/you）-> 模型原生搜索（perplexity sonar/grok live search，模型自带联网能力）
//! -> 自托管 searxng -> 内置 DDG HTML 抓取（脆但零配置，永远殿后）。
//! key 渠道：auth.json 同引擎条目 -> 环境变量；配置：config.toml [search]。

mod apis;
mod ddg;
mod native;

use crate::auth::credential::{AuthStore, CredentialKind};
use crate::core::config::SearchConfig;
use std::pin::Pin;

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const MAX_RESULTS: usize = 8;

/// 单例 client（UA/timeout 定制，与 LLM 共享 client 配置不同，自建池复用）。
fn http() -> reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(TIMEOUT)
                .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) kxen/0.1")
                .build()
                .expect("websearch http client")
        })
        .clone()
}

#[derive(Debug, PartialEq)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// 单次引擎调用的产出：hits 列表 + 可选综合答案（模型原生搜索）。
pub struct EngineResult {
    pub hits: Vec<SearchHit>,
    pub answer: Option<String>,
}

pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    /// 实际产出结果的引擎（降级时与配置不同，调用方需要能看出来）
    pub engine: &'static str,
    pub answer: Option<String>,
}

/// None = 跳过（无 key/配置）；Some(Err) = 尝试了但失败（记错误明细继续降级）。
type TryFuture<'a> = Pin<Box<dyn std::future::Future<Output = Option<Result<EngineResult, String>>> + Send + 'a>>;
type EngineFn = for<'a> fn(&'a str, &'a AuthStore, &'a SearchConfig) -> TryFuture<'a>;

/// 引擎全表 = auto 链序：API 型在前（快且便宜），模型原生居中（有答案但慢且烧 token），
/// searxng 需自托管配置，ddg 永远殿后。
const ENGINES: &[(&str, EngineFn)] = &[
    ("tavily", apis::tavily),
    ("brave", apis::brave),
    ("exa", apis::exa),
    ("jina", apis::jina),
    ("serper", apis::serper),
    ("serpapi", apis::serpapi),
    ("google", apis::google_cse),
    ("firecrawl", apis::firecrawl),
    ("you", apis::youcom),
    ("perplexity", native::perplexity),
    ("grok", native::grok_live),
    ("openai", native::openai_responses),
    ("anthropic", native::anthropic_native),
    ("searxng", apis::searxng),
    ("ddg", ddg::search),
];

/// 引擎候选链：显式配置打头，其余按表序补齐；未知配置按 auto（写错不该让搜索整个挂掉）。
fn engine_chain(configured: &str) -> Vec<&'static str> {
    let mut chain: Vec<&'static str> = Vec::new();
    if !configured.is_empty()
        && let Some(&(first, _)) = ENGINES.iter().find(|(id, _)| *id == configured)
    {
        chain.push(first);
    }
    for &(id, _) in ENGINES {
        if !chain.contains(&id) {
            chain.push(id);
        }
    }
    chain
}

/// key 解析：auth.json 优先（Api.key / OAuth.access 皆可作 bearer），环境变量兜底。
fn api_key(store: &AuthStore, engine: &str, env_names: &[&str]) -> Option<String> {
    crate::auth::credential::credential_for(store, engine, None)
        .and_then(|c| match c {
            CredentialKind::Api { key, .. } if !key.is_empty() => Some(key.to_string()),
            CredentialKind::Oauth { access, .. } if !access.is_empty() => Some(access.to_string()),
            _ => None,
        })
        .or_else(|| env_names.iter().find_map(|n| std::env::var(n).ok().filter(|k| !k.is_empty())))
}

pub async fn search(query: &str, store: &AuthStore) -> Result<SearchOutcome, String> {
    if query.trim().is_empty() {
        return Err("empty query".into());
    }
    let cfg = crate::core::config::Config::load(&crate::core::paths::config_dir().join("config.toml"), None).unwrap_or_default();
    let mut errs: Vec<String> = Vec::new();
    for id in engine_chain(&cfg.search.engine) {
        let f = ENGINES.iter().find(|(eid, _)| *eid == id).map(|(_, f)| f).expect("chain id 必在引擎表");
        match f(query, store, &cfg.search).await {
            None => errs.push(format!("{id}: skipped (no key/config)")),
            Some(Ok(r)) => return Ok(SearchOutcome { hits: r.hits, engine: id, answer: r.answer }),
            Some(Err(e)) => errs.push(format!("{id}: {e}")),
        }
    }
    Err(errs.join("; "))
}

/// POST JSON（可选 Bearer），响应非 2xx 报状态码（不回贴原文，防 key 随错误体回显）。
async fn post_json(
    url: &str,
    bearer: Option<&str>,
    extra_headers: &[(&str, &str)],
    body: &impl serde::Serialize,
) -> Result<String, String> {
    let mut req = http().post(url).json(body);
    if let Some(token) = bearer {
        req = req.bearer_auth(token);
    }
    for (k, v) in extra_headers {
        req = req.header(*k, *v);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("http {}", resp.status()));
    }
    resp.text().await.map_err(|e| e.to_string())
}

/// GET + query 参数 + 自定义头（query 走 reqwest 原生编码，不手工拼 URL）。
async fn get_json(url: &str, headers: &[(&str, &str)], query: &[(&str, &str)]) -> Result<String, String> {
    let mut req = http().get(url).query(query);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("http {}", resp.status()));
    }
    resp.text().await.map_err(|e| e.to_string())
}

pub fn format_hits(outcome: &SearchOutcome) -> String {
    let sources = if outcome.hits.is_empty() {
        String::new()
    } else {
        outcome
            .hits
            .iter()
            .enumerate()
            .map(|(i, h)| format!("{}. {}\n   {}\n   {}", i + 1, h.title, h.url, h.snippet))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    match &outcome.answer {
        Some(answer) => format!("(via {})\n{answer}\n\nSources:\n{sources}", outcome.engine),
        None if sources.is_empty() => format!("no results (via {})", outcome.engine),
        None => format!("(via {})\n{sources}", outcome.engine),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_chain_explicit_first_ddg_last() {
        let auto = engine_chain("");
        assert_eq!(auto.first(), Some(&"tavily"));
        assert_eq!(auto.last(), Some(&"ddg"));
        assert_eq!(auto.len(), ENGINES.len());

        let brave_first = engine_chain("brave");
        assert_eq!(brave_first.first(), Some(&"brave"));
        assert_eq!(brave_first.last(), Some(&"ddg"));
        assert_eq!(brave_first.len(), ENGINES.len());

        // 未知配置按 auto，不挂
        assert_eq!(engine_chain("unknown"), auto);
    }
}
