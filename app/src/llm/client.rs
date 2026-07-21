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
                let Some(crate::auth::credential::CredentialKind::Oauth { access, .. }) = store.get("anthropic") else {
                    return Box::pin(futures::stream::once(async { Delta::Error("anthropic credential missing (run doctor)".into()) }));
                };
                crate::llm::anthropic::AnthropicProvider::new(access.clone()).stream_chat(&model.model, messages, tools)
            }
            "openai" => {
                let Some(crate::auth::credential::CredentialKind::Oauth { access, account_id, .. }) = store.get("openai") else {
                    return Box::pin(futures::stream::once(async { Delta::Error("openai credential missing (run doctor)".into()) }));
                };
                crate::llm::openai::OpenAiProvider::new(access.clone(), account_id.clone(), true).stream_chat(&model.model, messages, tools)
            }
            "kimi-for-coding" => {
                let key = match store.get("kimi-for-coding") {
                    Some(crate::auth::credential::CredentialKind::Api { key }) => key.clone(),
                    Some(crate::auth::credential::CredentialKind::Oauth { access, .. }) => access.clone(),
                    _ => {
                        return Box::pin(futures::stream::once(async { Delta::Error("kimi credential missing (run doctor)".into()) }));
                    }
                };
                crate::llm::xai::XaiProvider::kimi(key).stream_chat_with_tools(&model.model, messages, tools)
            }
            "xai" => {
                let Some(cred) = store.get("xai") else {
                    return Box::pin(futures::stream::once(async { Delta::Error("xai credential missing (run doctor)".into()) }));
                };
                let crate::auth::credential::CredentialKind::Oauth { access, .. } = cred else {
                    return Box::pin(futures::stream::once(async { Delta::Error("xai credential is not oauth".into()) }));
                };
                crate::llm::xai::XaiProvider::new(access.clone()).stream_chat_with_tools(&model.model, messages, tools)
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
