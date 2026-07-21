//! 统一 client：auth 凭证 -> provider 实例；HTTP client 全局单例。

use crate::types::{Delta, Message, ModelRef};
use futures::Stream;
use std::pin::Pin;

pub struct LlmClient;

impl LlmClient {
    /// 按模型引用构造流式调用。凭证从 auth store 取（调用方保证已 probe 导入）。
    pub fn stream(
        model: &ModelRef,
        messages: &[Message],
        store: &kxen_auth::credential::AuthStore,
    ) -> Pin<Box<dyn Stream<Item = Delta> + Send>> {
        match model.provider.as_str() {
            "xai" => {
                let Some(cred) = store.get("xai") else {
                    return Box::pin(futures::stream::once(async { Delta::Error("xai credential missing (run doctor)".into()) }));
                };
                let kxen_auth::credential::CredentialKind::Oauth { access, .. } = cred else {
                    return Box::pin(futures::stream::once(async { Delta::Error("xai credential is not oauth".into()) }));
                };
                crate::xai::XaiProvider::new(access.clone()).stream_chat(&model.model, messages)
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
