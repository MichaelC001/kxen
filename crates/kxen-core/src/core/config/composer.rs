use serde::{Deserialize, Serialize};

/// Composer 主动候选。Local 默认开启，Provider 调用需要显式 opt-in。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ComposerSuggestionsConfig {
    pub enabled: bool,
    pub semantic: bool,
    pub llm: bool,
}

impl Default for ComposerSuggestionsConfig {
    fn default() -> Self {
        Self { enabled: true, semantic: false, llm: false }
    }
}

/// Embedding 语义召回。API key 不落 config，复用同 Provider 凭证。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingConfig {
    /// ""（关闭）| "openai" | "openrouter" | "ollama"
    pub provider: String,
    /// 空值使用 Provider 默认模型。
    pub model: String,
    /// 远程必须 HTTPS，HTTP 仅允许 localhost/loopback。
    pub base_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_features_are_opt_in() {
        let config = ComposerSuggestionsConfig::default();
        assert!(config.enabled);
        assert!(!config.semantic);
        assert!(!config.llm);
    }
}
