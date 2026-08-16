//! Google Vertex AI provider：复用 Gemini GenerateContent wire（裸请求 + 裸 SSE 帧），
//! 认证走 service account JSON -> RS256 断言换 OAuth2 access token（见 token.rs）。
//! 凭证形态：auth.json 的 Api key 槽存 service account 导出 JSON（可加 "location" 键，缺省 us-central1）。

use crate::llm::tool::ToolDefinition;
use crate::llm::types::{Delta, Message, ModelRef};
use futures::StreamExt;
use std::pin::Pin;

mod token;

/// 流式端点：global 区域用无前缀 host，其余 {location}-aiplatform.googleapis.com。
fn stream_url(project: &str, location: &str, model: &str) -> String {
    let host = if location == "global" { "aiplatform.googleapis.com".to_string() } else { format!("{location}-aiplatform.googleapis.com") };
    format!("https://{host}/v1/projects/{project}/locations/{location}/publishers/google/models/{model}:streamGenerateContent?alt=sse")
}

/// 已持有 token 的请求段（POST + SSE 解析），供 async 装配后调用。
fn stream_prebuilt(url: String, bearer: String, body: serde_json::Value) -> Pin<Box<dyn futures::Stream<Item = Delta> + Send>> {
    let http = crate::llm::client::shared_http();
    let error_bearer = bearer.clone();
    let start = async move { http.post(url).bearer_auth(bearer).json(&body).send().await };
    Box::pin(futures::stream::once(start).flat_map(move |result| match result {
        Ok(resp) if resp.status().is_success() => crate::llm::gemini::stream_sse_raw(resp),
        Ok(resp) => {
            let error_bearer = error_bearer.clone();
            futures::stream::once(async move {
                Delta::Error(crate::llm::client::bounded_http_error("google-vertex", resp, &[error_bearer.as_ref()]).await)
            })
            .boxed()
        }
        Err(error) => {
            let error_bearer = error_bearer.clone();
            futures::stream::once(async move {
                Delta::Error(format!(
                    "google-vertex request failed: {}",
                    crate::core::net_security::sanitize_authenticated_error(&error, &[error_bearer.as_ref()])
                ))
            })
            .boxed()
        }
    }))
}

/// client.rs 分派入口：凭证解析 + token 获取是 async，消息先编码成 owned body（同 stream_google_oauth 模式）。
pub fn stream(
    model: &ModelRef,
    messages: &[Message],
    tools: &[ToolDefinition],
    store: &crate::auth::credential::AuthStore,
) -> Pin<Box<dyn futures::Stream<Item = Delta> + Send>> {
    let account = (|| {
        let cred = crate::auth::credential::credential_for(store, "google-vertex", model.account.as_deref())
            .ok_or("google-vertex credential missing (import API key in settings)".to_string())?;
        let crate::auth::credential::CredentialKind::Api { key, .. } = cred else {
            return Err("google-vertex credential must be a service account JSON document".into());
        };
        token::parse_service_account(key)
    })();
    let model_id = model.model.clone();
    let body = crate::llm::gemini::generate_content_request(messages, tools);
    let start = async move {
        let account = account?;
        let url = stream_url(&account.project_id, account.location(), &model_id);
        let token = token::access_token(&crate::llm::client::shared_http(), &account).await?;
        Ok(stream_prebuilt(url, token, body))
    };
    Box::pin(futures::stream::once(start).flat_map(
        |result: Result<Pin<Box<dyn futures::Stream<Item = Delta> + Send>>, String>| match result {
            Ok(stream) => stream,
            Err(error) => Box::pin(futures::stream::once(async move { Delta::Error(error) })),
        },
    ))
}

/// 分派前本地校验（client.rs validate_dispatch_in 用）：凭证存在且 service account JSON 可解析。
pub(crate) fn validate_credential(store: &crate::auth::credential::AuthStore, account: Option<&str>) -> Result<(), String> {
    let cred = crate::auth::credential::credential_for(store, "google-vertex", account)
        .ok_or("google-vertex credential missing (import API key in settings)".to_string())?;
    let crate::auth::credential::CredentialKind::Api { key, .. } = cred else {
        return Err("google-vertex credential must be a service account JSON document".into());
    };
    token::parse_service_account(key).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::credential::{AuthStore, CredentialKind};

    #[test]
    fn stream_url_embeds_project_location_and_model() {
        assert_eq!(
            stream_url("proj-1", "us-central1", "gemini-2.5-flash"),
            "https://us-central1-aiplatform.googleapis.com/v1/projects/proj-1/locations/us-central1/publishers/google/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
        // global 区域 host 无 location 前缀
        assert_eq!(
            stream_url("proj-1", "global", "gemini-3-pro-preview"),
            "https://aiplatform.googleapis.com/v1/projects/proj-1/locations/global/publishers/google/models/gemini-3-pro-preview:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn validate_credential_rejects_missing_and_malformed() {
        let store = AuthStore::default();
        assert!(validate_credential(&store, None).unwrap_err().contains("missing"));
        let mut store = AuthStore::default();
        store.insert("google-vertex".into(), CredentialKind::Api { key: "not-json".into(), region: None });
        assert!(validate_credential(&store, None).unwrap_err().contains("service account"));
        store.insert(
            "google-vertex".into(),
            CredentialKind::Api { key: r#"{"client_email":"a@b","private_key":"k","project_id":"p"}"#.into(), region: None },
        );
        validate_credential(&store, None).expect("valid document");
    }

    #[test]
    fn dispatch_without_credential_yields_error_delta() {
        let model = ModelRef::new("google-vertex", "gemini-2.5-flash");
        let store = AuthStore::default();
        let deltas: Vec<Delta> = futures::executor::block_on_stream(stream(&model, &[Message::user("hi")], &[], &store)).collect();
        assert!(matches!(&deltas[0], Delta::Error(e) if e.contains("google-vertex credential missing")));
    }
}
