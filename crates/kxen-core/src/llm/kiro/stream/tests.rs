use super::*;
use std::io::{Read, Write};

fn event(message_type: &str, event_type: &str, payload: &str) -> Event {
    Event {
        message_type: message_type.to_string(),
        event_type: event_type.to_string(),
        error_code: String::new(),
        payload: serde_json::from_str(payload).ok(),
    }
}

fn collect(events: &[Event]) -> Vec<Delta> {
    let mut projection = Projection::default();
    let mut out = VecDeque::new();
    for event in events {
        if !projection.process(event, &mut out) {
            break;
        }
    }
    if !matches!(out.back(), Some(Delta::Error(_))) {
        projection.finish(&mut out);
    }
    out.into_iter().collect()
}

#[test]
fn assistant_response_chunks_become_text_deltas() {
    let deltas = collect(&[
        event("event", "assistantResponseEvent", r#"{"content":"你"}"#),
        event("event", "assistantResponseEvent", r#"{"content":"好"}"#),
    ]);
    assert!(matches!(&deltas[0], Delta::Text(t) if t == "你"));
    assert!(matches!(&deltas[1], Delta::Text(t) if t == "好"));
    assert!(matches!(&deltas[2], Delta::Done));
}

#[test]
fn inline_thinking_tags_split_into_reasoning_across_chunks() {
    let deltas = collect(&[
        event("event", "assistantResponseEvent", r#"{"content":"前<thinking>想一"}"#),
        event("event", "assistantResponseEvent", r#"{"content":"想二</thinking>后"}"#),
    ]);
    assert!(matches!(&deltas[0], Delta::Text(t) if t == "前"));
    assert!(matches!(&deltas[1], Delta::Reasoning(t) if t == "想一"));
    assert!(matches!(&deltas[2], Delta::Reasoning(t) if t == "想二"));
    assert!(matches!(&deltas[3], Delta::Text(t) if t == "后"));
    assert!(matches!(&deltas[4], Delta::Done));
}

#[test]
fn tool_use_string_fragments_aggregate_by_id() {
    let deltas = collect(&[
        event("event", "toolUseEvent", r#"{"toolUseId":"t1","name":"exec","input":"{\"command\":"}"#),
        event("event", "toolUseEvent", r#"{"toolUseId":"t1","name":"exec","input":"\"ls\"}"}"#),
        event("event", "messageStopEvent", r#"{"stopReason":"tool_use"}"#),
    ]);
    assert!(matches!(&deltas[0], Delta::ToolCall { name, input } if name == "exec" && input["command"] == "ls"));
    assert!(matches!(&deltas[1], Delta::Done));
}

#[test]
fn tool_use_object_input_and_array_payload() {
    let deltas = collect(&[event(
        "event",
        "toolUseEvent",
        r#"[{"toolUseId":"t1","name":"a","input":{"x":1}},{"toolUseId":"t2","name":"b","input":{"y":2}}]"#,
    )]);
    assert!(matches!(&deltas[0], Delta::ToolCall { name, input } if name == "a" && input["x"] == 1));
    assert!(matches!(&deltas[1], Delta::ToolCall { name, input } if name == "b" && input["y"] == 2));
    assert!(matches!(&deltas[2], Delta::Done));
}

#[test]
fn tool_use_invalid_json_fragments_are_an_error() {
    let deltas = collect(&[event("event", "toolUseEvent", r#"{"toolUseId":"t1","name":"exec","input":"{bad"}"#)]);
    assert!(matches!(&deltas[0], Delta::Error(e) if e.contains("not valid JSON")), "{deltas:?}");
    assert!(!deltas.iter().any(|d| matches!(d, Delta::Done)), "出错后不得再发 Done");
}

#[test]
fn exception_frame_is_terminal_error_without_done() {
    let deltas =
        collect(&[event("event", "assistantResponseEvent", r#"{"content":"前半"}"#), event("exception", "", r#"{"message":"throttled"}"#)]);
    assert!(matches!(&deltas[0], Delta::Text(t) if t == "前半"));
    assert!(matches!(&deltas[1], Delta::Error(e) if e.contains("throttled")));
    assert_eq!(deltas.len(), 2);
}

#[test]
fn empty_stream_is_an_error_not_done() {
    let deltas = collect(&[]);
    assert!(matches!(&deltas[0], Delta::Error(e) if e.contains("without model output")));
}

#[test]
fn reasoning_content_event_maps_to_reasoning() {
    let deltas = collect(&[event("event", "reasoningContentEvent", r#"{"reasoningContentEvent":{"text":"推理"}}"#)]);
    assert!(matches!(&deltas[0], Delta::Reasoning(t) if t == "推理"));
    assert!(matches!(&deltas[1], Delta::Done));
}

#[test]
fn error_frame_without_message_uses_default_wording() {
    let deltas = collect(&[event("error", "", "")]);
    assert!(matches!(&deltas[0], Delta::Error(e) if e.contains("upstream eventstream error")), "{deltas:?}");
}

#[test]
fn code_event_content_maps_to_text_and_empty_is_skipped() {
    let deltas = collect(&[event("event", "codeEvent", r#"{"content":"fn main() {}"}"#), event("event", "codeEvent", r#"{"content":""}"#)]);
    assert!(matches!(&deltas[0], Delta::Text(t) if t == "fn main() {}"));
    assert!(matches!(&deltas[1], Delta::Done), "空 codeEvent 不得产出 Delta: {deltas:?}");
    assert_eq!(deltas.len(), 2);
}

#[test]
fn error_event_is_terminal_with_payload_or_default_message() {
    let deltas = collect(&[event("event", "errorEvent", r#"{"message":"quota exhausted"}"#)]);
    assert!(matches!(&deltas[0], Delta::Error(e) if e == "kiro quota exhausted"), "{deltas:?}");
    assert_eq!(deltas.len(), 1);
    let deltas = collect(&[event("event", "errorEvent", r#"{}"#)]);
    assert!(matches!(&deltas[0], Delta::Error(e) if e.contains("upstream errorEvent")), "{deltas:?}");
}

#[test]
fn tool_use_without_name_is_terminal_error() {
    let deltas = collect(&[event("event", "toolUseEvent", r#"{"toolUseId":"t1","input":"{}"}"#)]);
    assert!(matches!(&deltas[0], Delta::Error(e) if e.contains("missing a tool name")), "{deltas:?}");
    assert_eq!(deltas.len(), 1, "终态错误后不得再发 Done");
}

#[test]
fn tool_name_change_between_fragments_is_terminal_error() {
    let deltas = collect(&[
        event("event", "toolUseEvent", r#"{"toolUseId":"t1","name":"a","input":"{"}"#),
        event("event", "toolUseEvent", r#"{"toolUseId":"t1","name":"b","input":"}"}"#),
    ]);
    assert!(matches!(&deltas[0], Delta::Error(e) if e.contains("name changed between fragments")), "{deltas:?}");
}

#[test]
fn tool_use_ignores_non_string_non_object_input_and_missing_id() {
    let deltas = collect(&[
        event("event", "toolUseEvent", r#"{"toolUseId":"t1","name":"exec","input":42}"#),
        event("event", "toolUseEvent", r#"{"name":"noop"}"#),
    ]);
    assert!(
        matches!(&deltas[0], Delta::ToolCall { name, input } if name == "exec" && input == &Value::Object(serde_json::Map::new())),
        "{deltas:?}"
    );
    assert!(
        matches!(&deltas[1], Delta::ToolCall { name, input } if name == "noop" && input == &Value::Object(serde_json::Map::new())),
        "{deltas:?}"
    );
    assert!(matches!(&deltas[2], Delta::Done));
}

#[test]
fn tool_use_event_without_payload_is_ignored() {
    let deltas = collect(&[event("event", "toolUseEvent", ""), event("event", "assistantResponseEvent", r#"{"content":"正文"}"#)]);
    assert!(matches!(&deltas[0], Delta::Text(t) if t == "正文"));
    assert!(matches!(&deltas[1], Delta::Done));
}

#[test]
fn reasoning_content_event_accepts_bare_string_and_content_key() {
    let deltas = collect(&[event("event", "reasoningContentEvent", r#""直接字符串""#)]);
    assert!(matches!(&deltas[0], Delta::Reasoning(t) if t == "直接字符串"));
    let deltas = collect(&[event("event", "reasoningContentEvent", r#"{"content":"content 键"}"#)]);
    assert!(matches!(&deltas[0], Delta::Reasoning(t) if t == "content 键"));
}

// ---- 字节流管线（stream_events）：loopback mock server 喂 eventstream 帧 ----

/// 与 eventstream/tests.rs 相同的 CRC-32（IEEE 反射多项式），测试帧构造用。
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { 0xEDB8_8320 ^ (crc >> 1) } else { crc >> 1 };
        }
    }
    crc ^ 0xFFFF_FFFF
}

fn frame(message_type: &str, event_type: &str, payload: &str) -> Vec<u8> {
    let mut headers = Vec::new();
    for (name, value) in [(":message-type", message_type), (":event-type", event_type)] {
        headers.push(name.len() as u8);
        headers.extend_from_slice(name.as_bytes());
        headers.push(7);
        headers.extend_from_slice(&(value.len() as u16).to_be_bytes());
        headers.extend_from_slice(value.as_bytes());
    }
    let total = (12 + headers.len() + payload.len() + 4) as u32;
    let mut frame = Vec::new();
    frame.extend_from_slice(&total.to_be_bytes());
    frame.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    frame.extend_from_slice(&crc32(&frame).to_be_bytes());
    frame.extend_from_slice(&headers);
    frame.extend_from_slice(payload.as_bytes());
    let crc = crc32(&frame);
    frame.extend_from_slice(&crc.to_be_bytes());
    frame
}

/// 单次应答的 loopback server：任意 GET 都返回给定 eventstream 字节。
fn serve_eventstream(body: Vec<u8>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0u8; 1024];
            let _ = stream.read(&mut buffer);
            let header = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/vnd.amazon.eventstream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let mut reply = header.into_bytes();
            reply.extend_from_slice(&body);
            if stream.write_all(&reply).is_err() {
                break;
            }
        }
    });
    format!("http://{address}")
}

async fn collect_body(body: Vec<u8>) -> Vec<Delta> {
    let url = serve_eventstream(body);
    let response = reqwest::Client::new().get(&url).send().await.expect("mock response");
    stream_events(response).collect().await
}

#[tokio::test]
async fn stream_events_projects_frames_to_deltas() {
    let mut body = frame("event", "assistantResponseEvent", r#"{"content":"答案"}"#);
    body.extend_from_slice(&frame("event", "messageStopEvent", r#"{"stopReason":"end_turn"}"#));
    let deltas = collect_body(body).await;
    assert!(matches!(&deltas[0], Delta::Text(t) if t == "答案"), "{deltas:?}");
    assert!(matches!(deltas.last(), Some(Delta::Done)), "{deltas:?}");
}

#[tokio::test]
async fn stream_events_corrupt_frame_is_terminal_error() {
    let mut body = frame("event", "assistantResponseEvent", r#"{"content":"x"}"#);
    let last = body.len() - 1;
    body[last] ^= 0xFF;
    let deltas = collect_body(body).await;
    assert_eq!(deltas.len(), 1, "{deltas:?}");
    assert!(matches!(&deltas[0], Delta::Error(e) if e.contains("message CRC")), "{deltas:?}");
}

#[tokio::test]
async fn stream_events_truncated_tail_is_an_error() {
    let bytes = frame("event", "assistantResponseEvent", r#"{"content":"x"}"#);
    let deltas = collect_body(bytes[..bytes.len() - 3].to_vec()).await;
    assert!(matches!(deltas.last(), Some(Delta::Error(e)) if e.contains("truncated")), "{deltas:?}");
}

#[tokio::test]
async fn stream_events_empty_body_is_zero_output_error() {
    let deltas = collect_body(Vec::new()).await;
    assert!(matches!(&deltas[0], Delta::Error(e) if e.contains("without model output")), "{deltas:?}");
}

#[tokio::test]
async fn stream_events_terminal_event_stops_processing_same_chunk() {
    // 同一分片解出多帧：终态错误帧之后的帧不得再投影，且不再发 Done。
    let mut body = frame("event", "errorEvent", r#"{"message":"quota exhausted"}"#);
    body.extend_from_slice(&frame("event", "assistantResponseEvent", r#"{"content":"不应出现"}"#));
    let deltas = collect_body(body).await;
    assert_eq!(deltas.len(), 1, "{deltas:?}");
    assert!(matches!(&deltas[0], Delta::Error(e) if e.contains("quota exhausted")), "{deltas:?}");
}

#[tokio::test]
async fn stream_events_transport_error_mid_stream_is_terminal_error() {
    // content-length 虚报后断连：已投影的增量保留，读取错误作为终态 Error 给出。
    let body = frame("event", "assistantResponseEvent", r#"{"content":"前半"}"#);
    let url = serve_incomplete(body, 4096);
    let response = reqwest::Client::new().get(&url).send().await.expect("mock response");
    let deltas: Vec<Delta> = stream_events(response).collect().await;
    assert!(matches!(&deltas[0], Delta::Text(t) if t == "前半"), "{deltas:?}");
    assert!(matches!(deltas.last(), Some(Delta::Error(e)) if e.contains("eventstream read")), "{deltas:?}");
}

/// 虚报 content-length 的 loopback server：写出 body 后立刻断连，制造传输层读取错误。
fn serve_incomplete(body: Vec<u8>, declared_len: usize) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buffer = [0u8; 1024];
            let _ = stream.read(&mut buffer);
            let header = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/vnd.amazon.eventstream\r\ncontent-length: {declared_len}\r\nconnection: close\r\n\r\n"
            );
            let mut reply = header.into_bytes();
            reply.extend_from_slice(&body);
            if stream.write_all(&reply).is_err() {
                break;
            }
        }
    });
    format!("http://{address}")
}
