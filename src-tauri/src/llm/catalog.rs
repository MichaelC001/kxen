//! 模型目录（ModelCatalog）：models.dev 快照为主 + 静态兜底。
//! picker / 路由配置 / 状态栏的单一数据源：内存 -> 磁盘 -> 静态表；24h TTL 惰性后台刷新，
//! 静默失败留旧缓存（models.dev 不可达不阻塞任何功能）。

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

const MODELS_DEV_URL: &str = "https://models.dev/api.json";
const TTL_MS: u64 = 24 * 3600 * 1000;
/// 白名单：四家订阅制 + openrouter/ollama（G7 扩展）+ 五家 OpenAI 兼容预设；自定义端点模型走 provider.models live 通道，不进快照。
/// 三元组：(kxen provider key, models.dev key, display)；key 不一致的（together -> togetherai）在此映射。
const TRACKED: &[(&str, &str, &str)] = &[
    ("anthropic", "anthropic", "Anthropic"),
    ("openai", "openai", "OpenAI"),
    ("xai", "xai", "xAI"),
    ("kimi-for-coding", "kimi-for-coding", "Kimi For Coding"),
    ("openrouter", "openrouter", "OpenRouter"),
    ("ollama", "ollama", "Ollama"),
    ("deepseek", "deepseek", "DeepSeek"),
    ("mistral", "mistral", "Mistral"),
    ("groq", "groq", "Groq"),
    ("google", "google", "Google Gemini"),
    ("together", "togetherai", "Together AI"),
];

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

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn cache_file() -> std::path::PathBuf {
    crate::core::paths::data_dir().join("models-catalog.json")
}

/// 目录读取：内存 -> 磁盘 -> 静态兜底；磁盘过期/缺失时后台刷新（不阻塞调用方）。
pub fn catalog() -> Vec<ProviderCatalog> {
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    if let Some(c) = cache.lock().expect("catalog").as_ref() {
        return c.clone();
    }
    let disk = std::fs::read_to_string(cache_file())
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<ProviderCatalog>>(&text).ok());
    let (out, stale) = match disk {
        Some(c) if !c.is_empty() => {
            let stale = now_ms().saturating_sub(c[0].fetched_at) > TTL_MS;
            (c, stale)
        }
        _ => (static_catalog(), true),
    };
    *cache.lock().expect("catalog") = Some(out.clone());
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
        let mut running = flag.lock().expect("refresh");
        if *running {
            return;
        }
        *running = true;
    }
    tokio::spawn(async move {
        let result = async {
            let resp = crate::llm::client::shared_http()
                .get(MODELS_DEV_URL)
                .timeout(std::time::Duration::from_secs(20))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let text = resp.text().await.map_err(|e| e.to_string())?;
            parse_models_dev(&text).ok_or_else(|| "parse failed".to_string())
        }
        .await;
        match result {
            Ok(c) => {
                if let Ok(json) = serde_json::to_string_pretty(&c) {
                    let _ = std::fs::write(cache_file(), json);
                }
                let cache = CACHE.get_or_init(|| Mutex::new(None));
                *cache.lock().expect("catalog") = Some(c);
                tracing::info!("models.dev catalog refreshed");
            }
            Err(e) => tracing::warn!(error = %e, "models.dev refresh failed (keep old cache)"),
        }
        *flag.lock().expect("refresh") = false;
    });
}

/// models.dev api.json 解析：按 TRACKED 白名单提取（api.json 全量 ~200 provider，只收白名单）。
fn parse_models_dev(text: &str) -> Option<Vec<ProviderCatalog>> {
    let root: serde_json::Value = serde_json::from_str(text).ok()?;
    let ts = now_ms();
    let mut out = Vec::new();
    for (id, dev_id, display) in TRACKED {
        let Some(prov) = root.get(*dev_id) else { continue };
        let provider_name = prov.get("name").and_then(|n| n.as_str()).unwrap_or(display).to_string();
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
        models.sort_by(|a, b| b.context.cmp(&a.context));
        out.push(ProviderCatalog { provider: id.to_string(), provider_name, models, fetched_at: ts, source: "models.dev".into() });
    }
    if out.is_empty() { None } else { Some(out) }
}

