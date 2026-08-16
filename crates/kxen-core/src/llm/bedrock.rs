//! AWS Bedrock provider：Converse Stream API（eventstream 二进制响应）+ SigV4-HMAC-SHA256 签名。
//! 凭证形态：auth.json 的 Api key 槽存 JSON 文档
//! {"access_key_id","secret_access_key","session_token"?,"region"?}（region 缺省 us-east-1）。
//! 契约对照 AWS 文档「ConverseStream」与「Signature Version 4」；不引 aws-sdk，签名见 sigv4.rs。

use crate::llm::tool::ToolDefinition;
use crate::llm::types::{Delta, Message, ModelRef};
use futures::StreamExt;
use std::pin::Pin;

mod sigv4;
mod stream;
mod wire;

const DEFAULT_REGION: &str = "us-east-1";

/// 凭证 JSON -> 签名凭证 + region（key 里缺 region 时回落缺省）。
fn parse_credentials(raw: &str) -> Result<(sigv4::Credentials, String), String> {
    let credentials: sigv4::Credentials = serde_json::from_str(raw)
        .map_err(|_| "bedrock credential must be a JSON object with access_key_id / secret_access_key".to_string())?;
    if credentials.access_key_id.trim().is_empty() || credentials.secret_access_key.trim().is_empty() {
        return Err("bedrock credential requires access_key_id and secret_access_key".into());
    }
    let region = credentials.region.as_deref().map(str::trim).filter(|r| !r.is_empty()).unwrap_or(DEFAULT_REGION).to_string();
    Ok((credentials, region))
}

