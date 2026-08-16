use super::*;
use crate::llm::types::AssistantToolCall;

/// 一次性 loopback server：记录首个请求的 head+body，回固定 SSE 流。
fn serve_once(sse_body: &'static str) -> (String, std::sync::mpsc::Receiver<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        // 一次性读满请求（body 长度由 content-length 决定，loopback 上单次 read 通常足够，做保守循环）
        loop {
            let n = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..n]);
            let text = String::from_utf8_lossy(&request);
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let lower = head.to_ascii_lowercase();
                let len: usize = lower
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:").map(str::trim))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                if body.len() >= len {
                    break;
                }
            }
        }
        let reply = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{sse_body}",
            sse_body.len()
        );
        stream.write_all(reply.as_bytes()).unwrap();
        tx.send(String::from_utf8_lossy(&request).into_owned()).unwrap();
    });
    (format!("http://{address}"), rx)
}

const PONG_SSE: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"pong\"}}\n\n",
    "event: message_delta\n",
    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
    "event: message_stop\n",
    "data: {\"type\":\"message_stop\"}\n\n",
);

#[tokio::test]
async fn api_key_direct_sends_x_api_key_without_oauth_contract() {
    let (base, rx) = serve_once(PONG_SSE);
    let tool = crate::llm::tool::ToolDefinition::function("exec", "运行命令", serde_json::json!({"type": "object"}));
    let stream = AnthropicProvider::custom(format!("{base}/v1/messages"), "sk-ant-test-key").stream_chat(
        "claude-sonnet-4-6",
        &[Message::system("系统提示"), Message::user("ping")],
        std::slice::from_ref(&tool),
    );
    let deltas: Vec<crate::llm::types::Delta> = futures::StreamExt::collect(stream).await;
    assert!(deltas.iter().any(|d| matches!(d, crate::llm::types::Delta::Text(t) if t == "pong")), "{deltas:?}");
    assert!(deltas.iter().any(|d| matches!(d, crate::llm::types::Delta::Done)), "{deltas:?}");

    let request = rx.recv().unwrap();
    let (head, body) = request.split_once("\r\n\r\n").unwrap();
    assert!(head.to_ascii_lowercase().contains("x-api-key: sk-ant-test-key"), "{head}");
    assert!(!head.to_ascii_lowercase().contains("authorization:"), "API key 直连不得带 bearer 头: {head}");
    assert!(!head.to_ascii_lowercase().contains("anthropic-beta"), "API key 直连不得带 OAuth beta 头: {head}");
    assert!(!head.to_ascii_lowercase().contains("claude-cli"), "API key 直连不得带 claude-cli UA: {head}");
    assert!(head.to_ascii_lowercase().contains("anthropic-version: 2023-06-01"), "{head}");
    assert!(!body.contains(IDENTITY_LINE), "API key 直连不得注入身份行: {body}");
    assert!(body.contains("系统提示"));
}

#[test]
fn tool_remap_roundtrip() {
    assert_eq!(remap_tool_name("exec"), "Bash");
    assert_eq!(unmap_tool_name("Bash"), "exec");
    assert_eq!(unmap_tool_name("custom_tool"), "custom_tool");
}

#[test]
fn system_blocks_split_at_cache_boundary() {
    let text = format!("frozen part\n\n{}\n\ndynamic part", crate::agent::prompt::CACHE_BOUNDARY);
    let blocks = system_blocks_of([text.as_str()].into_iter());
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text, "frozen part");
    assert!(blocks[0].cache_control.is_some(), "frozen 块必须打 ephemeral 断点");
    assert_eq!(blocks[1].text, "dynamic part");
    assert!(blocks[1].cache_control.is_none());
}

#[test]
fn system_blocks_without_boundary_stay_plain() {
    let blocks = system_blocks_of(["no marker here"].into_iter());
    assert_eq!(blocks.len(), 1);
    assert!(blocks[0].cache_control.is_none());
}

#[test]
fn assistant_tool_calls_become_tool_use_blocks() {
    let m = Message::assistant_with_tools("看下目录", vec![AssistantToolCall::function("toolu_1", "exec", "{\"command\":\"ls\"}")]);
    let v = assistant_content(m);
    let arr = v.as_array().unwrap();
    assert_eq!(arr[0]["type"], "text");
    assert_eq!(arr[1]["type"], "tool_use");
    assert_eq!(arr[1]["name"], "Bash");
    assert_eq!(arr[1]["input"]["command"], "ls");
}

#[test]
fn consecutive_tool_results_merge_into_one_user() {
    let msgs = vec![
        Message::assistant_with_tools(
            "",
            vec![AssistantToolCall::function("toolu_1", "exec", "{}"), AssistantToolCall::function("toolu_2", "read", "{}")],
        ),
        Message::tool_result("toolu_1", "exec", "out1"),
        Message::tool_result("toolu_2", "read", "out2"),
        Message::user("继续"),
    ];
    let api = api_messages_of(msgs);
    assert_eq!(api.len(), 3);
    assert_eq!(api[1].role, "user");
    let blocks = api[1].content.as_array().unwrap();
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0]["type"], "tool_result");
    assert_eq!(blocks[0]["tool_use_id"], "toolu_1");
    assert_eq!(blocks[1]["tool_use_id"], "toolu_2");
}
