//! 内置 OpenAI 兼容预设（G7：主流服务免走自定义提供商配置）：base URL 单一来源，
//! client / models / verify 共用；协议复用 xai.rs 的 OpenAI 兼容薄实现（bearer + SSE）。

/// 预设条目：kxen provider key + 官方 OpenAI 兼容 base（chat/models 端点由 base 派生）。
pub struct CompatPreset {
    pub provider: &'static str,
    pub display: &'static str,
    pub base_url: &'static str,
    /// verify 的默认 ping 模型（与 catalog 静态兜底首选项对齐）
    pub default_model: &'static str,
}

pub const PRESETS: &[CompatPreset] = &[
    CompatPreset { provider: "deepseek", display: "DeepSeek", base_url: "https://api.deepseek.com", default_model: "deepseek-chat" },
    CompatPreset { provider: "mistral", display: "Mistral", base_url: "https://api.mistral.ai/v1", default_model: "mistral-large-latest" },
    CompatPreset { provider: "groq", display: "Groq", base_url: "https://api.groq.com/openai/v1", default_model: "llama-3.3-70b-versatile" },
    CompatPreset { provider: "google", display: "Google Gemini", base_url: "https://generativelanguage.googleapis.com/v1beta/openai", default_model: "gemini-2.5-flash" },
    CompatPreset { provider: "together", display: "Together AI", base_url: "https://api.together.xyz/v1", default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo" },
];

pub fn preset(provider: &str) -> Option<&'static CompatPreset> {
    PRESETS.iter().find(|p| p.provider == provider)
}

/// chat completions 完整端点（base 不带尾斜杠，直接拼路径）。
pub fn chat_url(provider: &str) -> Option<String> {
    preset(provider).map(|p| format!("{}/chat/completions", p.base_url))
}

/// /models 清单端点。Gemini OpenAI 兼容层官方文档只覆盖 chat/embeddings，
/// 未暴露 /models，返回 None 走静态目录兜底。
pub fn models_url(provider: &str) -> Option<String> {
    match provider {
        "google" => None,
        _ => preset(provider).map(|p| format!("{}/models", p.base_url)),
    }
}

/// verify 的默认 ping 模型（预设外返回 None，由调用方回落静态表）。
pub fn default_model(provider: &str) -> Option<&'static str> {
    preset(provider).map(|p| p.default_model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_have_https_bases_without_trailing_slash() {
        assert_eq!(PRESETS.len(), 5);
        for p in PRESETS {
            assert!(p.base_url.starts_with("https://"), "{} base 必须 https", p.provider);
            assert!(!p.base_url.ends_with('/'), "{} base 不带尾斜杠", p.provider);
            assert!(!p.default_model.is_empty(), "{} 缺默认 ping 模型", p.provider);
            assert!(!p.display.is_empty());
        }
    }

    #[test]
    fn chat_url_construction() {
        assert_eq!(chat_url("deepseek").as_deref(), Some("https://api.deepseek.com/chat/completions"));
        assert_eq!(chat_url("mistral").as_deref(), Some("https://api.mistral.ai/v1/chat/completions"));
        assert_eq!(chat_url("groq").as_deref(), Some("https://api.groq.com/openai/v1/chat/completions"));
        assert_eq!(chat_url("google").as_deref(), Some("https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"));
        assert_eq!(chat_url("together").as_deref(), Some("https://api.together.xyz/v1/chat/completions"));
        assert!(chat_url("unknown-x").is_none());
    }

    #[test]
    fn models_url_construction() {
        assert_eq!(models_url("deepseek").as_deref(), Some("https://api.deepseek.com/models"));
        assert_eq!(models_url("mistral").as_deref(), Some("https://api.mistral.ai/v1/models"));
        assert_eq!(models_url("groq").as_deref(), Some("https://api.groq.com/openai/v1/models"));
        assert_eq!(models_url("together").as_deref(), Some("https://api.together.xyz/v1/models"));
        assert!(models_url("google").is_none(), "gemini 兼容层未暴露 /models");
        assert!(models_url("unknown-x").is_none());
    }

    #[test]
    fn default_models_are_preset_only() {
        assert_eq!(default_model("google"), Some("gemini-2.5-flash"));
        assert!(default_model("anthropic").is_none(), "订阅制不走预设表");
    }
}
