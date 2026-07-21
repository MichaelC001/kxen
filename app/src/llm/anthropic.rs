//! Anthropic provider（Claude Pro/Max 订阅，OAuth contract 五要素，jcode 实证）。

use crate::llm::sse::{SseFrame, SseParser};
use crate::llm::types::{Delta, Message, Role};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

const API_URL: &str = "https://api.anthropic.com/v1/messages?beta=true";
const USER_AGENT: &str = "claude-cli/1.0.0";
const OAUTH_BETA: &str = "oauth-2025-04-20,claude-code-20250219";
const IDENTITY_LINE: &str = "You are Claude Code, Anthropic's official CLI for Claude.";

/// 内置工具名 allow-list 重映射（Claude OAuth 契约）。
pub fn remap_tool_name(name: &str) -> &str {
    match name {
        "exec" => "Bash",
        "read" => "Read",
        "write" => "Write",
        "edit" => "Edit",
        "glob" => "Glob",
        "grep" => "Grep",
        "subagent" => "Agent",
        "schedule" => "ScheduleWakeup",
        "skill_manage" => "Skill",
        other => other,
    }
}

pub struct AnthropicProvider {
    http: reqwest::Client,
    bearer: crate::core::shared::SharedStr,
}

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: Vec<SystemBlock<'a>>,
    messages: Vec<ApiMessage<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ApiTool<'a>>>,
}

#[derive(Serialize)]
struct SystemBlock<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'a str,
}

#[derive(Serialize)]
struct ApiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ApiTool<'a> {
    name: &'a str,
    description: &'a str,
    input_schema: serde_json::Value,
}

#[derive(Deserialize)]
struct SseEvent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    delta: Option<EventDelta>,
    #[serde(default)]
    usage: Option<EventUsage>,
    #[serde(default)]
    message: Option<UsageMessage>,
}

#[derive(Deserialize)]
struct EventDelta {
    #[serde(rename = "type")]
    kind: Option<String>,
    text: Option<String>,
}

#[derive(Deserialize)]
struct EventUsage {
    output_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct UsageMessage {
    usage: Option<EventUsage>,
}

impl AnthropicProvider {
    pub fn new(bearer: impl Into<String>) -> Self {
        Self { http: crate::llm::client::shared_http(), bearer: crate::core::shared::SharedStr::from(bearer.into()) }
    }

    pub fn stream_chat(&self, model: &str, messages: &[Message], tools: &[crate::llm::tool::ToolDefinition]) -> Pin<Box<dyn futures::Stream<Item = Delta> + Send>> {
        let bearer = self.bearer.clone();
        let model = model.to_string();
        let messages_owned: Vec<Message> = messages.to_vec();
        let tools_owned: Vec<crate::llm::tool::ToolDefinition> = tools.to_vec();
        let http = self.http.clone();

        let start = async move {
            // OAuth contract: 系统块第一行固定身份行，用户 system 追加在后
            let mut system: Vec<SystemBlock> = vec![SystemBlock { kind: "text", text: IDENTITY_LINE }];
            for m in &messages_owned {
                if m.role == Role::System {
                    system.push(SystemBlock { kind: "text", text: &m.content });
                }
            }
            let api_messages: Vec<ApiMessage> = messages_owned
                .iter()
                .filter(|m| m.role != Role::System)
                .map(|m| ApiMessage {
                    role: match m.role {
                        Role::User | Role::Tool => "user",
                        _ => "assistant",
                    },
                    content: &m.content,
                })
                .collect();
            let tools_api: Option<Vec<ApiTool>> = if tools_owned.is_empty() {
                None
            } else {
                Some(
                    tools_owned
                        .iter()
                        .map(|t| ApiTool {
                            name: remap_tool_name(&t.function.name),
                            description: &t.function.description,
                            input_schema: t.function.parameters.clone(),
                        })
                        .collect(),
                )
            };
            let req = MessagesRequest { model: &model, max_tokens: 8192, system, messages: api_messages, stream: true, tools: tools_api };
            http.post(API_URL)
                .header("authorization", format!("Bearer {bearer}"))
                .header("anthropic-version", "2023-06-01")
                .header("anthropic-beta", OAUTH_BETA)
                .header("user-agent", USER_AGENT)
                .header("content-type", "application/json")
                .json(&req)
                .send()
                .await
        };

        Box::pin(futures::stream::once(start).flat_map(|result| match result {
            Ok(resp) if resp.status().is_success() => stream_sse(resp),
            Ok(resp) => futures::stream::once(async move {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                Delta::Error(format!("anthropic HTTP {status}: {}", truncate(&body, 300)))
            })
            .boxed(),
            Err(e) => futures::stream::once(async move { Delta::Error(format!("anthropic request failed: {e}")) }).boxed(),
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
    let event: SseEvent = serde_json::from_str(&data).ok()?;
    match event.kind.as_str() {
        "content_block_delta" => {
            let delta = event.delta?;
            match delta.kind.as_deref() {
                Some("text_delta") => delta.text.map(Delta::Text),
                Some("thinking_delta") => delta.text.map(Delta::Reasoning),
                Some("input_json_delta") => None, // tool 输入分片（M3 后续接 tool calling）
                _ => None,
            }
        }
        "message_delta" => event.usage.and_then(|u| u.output_tokens).map(|output| Delta::Usage { input: 0, output }),
        "message_start" => event.message.and_then(|m| m.usage).and_then(|u| u.output_tokens).map(|output| Delta::Usage { input: 0, output }),
        _ => None,
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..s.floor_char_boundary(max)] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_remap() {
        assert_eq!(remap_tool_name("exec"), "Bash");
        assert_eq!(remap_tool_name("read"), "Read");
        assert_eq!(remap_tool_name("custom_tool"), "custom_tool");
    }

    #[test]
    fn parses_text_delta() {
        let frame = SseFrame::Data(r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"pong"}}"#.into());
        assert!(matches!(delta_of(frame), Some(Delta::Text(t)) if t == "pong"));
    }
}