/// 签名后的请求要素（纯函数，单测可钉死）：URL + 头表 + body。
fn signed_request(
    credentials: &sigv4::Credentials,
    region: &str,
    model: &str,
    body: &serde_json::Value,
    amz_date: &str,
    date_stamp: &str,
) -> (String, Vec<(String, String)>, Vec<u8>) {
    let host = format!("bedrock-runtime.{region}.amazonaws.com");
    let path = format!("/model/{}/converse-stream", sigv4::uri_encode(model));
    let payload = serde_json::to_vec(body).expect("converse request serializes");
    let payload_hash = {
        use sha2::Digest;
        let digest = sha2::Sha256::digest(&payload);
        digest.iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    let mut headers: Vec<(String, String)> = vec![
        ("content-type".into(), "application/json".into()),
        ("host".into(), host.clone()),
        ("x-amz-content-sha256".into(), payload_hash),
        ("x-amz-date".into(), amz_date.to_string()),
    ];
    if let Some(token) = credentials.session_token.as_deref().filter(|t| !t.trim().is_empty()) {
        headers.push(("x-amz-security-token".into(), token.to_string()));
    }
    let header_refs: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let authorization = sigv4::sign(
        credentials,
        &sigv4::SignRequest {
            method: "POST",
            path: &path,
            query: &[],
            headers: &header_refs,
            payload: &payload,
            amz_date,
            date_stamp,
            region,
            service: "bedrock",
        },
    );
    headers.push(("authorization".into(), authorization));
    headers.push(("accept".into(), "application/vnd.amazon.eventstream".into()));
    (format!("https://{host}{path}"), headers, payload)
}

/// client.rs 分派入口（凭证查找 + 解析 + 请求），保持 client.rs 只加一行 match 臂。
pub fn stream(
    model: &ModelRef,
    messages: &[Message],
    tools: &[ToolDefinition],
    store: &crate::auth::credential::AuthStore,
) -> Pin<Box<dyn futures::Stream<Item = Delta> + Send>> {
    let resolved = (|| {
        let cred = crate::auth::credential::credential_for(store, "bedrock", model.account.as_deref())
            .ok_or("bedrock credential missing (import API key in settings)".to_string())?;
        let crate::auth::credential::CredentialKind::Api { key, .. } = cred else {
            return Err("bedrock credential must be an API key JSON document".into());
        };
        parse_credentials(key)
    })();
    let (credentials, region) = match resolved {
        Ok(pair) => pair,
        Err(error) => return Box::pin(futures::stream::once(async move { Delta::Error(error) })),
    };
    let now = chrono::Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let body = wire::build_request(messages, tools);
    let (url, headers, payload) = signed_request(&credentials, &region, &model.model, &body, &amz_date, &date_stamp);
    let http = crate::llm::client::shared_http();
    let error_secret = credentials.secret_access_key.clone();

    let start = async move {
        let mut request = http.post(url).body(payload);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        request.send().await
    };

    Box::pin(futures::stream::once(start).flat_map(move |result| match result {
        Ok(resp) if resp.status().is_success() => stream::stream_events(resp),
        Ok(resp) => {
            let secret = error_secret.clone();
            futures::stream::once(async move { Delta::Error(crate::llm::client::bounded_http_error("bedrock", resp, &[&secret]).await) })
                .boxed()
        }
        Err(error) => {
            let secret = error_secret.clone();
            futures::stream::once(async move {
                Delta::Error(format!(
                    "bedrock request failed: {}",
                    crate::core::net_security::sanitize_authenticated_error(&error, &[&secret])
                ))
            })
            .boxed()
        }
    }))
}

/// 分派前本地校验（client.rs validate_dispatch_in 用）：凭证存在且 JSON 可解析。
pub(crate) fn validate_credential(store: &crate::auth::credential::AuthStore, account: Option<&str>) -> Result<(), String> {
    let cred = crate::auth::credential::credential_for(store, "bedrock", account)
        .ok_or("bedrock credential missing (import API key in settings)".to_string())?;
    let crate::auth::credential::CredentialKind::Api { key, .. } = cred else {
        return Err("bedrock credential must be an API key JSON document".into());
    };
    parse_credentials(key).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::credential::{AuthStore, CredentialKind};

    const DOC: &str = r#"{"access_key_id":"AKIAEXAMPLE","secret_access_key":"secret","region":"eu-west-1"}"#;

    #[test]
    fn credential_json_parses_with_region_fallback() {
        let (creds, region) = parse_credentials(DOC).unwrap();
        assert_eq!(creds.access_key_id, "AKIAEXAMPLE");
        assert_eq!(region, "eu-west-1");
        let (_, region) = parse_credentials(r#"{"access_key_id":"A","secret_access_key":"S"}"#).unwrap();
        assert_eq!(region, DEFAULT_REGION);
        for bad in ["not-json", r#"{"access_key_id":"","secret_access_key":"S"}"#, r#"{"access_key_id":"A"}"#] {
            assert!(parse_credentials(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn signed_request_url_and_headers_are_deterministic() {
        let (creds, region) = parse_credentials(DOC).unwrap();
        let body = wire::build_request(&[Message::user("hi")], &[]);
        let (url, headers, _) =
            signed_request(&creds, &region, "anthropic.claude-sonnet-4-5-20250929-v1:0", &body, "20260101T000000Z", "20260101");
        assert_eq!(
            url,
            "https://bedrock-runtime.eu-west-1.amazonaws.com/model/anthropic.claude-sonnet-4-5-20250929-v1%3A0/converse-stream"
        );
        let find = |name: &str| headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
        let authorization = find("authorization").unwrap();
        assert!(
            authorization.starts_with("AWS4-HMAC-SHA256 Credential=AKIAEXAMPLE/20260101/eu-west-1/bedrock/aws4_request, "),
            "{authorization}"
        );
        assert!(authorization.contains("SignedHeaders=content-type;host;x-amz-content-sha256;x-amz-date"), "{authorization}");
        assert!(find("x-amz-security-token").is_none(), "无 session token 不得出头");
        let with_token = sigv4::Credentials { session_token: Some("tok".into()), ..creds.clone() };
        let (_, headers, _) = signed_request(&with_token, &region, "m", &body, "20260101T000000Z", "20260101");
        let find = |name: &str| headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
        assert_eq!(find("x-amz-security-token"), Some("tok"));
        assert!(find("authorization").unwrap().contains("x-amz-security-token"), "session token 必须进签名头表");
    }

    #[test]
    fn validate_credential_rejects_missing_and_malformed() {
        let store = AuthStore::default();
        assert!(validate_credential(&store, None).unwrap_err().contains("missing"));
        let mut store = AuthStore::default();
        store.insert("bedrock".into(), CredentialKind::Api { key: "not-json".into(), region: None });
        assert!(validate_credential(&store, None).unwrap_err().contains("JSON"));
        store.insert("bedrock".into(), CredentialKind::Api { key: DOC.into(), region: None });
        validate_credential(&store, None).expect("valid document");
    }

    #[test]
    fn dispatch_without_credential_yields_error_delta() {
        let model = ModelRef::new("bedrock", "anthropic.claude-sonnet-4-5-20250929-v1:0");
        let store = AuthStore::default();
        let deltas: Vec<Delta> = futures::executor::block_on_stream(stream(&model, &[Message::user("hi")], &[], &store)).collect();
        assert!(matches!(&deltas[0], Delta::Error(e) if e.contains("bedrock credential missing")));
    }
}
