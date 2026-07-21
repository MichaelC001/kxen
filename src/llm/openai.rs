//! OpenAI/Codex provider（ChatGPT Plus/Pro 订阅：backend-api 端点 + account 头）。

use crate::llm::sse::{SseFrame, SseParser};
use crate::llm::types::{Delta, Message, Role};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

const SUBSCRIPTION_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const API_URL: &str = "https://api.openai.com/v1/responses";
const ORIGINATOR: &str = "codex_cli_rs";

pub struct OpenAiProvider {
    http: reqwest::Client,
    bearer: crate::core::shared::SharedStr,
    account_id: Option<crate::core::shared::SharedStr>,
    subscription: bool,
}

#[derive(Serialize)]
struct ResponsesRequest<'a> {
    model: &'a str,
    input: Vec<InputItem<'a>>,
    stream: bool,
    store: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ResponsesTool<'a>>,
}

#[derive(Serialize)]
struct InputItem<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    role: &'a str,
    content: String,
}

#[derive(Serialize)]
struct ResponsesTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    name: &'a str,
    description: &'a str,
    parameters: serde_json::Value,
}

#[derive(Deserialize)]
struct ResponsesEvent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    delta: Option<String>,
    #[serde(default)]
    response: Option<ResponseUsage>,
}

#[derive(Deserialize)]
struct ResponseUsage {
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

impl OpenAiProvider {
    pub fn new(bearer: impl Into<String>, account_id: Option<String>, subscription: bool) -> Self {
        Self {
            http: crate::llm::client::shared_http(),
            bearer: crate::core::shared::SharedStr::from(bearer.into()),
            account_id: account_id.map(crate::core::shared::SharedStr::from),
            subscription,
        }
    }

    pub fn stream_chat(&self, model: &str, messages: &[Message], tools: &[crate::llm::tool::ToolDefinition]) -> Pin<Box<dyn futures::Stream<Item = Delta> + Send>> {
        let bearer = self.bearer.clone();
        let account_id = self.account_id.clone();
        let url = if self.subscription { SUBSCRIPTION_URL } else { API_URL };
        let model = model.to_string();
        let messages_owned: Vec<Message> = messages.to_vec();
        let tools_owned: Vec<crate::llm::tool::ToolDefinition> = tools.to_vec();
        let http = self.http.clone();

        let start = async move {
            let input: Vec<InputItem> = messages_owned
                .iter()
                .map(|m| InputItem {
                    kind: "message",
                    role: match m.role {
                        Role::System => "developer",
                        Role::User | Role::Tool => "user",
                        Role::Assistant => "assistant",
                    },
                    content: m.content.clone(),
                })
                .collect();
            let tools_api: Vec<ResponsesTool> = tools_owned
                .iter()
                .map(|t| ResponsesTool { kind: "function", name: &t.function.name, description: &t.function.description, parameters: t.function.parameters.clone() })
                .collect();
            let req = ResponsesRequest { model: &model, input, stream: true, store: false, tools: tools_api };
            let mut builder = http
                .post(url)
                .header("authorization", format!("Bearer {bearer}"))
                .header("content-type", "application/json")
                .header("originator", ORIGINATOR);
            if let Some(account) = account_id {
                builder = builder.header("chatgpt-account-id", account.as_ref());
            }
            builder.json(&req).send().await
        };

        Box::pin(futures::stream::once(start).flat_map(|result| match result {
            Ok(resp) if resp.status().is_success() => stream_sse(resp),
            Ok(resp) => futures::stream::once(async move {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                Delta::Error(format!("openai HTTP {status}: {}", truncate(&body, 300)))
            })
            .boxed(),
            Err(e) => futures::stream::once(async move { Delta::Error(format!("openai request failed: {e}")) }).boxed(),
        }))
    }
}

fn stream_sse(resp: reqwest::Response) -> Pin<Box<dyn futures::Stream<Item = Delta> + Send>> {
    let mut parser = SseParser::new();
    let stream = resp.bytes_stream();
    let stream = stream.flat_map(move |chunk| {
        let deltas: Vec<Delta> = match chunk {
            Ok(bytes) => parser.feed(&bytes).into_iter().filter_map(delta_of).collect(),
            Err(e) => vec![Delta::Error(format!("sse read: {e}"))],
        };
        futures::stream::iter(deltas)
    });
    Box::pin(stream.chain(futures::stream::once(async { Delta::Done })))
}

fn delta_of(frame: SseFrame) -> Option<Delta> {
    let SseFrame::Data(data) = frame else { return None };
    let event: ResponsesEvent = serde_json::from_str(&data).ok()?;
    match event.kind.as_str() {
        "response.output_text.delta" => event.delta.map(Delta::Text),
        "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => event.delta.map(Delta::Reasoning),
        "response.completed" => event.response.and_then(|r| r.usage).map(|u| Delta::Usage {
            input: u.input_tokens.unwrap_or(0),
            output: u.output_tokens.unwrap_or(0),
        }),
        _ => None,
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..s.floor_char_boundary(max)] }
}
