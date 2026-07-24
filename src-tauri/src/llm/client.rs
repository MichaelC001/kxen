//! 统一 client：auth 凭证 -> provider 实例；HTTP client 全局单例。

use crate::llm::types::{Delta, Message, ModelRef};
use futures::Stream;
use std::pin::Pin;

pub struct LlmClient;

impl LlmClient {
    /// 按模型引用构造流式调用。凭证从 auth store 取（调用方保证已 probe 导入）。
    pub fn stream(
        model: &ModelRef,
        messages: &[Message],
        store: &crate::auth::credential::AuthStore,
    ) -> Pin<Box<dyn Stream<Item = Delta> + Send>> {
        Self::stream_with_tools(model, messages, &[], store)
    }

    pub fn stream_with_tools(
        model: &ModelRef,
        messages: &[Message],
        tools: &[crate::llm::tool::ToolDefinition],
        store: &crate::auth::credential::AuthStore,
    ) -> Pin<Box<dyn Stream<Item = Delta> + Send>> {
        match model.provider.as_str() {
            "anthropic" => {
                let Some(crate::auth::credential::CredentialKind::Oauth { access, .. }) =
                    crate::auth::credential::credential_for(&store, "anthropic", model.account.as_deref())
                else {
                    return Box::pin(futures::stream::once(async { Delta::Error("anthropic credential missing (run doctor)".into()) }));
                };
                crate::llm::anthropic::AnthropicProvider::new(access.clone()).stream_chat(&model.model, messages, tools)
            }
            "openai" => {
                match crate::auth::credential::credential_for(&store, "openai", model.account.as_deref()) {
                    Some(crate::auth::credential::CredentialKind::Oauth { access, account_id, .. }) => {
                        crate::llm::openai::OpenAiProvider::new(access.clone(), account_id.clone(), true).stream_chat(&model.model, messages, tools)
                    }
                    Some(crate::auth::credential::CredentialKind::Api { key }) => {
                        crate::llm::openai::OpenAiProvider::new(key.clone(), None, false).stream_chat(&model.model, messages, tools)
                    }
                    _ => Box::pin(futures::stream::once(async { Delta::Error("openai credential missing (run doctor)".into()) })),
                }
            }
            "kimi-for-coding" => {
                let key = match crate::auth::credential::credential_for(&store, "kimi-for-coding", model.account.as_deref()) {
                    Some(crate::auth::credential::CredentialKind::Api { key }) => key.clone(),
                    Some(crate::auth::credential::CredentialKind::Oauth { access, .. }) => access.clone(),
                    _ => {
                        return Box::pin(futures::stream::once(async { Delta::Error("kimi credential missing (run doctor)".into()) }));
                    }
                };
                crate::llm::xai::XaiProvider::kimi(key).stream_chat_with_tools(&model.model, messages, tools)
            }
            "xai" => {
                match crate::auth::credential::credential_for(&store, "xai", model.account.as_deref()) {
                    Some(crate::auth::credential::CredentialKind::Oauth { access, .. }) => {
                        crate::llm::xai::XaiProvider::new(access.clone()).stream_chat_with_tools(&model.model, messages, tools)
                    }
                    Some(crate::auth::credential::CredentialKind::Api { key }) => {
                        crate::llm::xai::XaiProvider::new(key.clone()).stream_chat_with_tools(&model.model, messages, tools)
                    }
                    _ => Box::pin(futures::stream::once(async { Delta::Error("xai credential is not oauth".into()) })),
                }
            }
            "openrouter" => {
                let Some(crate::auth::credential::CredentialKind::Api { key }) = crate::auth::credential::credential_for(&store, "openrouter", model.account.as_deref()) else {
                    return Box::pin(futures::stream::once(async { Delta::Error("openrouter credential missing (import API key in settings)".into()) }));
                };
                crate::llm::xai::XaiProvider::custom("https://openrouter.ai/api/v1/chat/completions".into(), key.clone()).stream_chat_with_tools(&model.model, messages, tools)
            }
            "ollama" => {
                // 本地 Ollama 无鉴权，OpenAI 兼容端点的 bearer 仅为占位
                crate::llm::xai::XaiProvider::custom("http://localhost:11434/v1/chat/completions".to_string(), "ollama".to_string()).stream_chat_with_tools(&model.model, messages, tools)
            }
            p if crate::llm::compat::preset(p).is_some() => {
                // 内置 OpenAI 兼容预设：与自定义提供商同路径，仅端点来自预设表
                let provider = p.to_string();
                let Some(crate::auth::credential::CredentialKind::Api { key }) = crate::auth::credential::credential_for(&store, p, model.account.as_deref()) else {
                    return Box::pin(futures::stream::once(async move { Delta::Error(format!("{provider} credential missing (import API key in settings)")) }));
                };
                let url = crate::llm::compat::chat_url(p).expect("preset checked");
                crate::llm::xai::XaiProvider::custom(url, key.clone()).stream_chat_with_tools(&model.model, messages, tools)
            }
            other if other.starts_with("custom:") => {
                // 自定义类型提供商：config.toml 给端点+协议，auth.json 给 key（custom:<name>）
                let name = other[7..].to_string();
                let cfg = crate::core::config::Config::load(&crate::core::paths::config_dir().join("config.toml"), None).unwrap_or_default();
                let Some(def) = cfg.custom_providers.get(&name).cloned() else {
                    return Box::pin(futures::stream::once(async move { Delta::Error(format!("custom provider not configured: {name}")) }));
                };
                let Some(crate::auth::credential::CredentialKind::Api { key }) = store.get(other) else {
                    return Box::pin(futures::stream::once(async move { Delta::Error(format!("custom provider {name} missing api key")) }));
                };
                if def.protocol == "anthropic" {
                    crate::llm::anthropic::AnthropicProvider::custom(format!("{}/v1/messages", def.base_url.trim_end_matches('/')), key.clone()).stream_chat(&model.model, messages, tools)
                } else {
                    crate::llm::xai::XaiProvider::custom(format!("{}/chat/completions", def.base_url.trim_end_matches('/')), key.clone()).stream_chat_with_tools(&model.model, messages, tools)
                }
            }
            other => {
                let provider = other.to_string();
                Box::pin(futures::stream::once(async move {
                    Delta::Error(format!("provider not implemented yet: {provider} (M1 is xai-only)"))
                }))
            }
        }
    }
}

/// 全局 HTTP client（连接池复用）。
pub(crate) fn shared_http() -> reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent(concat!("kxen/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("http client")
        })
        .clone()
}
