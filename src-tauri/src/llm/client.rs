//! 统一 client：auth 凭证 -> provider 实例；HTTP client 全局单例。
//! 路由 = 两个订阅特例（anthropic/openai 的 OAuth 形态）+ custom: 用户端点 + providers registry（其余全部）。

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
            "openai" => match crate::auth::credential::credential_for(&store, "openai", model.account.as_deref()) {
                Some(crate::auth::credential::CredentialKind::Oauth { access, account_id, .. }) => crate::llm::openai::OpenAiProvider::new(
                    access.clone(),
                    account_id.clone(),
                    true,
                )
                .stream_chat(&model.model, messages, tools),
                Some(crate::auth::credential::CredentialKind::Api { key, .. }) => {
                    crate::llm::openai::OpenAiProvider::new(key.clone(), None, false).stream_chat(&model.model, messages, tools)
                }
                _ => Box::pin(futures::stream::once(async { Delta::Error("openai credential missing (run doctor)".into()) })),
            },
            other if other.starts_with("custom:") => {
                // 自定义类型提供商：config.toml 给端点+协议，auth.json 给 key（custom:<name>）
                let name = other[7..].to_string();
                let cfg =
                    crate::core::config::Config::load(&crate::core::paths::config_dir().join("config.toml"), None).unwrap_or_default();
                let Some(def) = cfg.custom_providers.get(&name).cloned() else {
                    return Box::pin(futures::stream::once(async move { Delta::Error(format!("custom provider not configured: {name}")) }));
                };
                let Some(crate::auth::credential::CredentialKind::Api { key, .. }) = store.get(other) else {
                    return Box::pin(futures::stream::once(async move { Delta::Error(format!("custom provider {name} missing api key")) }));
                };
                if def.protocol == "anthropic" {
                    crate::llm::anthropic::AnthropicProvider::custom(
                        format!("{}/v1/messages", def.base_url.trim_end_matches('/')),
                        key.clone(),
                    )
                    .stream_chat(&model.model, messages, tools)
                } else {
                    crate::llm::xai::XaiProvider::custom(format!("{}/chat/completions", def.base_url.trim_end_matches('/')), key.clone())
                        .stream_chat_with_tools(&model.model, messages, tools)
                }
            }
            p => {
                // registry 驱动的统一路径：端点来自 spec（region 跟随凭证），wire 复用 OpenAI 兼容薄实现
                let Some(spec) = crate::providers::find(p) else {
                    let provider = p.to_string();
                    return Box::pin(futures::stream::once(async move { Delta::Error(format!("unknown provider: {provider}")) }));
                };
                let cred = crate::auth::credential::credential_for(&store, p, model.account.as_deref());
                let bearer = match (spec.auth, cred) {
                    // 本地免鉴权端点的 bearer 仅为占位（ollama 不校验）
                    (crate::providers::AuthKind::LocalFree, _) => p.to_string(),
                    (_, Some(c)) => c.bearer().to_string(),
                    _ => {
                        let (provider, hint) = match spec.auth {
                            crate::providers::AuthKind::Oauth => (p.to_string(), "run doctor"),
                            _ => (p.to_string(), "import API key in settings"),
                        };
                        return Box::pin(futures::stream::once(
                            async move { Delta::Error(format!("{provider} credential missing ({hint})")) },
                        ));
                    }
                };
                let url = spec.chat_url(cred.and_then(|c| c.region()));
                crate::llm::xai::XaiProvider::custom(url, bearer).stream_chat_with_tools(&model.model, messages, tools)
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