/// 静态兜底：models.dev 首次不可达时的最小可用集（与快照同构）。
fn static_catalog() -> Vec<ProviderCatalog> {
    let m = |id: &str, name: &str, ctx: u64, reasoning: bool, attach: bool| ModelInfo {
        id: id.into(),
        name: name.into(),
        family: String::new(),
        reasoning,
        tool_call: true,
        attachment: attach,
        modalities_in: if attach { vec!["text".into(), "image".into()] } else { vec!["text".into()] },
        context: ctx,
        output: 0,
    };
    let ts = now_ms();
    TRACKED
        .iter()
        .map(|(id, _dev_id, display)| {
            let models = match *id {
                "anthropic" => vec![
                    m("claude-opus-4-8", "Claude Opus 4.8", 1_000_000, true, true),
                    m("claude-sonnet-4-6", "Claude Sonnet 4.6", 1_000_000, true, true),
                    m("claude-haiku-4-5", "Claude Haiku 4.5", 200_000, true, true),
                ],
                "openai" => vec![
                    m("gpt-5.4", "GPT-5.4", 1_050_000, true, true),
                    m("gpt-5-codex", "GPT-5-Codex", 400_000, true, true),
                    m("o3", "o3", 200_000, true, true),
                ],
                "xai" => vec![
                    m("grok-4.5", "Grok 4.5", 500_000, true, true),
                    m("grok-4.3", "Grok 4.3", 1_000_000, true, true),
                    m("grok-build-0.1", "Grok Build 0.1", 256_000, true, true),
                ],
                "kimi-for-coding" => vec![
                    m("k3", "Kimi K3", 1_048_576, true, false),
                    m("kimi-for-coding", "Kimi K2.7 Code", 262_144, true, true),
                ],
                "openrouter" => vec![
                    m("anthropic/claude-opus-4.8", "Claude Opus 4.8 (OpenRouter)", 1_000_000, true, true),
                    m("openai/gpt-5.4", "GPT-5.4 (OpenRouter)", 1_050_000, true, true),
                    m("x-ai/grok-4.5", "Grok 4.5 (OpenRouter)", 500_000, true, true),
                ],
                "ollama" => vec![
                    m("llama3.3", "Llama 3.3 70B", 131_072, false, false),
                    m("qwen3", "Qwen3 32B", 131_072, true, false),
                    m("deepseek-r1", "DeepSeek R1", 131_072, true, false),
                ],
                // 以下五家数值取自 models.dev 快照（live 刷新后由快照值覆盖）
                "deepseek" => vec![
                    m("deepseek-chat", "DeepSeek Chat", 1_000_000, false, true),
                    m("deepseek-reasoner", "DeepSeek Reasoner", 1_000_000, true, true),
                    m("deepseek-v4-pro", "DeepSeek V4 Pro", 1_000_000, true, false),
                ],
                "mistral" => vec![
                    m("mistral-large-latest", "Mistral Large (latest)", 262_144, false, true),
                    m("mistral-medium-latest", "Mistral Medium (latest)", 262_144, true, true),
                    m("codestral-latest", "Codestral (latest)", 256_000, false, false),
                    m("magistral-medium-latest", "Magistral Medium (latest)", 128_000, true, false),
                ],
                "groq" => vec![
                    m("llama-3.3-70b-versatile", "Llama 3.3 70B", 131_072, false, false),
                    m("openai/gpt-oss-120b", "GPT OSS 120B", 131_072, true, false),
                    m("qwen/qwen3-32b", "Qwen3-32B", 131_072, true, false),
                    m("meta-llama/llama-4-scout-17b-16e-instruct", "Llama 4 Scout 17B 16E", 131_072, false, true),
                ],
                "google" => vec![
                    m("gemini-2.5-pro", "Gemini 2.5 Pro", 1_048_576, true, true),
                    m("gemini-2.5-flash", "Gemini 2.5 Flash", 1_048_576, true, true),
                    m("gemini-2.5-flash-lite", "Gemini 2.5 Flash-Lite", 1_048_576, true, true),
                    m("gemini-2.0-flash", "Gemini 2.0 Flash", 1_048_576, false, true),
                ],
                "together" => vec![
                    m("meta-llama/Llama-3.3-70B-Instruct-Turbo", "Llama 3.3 70B (Together)", 131_072, false, false),
                    m("deepseek-ai/DeepSeek-V3", "DeepSeek-V3 (Together)", 131_072, false, false),
                    m("deepseek-ai/DeepSeek-R1", "DeepSeek-R1 (Together)", 163_839, true, false),
                    m("Qwen/Qwen3-Coder-480B-A35B-Instruct-FP8", "Qwen3 Coder 480B (Together)", 262_144, false, false),
                ],
                _ => vec![],
            };
            ProviderCatalog { provider: id.to_string(), provider_name: display.to_string(), models, fetched_at: ts, source: "static".into() }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extracts_tracked_providers() {
        let text = r#"{
          "anthropic": {"name": "Anthropic", "models": {"claude-x": {"name": "Claude X", "reasoning": true, "tool_call": true, "attachment": true, "modalities": {"input": ["text", "image"]}, "limit": {"context": 200000, "output": 64000}}}},
          "302ai": {"name": "302.AI", "models": {"foo": {}}},
          "togetherai": {"name": "Together AI", "models": {"m/t": {"name": "T", "limit": {"context": 131072}}}},
          "xai": {"name": "xAI", "models": {"grok-y": {"name": "Grok Y", "limit": {"context": 100000}}}}
        }"#;
        let c = parse_models_dev(text).unwrap();
        assert_eq!(c.len(), 3);
        let ant = c.iter().find(|p| p.provider == "anthropic").unwrap();
        assert_eq!(ant.provider_name, "Anthropic");
        assert_eq!(ant.models[0].name, "Claude X");
        assert!(ant.models[0].reasoning);
        assert_eq!(ant.models[0].context, 200000);
        assert_eq!(ant.models[0].modalities_in, vec!["text", "image"]);
        assert!(!c.iter().any(|p| p.provider == "302ai"), "白名单外不收");
        let tg = c.iter().find(|p| p.provider == "together").expect("models.dev 的 togetherai 映射到 kxen 的 together");
        assert_eq!(tg.provider_name, "Together AI");
        assert_eq!(tg.models[0].context, 131072);
    }

    #[test]
    fn static_catalog_covers_all_tracked() {
        let c = static_catalog();
        assert_eq!(c.len(), TRACKED.len());
        for p in &c {
            assert!(!p.models.is_empty(), "{} 静态兜底为空", p.provider);
            assert!(p.models.iter().all(|m| !m.name.is_empty() && m.context > 0));
        }
    }

    #[test]
    fn static_catalog_has_openrouter_and_ollama() {
        let c = static_catalog();
        let or = c.iter().find(|p| p.provider == "openrouter").expect("openrouter 入表");
        assert!(or.models.iter().any(|m| m.id.contains('/')), "openrouter 模型 id 带 provider 前缀");
        let ol = c.iter().find(|p| p.provider == "ollama").expect("ollama 入表");
        assert!(ol.models.iter().any(|m| m.id == "llama3.3"));
    }

    #[test]
    fn static_catalog_covers_compat_presets() {
        // 静态兜底与 compat 预设对齐：verify 的默认 ping 模型必须在清单内，否则开箱实测必挂
        let c = static_catalog();
        for p in crate::llm::compat::PRESETS {
            let entry = c.iter().find(|x| x.provider == p.provider).unwrap_or_else(|| panic!("{} 入静态兜底", p.provider));
            assert!(entry.models.iter().any(|m| m.id == p.default_model), "{} 静态兜底缺 verify 默认模型 {}", p.provider, p.default_model);
        }
    }
}
