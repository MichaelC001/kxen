//! provider registry：全部内置提供商的静态声明表 + 查找 API。
//! 新增/调整提供商只改本目录（spec/seeds/本表），路由/目录/verify/设置页 RPC 自动跟随。
//! base URL 与 models.dev api.json 核对过（2026-07 快照）；minimax 例外见条目注释。

mod seeds;
mod spec;

pub use spec::{AuthKind, Protocol, ProviderSpec, RegionSpec, StaticModel};

use AuthKind::{ApiKey, LocalFree, Oauth};
use Protocol::{Anthropic, OpenAiCompat};

const CN: &str = "中国版";
const INTL: &str = "国际版";
const GL: &str = "全球";

/// 全部内置提供商（顺序 = 设置页展示顺序：订阅四家 -> 聚合/本地 -> 国际 API -> 国内 API）。
pub static REGISTRY: &[ProviderSpec] = &[
    ProviderSpec { key: "anthropic", display: "Anthropic", protocol: Anthropic, auth: Oauth,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.anthropic.com" }],
        models_endpoint: true, default_model: "claude-sonnet-4-6", doc_url: "https://docs.anthropic.com",
        models_dev: Some("anthropic"), static_models: seeds::ANTHROPIC },
    ProviderSpec { key: "openai", display: "OpenAI", protocol: OpenAiCompat, auth: Oauth,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.openai.com/v1" }],
        models_endpoint: true, default_model: "gpt-5.4", doc_url: "https://platform.openai.com/docs",
        models_dev: Some("openai"), static_models: seeds::OPENAI },
    ProviderSpec { key: "xai", display: "xAI", protocol: OpenAiCompat, auth: Oauth,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.x.ai/v1" }],
        models_endpoint: true, default_model: "grok-build-0.1", doc_url: "https://docs.x.ai",
        models_dev: Some("xai"), static_models: seeds::XAI },
    ProviderSpec { key: "kimi-for-coding", display: "Kimi For Coding", protocol: OpenAiCompat, auth: Oauth,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.kimi.com/coding/v1" }],
        models_endpoint: false, default_model: "kimi-for-coding", doc_url: "https://www.kimi.com/coding",
        models_dev: Some("kimi-for-coding"), static_models: seeds::KIMI_CODING },
    ProviderSpec { key: "openrouter", display: "OpenRouter", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://openrouter.ai/api/v1" }],
        models_endpoint: true, default_model: "openai/gpt-5.4", doc_url: "https://openrouter.ai/docs",
        models_dev: Some("openrouter"), static_models: seeds::OPENROUTER },
    ProviderSpec { key: "ollama", display: "Ollama", protocol: OpenAiCompat, auth: LocalFree,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "http://localhost:11434/v1" }],
        models_endpoint: true, default_model: "llama3.3", doc_url: "https://ollama.com",
        models_dev: None, static_models: seeds::OLLAMA },
    ProviderSpec { key: "deepseek", display: "DeepSeek", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.deepseek.com" }],
        models_endpoint: true, default_model: "deepseek-chat", doc_url: "https://api-docs.deepseek.com",
        models_dev: Some("deepseek"), static_models: seeds::DEEPSEEK },
    ProviderSpec { key: "mistral", display: "Mistral", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.mistral.ai/v1" }],
        models_endpoint: true, default_model: "mistral-large-latest", doc_url: "https://docs.mistral.ai",
        models_dev: Some("mistral"), static_models: seeds::MISTRAL },
    ProviderSpec { key: "groq", display: "Groq", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.groq.com/openai/v1" }],
        models_endpoint: true, default_model: "llama-3.3-70b-versatile", doc_url: "https://console.groq.com/docs",
        models_dev: Some("groq"), static_models: seeds::GROQ },
    ProviderSpec { key: "google", display: "Google Gemini", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://generativelanguage.googleapis.com/v1beta/openai" }],
        models_endpoint: false, default_model: "gemini-2.5-flash", doc_url: "https://ai.google.dev",
        models_dev: Some("google"), static_models: seeds::GOOGLE },
    ProviderSpec { key: "together", display: "Together AI", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.together.xyz/v1" }],
        models_endpoint: true, default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo", doc_url: "https://docs.together.ai",
        models_dev: Some("togetherai"), static_models: seeds::TOGETHER },
    ProviderSpec { key: "kimi", display: "Kimi", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[
            RegionSpec { key: "cn", display: CN, base_url: "https://api.moonshot.cn/v1" },
            RegionSpec { key: "intl", display: INTL, base_url: "https://api.moonshot.ai/v1" },
        ],
        models_endpoint: true, default_model: "kimi-k2.5", doc_url: "https://platform.moonshot.cn/docs",
        models_dev: Some("moonshotai"), static_models: seeds::KIMI },
    ProviderSpec { key: "zhipu", display: "智谱 GLM", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[
            RegionSpec { key: "cn", display: CN, base_url: "https://open.bigmodel.cn/api/paas/v4" },
            RegionSpec { key: "intl", display: INTL, base_url: "https://api.z.ai/api/paas/v4" },
        ],
        models_endpoint: true, default_model: "glm-4.6", doc_url: "https://docs.bigmodel.cn",
        models_dev: Some("zhipuai"), static_models: seeds::ZHIPU },
    ProviderSpec { key: "qwen", display: "通义千问", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[
            RegionSpec { key: "cn", display: CN, base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1" },
            RegionSpec { key: "intl", display: INTL, base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1" },
        ],
        models_endpoint: true, default_model: "qwen3-max", doc_url: "https://help.aliyun.com/zh/model-studio",
        models_dev: Some("alibaba-cn"), static_models: seeds::QWEN },
    // models.dev 的 api 字段给的是 Anthropic 协议端点（/anthropic/v1，npm @ai-sdk/anthropic）；
    // 这里用 MiniMax 官方文档的 OpenAI 兼容端点（同 host root 的 /v1），与仓库的 OpenAI 兼容薄层对齐
    ProviderSpec { key: "minimax", display: "MiniMax", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[
            RegionSpec { key: "cn", display: CN, base_url: "https://api.minimaxi.com/v1" },
            RegionSpec { key: "intl", display: INTL, base_url: "https://api.minimax.io/v1" },
        ],
        models_endpoint: true, default_model: "MiniMax-M2.5", doc_url: "https://platform.minimaxi.com/document",
        models_dev: Some("minimax"), static_models: seeds::MINIMAX },
    ProviderSpec { key: "siliconflow", display: "硅基流动", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[
            RegionSpec { key: "cn", display: CN, base_url: "https://api.siliconflow.cn/v1" },
            RegionSpec { key: "intl", display: INTL, base_url: "https://api.siliconflow.com/v1" },
        ],
        models_endpoint: true, default_model: "deepseek-ai/DeepSeek-V3.2", doc_url: "https://docs.siliconflow.cn",
        models_dev: Some("siliconflow-cn"), static_models: seeds::SILICONFLOW },
    ProviderSpec { key: "stepfun", display: "阶跃星辰", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[
            RegionSpec { key: "cn", display: CN, base_url: "https://api.stepfun.com/v1" },
            RegionSpec { key: "intl", display: INTL, base_url: "https://api.stepfun.ai/v1" },
        ],
        models_endpoint: true, default_model: "step-3.5-flash", doc_url: "https://platform.stepfun.com/docs",
        models_dev: Some("stepfun"), static_models: seeds::STEPFUN },
    ProviderSpec { key: "doubao", display: "豆包", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://ark.cn-beijing.volces.com/api/v3" }],
        models_endpoint: true, default_model: "doubao-seed-1-6-250615", doc_url: "https://www.volcengine.com/docs/82379",
        models_dev: None, static_models: seeds::DOUBAO },
    ProviderSpec { key: "yi", display: "零一万物", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.lingyiwanwu.com/v1" }],
        models_endpoint: true, default_model: "yi-lightning", doc_url: "https://platform.lingyiwanwu.com/docs",
        models_dev: None, static_models: seeds::YI },
    ProviderSpec { key: "hunyuan", display: "腾讯混元", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.hunyuan.cloud.tencent.com/v1" }],
        models_endpoint: true, default_model: "hunyuan-turbos-latest", doc_url: "https://cloud.tencent.com/document/product/1729",
        models_dev: None, static_models: seeds::HUNYUAN },
    ProviderSpec { key: "qianfan", display: "百度千帆", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://qianfan.baidubce.com/v2" }],
        models_endpoint: true, default_model: "ernie-4.5-turbo-128k", doc_url: "https://cloud.baidu.com/doc/WENXINWORKSHOP",
        models_dev: None, static_models: seeds::QIANFAN },
    ProviderSpec { key: "fireworks", display: "Fireworks", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.fireworks.ai/inference/v1" }],
        models_endpoint: true, default_model: "accounts/fireworks/models/gpt-oss-120b", doc_url: "https://docs.fireworks.ai",
        models_dev: Some("fireworks-ai"), static_models: seeds::FIREWORKS },
    ProviderSpec { key: "cerebras", display: "Cerebras", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.cerebras.ai/v1" }],
        models_endpoint: true, default_model: "gpt-oss-120b", doc_url: "https://inference-docs.cerebras.ai",
        models_dev: Some("cerebras"), static_models: seeds::CEREBRAS },
    ProviderSpec { key: "sambanova", display: "SambaNova", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.sambanova.ai/v1" }],
        models_endpoint: true, default_model: "Meta-Llama-3.3-70B-Instruct", doc_url: "https://docs.sambanova.ai",
        models_dev: None, static_models: seeds::SAMBANOVA },
    ProviderSpec { key: "perplexity", display: "Perplexity", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.perplexity.ai" }],
        models_endpoint: false, default_model: "sonar", doc_url: "https://docs.perplexity.ai",
        models_dev: Some("perplexity"), static_models: seeds::PERPLEXITY },
    // OpenAI 兼容端点 = compatibility 层（cohere 原生 v2 非 OpenAI 形态）；该层未文档化 /models
    ProviderSpec { key: "cohere", display: "Cohere", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.cohere.ai/compatibility/v1" }],
        models_endpoint: false, default_model: "command-a-03-2025", doc_url: "https://docs.cohere.com",
        models_dev: Some("cohere"), static_models: seeds::COHERE },
    ProviderSpec { key: "github_models", display: "GitHub Models", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://models.github.ai/inference" }],
        models_endpoint: true, default_model: "openai/gpt-4.1-mini", doc_url: "https://docs.github.com/en/github-models",
        models_dev: Some("github-models"), static_models: seeds::GITHUB_MODELS },
    ProviderSpec { key: "novita", display: "Novita", protocol: OpenAiCompat, auth: ApiKey,
        regions: &[RegionSpec { key: "global", display: GL, base_url: "https://api.novita.ai/openai" }],
        models_endpoint: true, default_model: "deepseek/deepseek-v3.1", doc_url: "https://novita.ai/docs",
        models_dev: Some("novita-ai"), static_models: seeds::NOVITA },
];

/// 按 key 查找（kxen provider key，如 "kimi" / "kimi-for-coding"）。
pub fn find(key: &str) -> Option<&'static ProviderSpec> {
    REGISTRY.iter().find(|s| s.key == key)
}

/// 全表（catalog / provider.list RPC / 测试门禁用）。
pub fn all() -> &'static [ProviderSpec] {
    REGISTRY
}
