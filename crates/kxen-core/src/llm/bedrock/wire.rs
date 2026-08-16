//! 请求构造：kxen Message/ToolDefinition -> Bedrock Converse wire。
//! 契约对照 AWS 文档「Converse API」：messages 只认 user/assistant 且必须交替（相邻同 role 合并）、
//! system 独立顶层数组、tool 定义进 toolConfig.tools[].toolSpec、assistant tool_calls 转 toolUse 块。

use crate::llm::tool::ToolDefinition;
use crate::llm::types::{Message, Role};
use serde_json::{Value, json};

const MAX_TOKENS: u32 = 8192;

pub(super) fn build_request(messages: &[Message], tools: &[ToolDefinition]) -> Value {
    let mut request = json!({
        "messages": messages_of(messages),
        "inferenceConfig": { "maxTokens": MAX_TOKENS },
    });
    let system: Vec<Value> =
        messages.iter().filter(|m| m.role == Role::System && !m.content.is_empty()).map(|m| json!({ "text": m.content })).collect();
    if !system.is_empty() {
        request["system"] = json!(system);
    }
    if !tools.is_empty() {
        request["toolConfig"] = json!({ "tools": tools.iter().map(tool_spec).collect::<Vec<_>>() });
    }
    request
}

fn text_block(text: &str) -> Value {
    json!({ "text": text })
}

/// 消息 -> content 块数组；空块数组的消息被丢弃（Converse 拒绝空 content）。
fn blocks_of(m: &Message) -> (&'static str, Vec<Value>) {
    match m.role {
        Role::System => unreachable!("system filtered by caller"),
        Role::User => {
            let mut blocks: Vec<Value> = m
                .images
                .iter()
                .map(|img| {
                    json!({ "image": { "format": img.media_type.rsplit('/').next().unwrap_or("png"), "source": { "bytes": img.data } } })
                })
                .collect();
            if !m.content.is_empty() {
                blocks.push(text_block(&m.content));
            }
            ("user", blocks)
        }
        Role::Assistant => {
            let mut blocks: Vec<Value> = Vec::new();
            if !m.content.is_empty() {
                blocks.push(text_block(&m.content));
            }
            for call in &m.tool_calls {
                blocks.push(json!({
                    "toolUse": {
                        "toolUseId": call.id,
                        "name": call.function.name,
                        "input": serde_json::from_str::<Value>(&call.function.arguments).unwrap_or_else(|_| json!({})),
                    }
                }));
            }
            ("assistant", blocks)
        }
        Role::Tool => (
            "user",
            vec![json!({
                "toolResult": {
                    "toolUseId": m.tool_call_id.clone().unwrap_or_default(),
                    "content": [{ "text": m.content }],
                    "status": "success",
                }
            })],
        ),
    }
}

/// Converse 要求 user/assistant 交替且首条为 user：相邻同 role 合并块；开头 assistant 前补占位 user。
fn messages_of(messages: &[Message]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for m in messages {
        if m.role == Role::System {
            continue;
        }
        let (role, blocks) = blocks_of(m);
        if blocks.is_empty() {
            continue;
        }
        if out.last().and_then(|last| last.get("role")).and_then(Value::as_str) == Some(role) {
            if let Some(existing) = out.last_mut().and_then(|last| last.get_mut("content")).and_then(Value::as_array_mut) {
                existing.extend(blocks);
            }
        } else {
            out.push(json!({ "role": role, "content": blocks }));
        }
    }
    if out.first().and_then(|first| first.get("role")).and_then(Value::as_str) != Some("user") {
        out.insert(0, json!({ "role": "user", "content": [text_block("continue")] }));
    }
    out
}

fn tool_spec(tool: &ToolDefinition) -> Value {
    let schema = if tool.function.parameters.get("type").and_then(Value::as_str) == Some("object") {
        std::borrow::Cow::Borrowed(&tool.function.parameters)
    } else {
        let mut schema = if tool.function.parameters.is_object() { tool.function.parameters.clone() } else { json!({}) };
        schema["type"] = json!("object");
        std::borrow::Cow::Owned(schema)
    };
    json!({
        "toolSpec": {
            "name": tool.function.name,
            "description": if tool.function.description.is_empty() {
                std::borrow::Cow::Owned(format!("Tool: {}", tool.function.name))
            } else {
                std::borrow::Cow::Borrowed(tool.function.description.as_str())
            },
            "inputSchema": { "json": schema },
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::{AssistantToolCall, ImagePart};

    #[test]
    fn system_goes_top_level_and_messages_start_with_user() {
        let request = build_request(&[Message::system("你是助手"), Message::user("hi")], &[]);
        assert_eq!(request["system"][0]["text"], "你是助手");
        let messages = request["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(request["inferenceConfig"]["maxTokens"], 8192);
        // 开头 assistant 前必须补占位 user
        let request = build_request(&[Message::assistant("先说话")], &[]);
        let messages = request["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[1]["role"], "assistant");
    }

    #[test]
    fn tool_calls_and_results_keep_ids_and_merge_adjacent_roles() {
        let tool = ToolDefinition::function("exec", "运行命令", json!({ "type": "object", "properties": {} }));
        let messages = vec![
            Message::user("跑一下"),
            Message::assistant_with_tools("好", vec![AssistantToolCall::function("c1", "exec", "{\"command\":\"ls\"}")]),
            Message::tool_result("c1", "exec", "file.rs"),
            Message::user("继续"),
        ];
        let request = build_request(&messages, std::slice::from_ref(&tool));
        let wire = request["messages"].as_array().unwrap();
        assert_eq!(wire.len(), 3, "tool result 与后续 user 合并: {wire:?}");
        let tool_use = &wire[1]["content"][1]["toolUse"];
        assert_eq!(tool_use["toolUseId"], "c1");
        assert_eq!(tool_use["name"], "exec");
        assert_eq!(tool_use["input"]["command"], "ls");
        let merged = wire[2]["content"].as_array().unwrap();
        assert_eq!(merged[0]["toolResult"]["toolUseId"], "c1");
        assert_eq!(merged[0]["toolResult"]["content"][0]["text"], "file.rs");
        assert_eq!(merged[1]["text"], "继续");
        assert_eq!(request["toolConfig"]["tools"][0]["toolSpec"]["name"], "exec");
        assert_eq!(request["toolConfig"]["tools"][0]["toolSpec"]["inputSchema"]["json"]["type"], "object");
    }

    #[test]
    fn images_become_image_blocks_with_format_and_bytes() {
        let request = build_request(
            &[Message::user_with_images("看图", vec![ImagePart { media_type: "image/jpeg".into(), data: "QUJD".into() }])],
            &[],
        );
        let image = &request["messages"][0]["content"][0]["image"];
        assert_eq!(image["format"], "jpeg");
        assert_eq!(image["source"]["bytes"], "QUJD");
        assert_eq!(request["messages"][0]["content"][1]["text"], "看图");
    }
}
