//! xAI provider（grok-build 订阅：api.x.ai Bearer）。

use crate::sse::{SseFrame, SseParser};
use crate::types::{Delta, Message};
use crate::tool::ToolDefinition;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use kxen_core::shared::SharedStr;
use std::pin::Pin;

const API_URL: &str = "https://api.x.ai/v1/chat/completions";

pub struct XaiProvider {
    http: reqwest::Client,
    bearer: SharedStr,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a [crate::tool::ToolDefinition]>,
}

#[derive(Deserialize)]
struct ChatChunk {
    choices: Vec<ChunkChoice>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    delta: ChunkDelta,
}

#[derive(Deserialize)]
struct ChunkDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<crate::tool::ChunkToolCall>,
}

#[derive(Deserialize)]
struct Usage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

impl XaiProvider {
    pub fn new(bearer: impl Into<String>) -> Self {
        Self { http: crate::client::shared_http(), bearer: SharedStr::from(bearer.into()) }
    }

    /// 流式调用：返回 Delta 的异步流（'static，不借 provider）。
    pub fn stream_chat(&self, model: &str, messages: &[Message]) -> Pin<Box<dyn futures::Stream<Item = Delta> + Send>> {
        self.stream_chat_with_tools(model, messages, &[])
    }

    pub fn stream_chat_with_tools(&self, model: &str, messages: &[Message], tools: &[crate::tool::ToolDefinition]) -> Pin<Box<dyn futures::Stream<Item = Delta> + Send>> {
        let tools_owned: Option<Vec<ToolDefinition>> = if tools.is_empty() { None } else { Some(tools.to_vec()) };
        let bearer = self.bearer.clone();
        let model = model.to_string();
        let messages = messages.to_vec();
        let http = self.http.clone();

        let start = async move {
            let tools_opt = tools_owned.as_deref();
            http.post(API_URL).bearer_auth(bearer).json(&ChatRequest { model: &model, messages: &messages, stream: true, tools: tools_opt }).send().await
        };

        Box::pin(futures::stream::once(start).flat_map(|result| match result {
            Ok(resp) if resp.status().is_success() => stream_sse(resp),
            Ok(resp) => futures::stream::once(async move {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                Delta::Error(format!("xai HTTP {status}: {}", truncate(&body, 300)))
            })
            .boxed(),
            Err(e) => futures::stream::once(async move { Delta::Error(format!("xai request failed: {e}")) }).boxed(),
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
    match frame {
        SseFrame::Done => None,
        SseFrame::Data(data) => {
            let chunk: ChatChunk = serde_json::from_str(&data).ok()?;
            if let Some(usage) = chunk.usage {
                return Some(Delta::Usage {
                    input: usage.prompt_tokens.unwrap_or(0),
                    output: usage.completion_tokens.unwrap_or(0),
                });
            }
            let delta = chunk.choices.into_iter().next()?.delta;
            if !delta.tool_calls.is_empty() {
                return Some(Delta::ToolFragments(delta.tool_calls));
            }
            if let Some(reasoning) = delta.reasoning_content {
                return Some(Delta::Reasoning(reasoning));
            }
            delta.content.map(Delta::Text)
        }
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..s.floor_char_boundary(max)] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chat_chunk() {
        let json = r#"{"choices":[{"delta":{"content":"pong"}}]}"#;
        let frame = SseFrame::Data(json.into());
        assert!(matches!(delta_of(frame), Some(Delta::Text(t)) if t == "pong"));
    }

    #[test]
    fn parses_usage() {
        let json = r#"{"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":4}}"#;
        let frame = SseFrame::Data(json.into());
        assert!(matches!(delta_of(frame), Some(Delta::Usage { input: 10, output: 4 })));
    }
}
