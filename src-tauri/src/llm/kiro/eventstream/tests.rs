use super::*;

/// 测试帧构造：:message-type / :event-type 两个字符串头 + JSON payload。
fn frame(message_type: &str, event_type: &str, payload: &str) -> Vec<u8> {
    let mut headers = Vec::new();
    for (name, value) in [(":message-type", message_type), (":event-type", event_type)] {
        headers.push(name.len() as u8);
        headers.extend_from_slice(name.as_bytes());
        headers.push(7);
        headers.extend_from_slice(&(value.len() as u16).to_be_bytes());
        headers.extend_from_slice(value.as_bytes());
    }
    finish_frame(headers, payload.as_bytes())
}

/// 原始帧构造：任意头字节（含非字符串类型）+ 任意 payload 字节，用于边界与损坏输入。
fn raw_frame(headers: &[(Vec<u8>, u8, Vec<u8>)], payload: &[u8]) -> Vec<u8> {
    let mut header_bytes = Vec::new();
    for (name, kind, value) in headers {
        header_bytes.push(name.len() as u8);
        header_bytes.extend_from_slice(name);
        header_bytes.push(*kind);
        if matches!(kind, 6 | 7) {
            header_bytes.extend_from_slice(&(value.len() as u16).to_be_bytes());
        }
        header_bytes.extend_from_slice(value);
    }
    finish_frame(header_bytes, payload)
}

fn finish_frame(headers: Vec<u8>, payload: &[u8]) -> Vec<u8> {
    let total = (PRELUDE_BYTES + headers.len() + payload.len() + CRC_BYTES) as u32;
    let mut frame = Vec::new();
    frame.extend_from_slice(&total.to_be_bytes());
    frame.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    frame.extend_from_slice(&crc32(&frame).to_be_bytes());
    frame.extend_from_slice(&headers);
    frame.extend_from_slice(payload);
    let crc = crc32(&frame);
    frame.extend_from_slice(&crc.to_be_bytes());
    frame
}

fn str_header(name: &str, value: &str) -> (Vec<u8>, u8, Vec<u8>) {
    (name.as_bytes().to_vec(), 7, value.as_bytes().to_vec())
}

#[test]
fn decodes_single_frame_with_headers_and_payload() {
    let bytes = frame("event", "assistantResponseEvent", r#"{"content":"你好"}"#);
    let events = FrameDecoder::default().feed(&bytes).expect("valid frame");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].message_type, "event");
    assert_eq!(events[0].event_type, "assistantResponseEvent");
    assert_eq!(events[0].payload.as_ref().and_then(|p| p.get("content")).and_then(Value::as_str), Some("你好"));
}

#[test]
fn frame_split_across_feeds_decodes_once_complete() {
    let bytes = frame("event", "assistantResponseEvent", r#"{"content":"ab"}"#);
    let mut decoder = FrameDecoder::default();
    let mid = bytes.len() / 2;
    assert!(decoder.feed(&bytes[..mid]).expect("prefix").is_empty(), "半帧不得产出事件");
    let events = decoder.feed(&bytes[mid..]).expect("suffix");
    assert_eq!(events.len(), 1);
    assert!(decoder.finish().is_ok());
}

#[test]
fn multiple_frames_in_one_chunk_all_decode() {
    let one = frame("event", "assistantResponseEvent", r#"{"content":"a"}"#);
    let two = frame("event", "messageStopEvent", r#"{"stopReason":"end_turn"}"#);
    let mut joined = one;
    joined.extend_from_slice(&two);
    let events = FrameDecoder::default().feed(&joined).expect("two frames");
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].event_type, "messageStopEvent");
}

#[test]
fn corrupt_prelude_crc_is_rejected() {
    let mut bytes = frame("event", "x", "{}");
    bytes[0] ^= 0xFF;
    let error = FrameDecoder::default().feed(&bytes).expect_err("corrupt prelude must fail");
    assert!(error.contains("prelude CRC"), "{error}");
}

#[test]
fn corrupt_message_crc_is_rejected() {
    let mut bytes = frame("event", "x", "{}");
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    let error = FrameDecoder::default().feed(&bytes).expect_err("corrupt message crc must fail");
    assert!(error.contains("message CRC"), "{error}");
}

#[test]
fn truncated_frame_at_eof_is_an_error() {
    let bytes = frame("event", "x", "{}");
    let mut decoder = FrameDecoder::default();
    assert!(decoder.feed(&bytes[..bytes.len() - 2]).expect("prefix").is_empty());
    assert!(decoder.finish().expect_err("leftover bytes must fail").contains("truncated"));
}

#[test]
fn error_frame_surfaces_message_type_and_error_code() {
    let bytes = frame("exception", "", r#"{"message":"throttled"}"#);
    let events = FrameDecoder::default().feed(&bytes).expect("error frame");
    assert_eq!(events[0].message_type, "exception");
    assert_eq!(events[0].payload.as_ref().and_then(|p| p.get("message")).and_then(Value::as_str), Some("throttled"));
}

#[test]
fn crc32_matches_ieee_reference_vectors() {
    // CRC-32/ISO-HDLC 参考向量：运行时重算表，验证反射多项式与初值/终值异或。
    assert_eq!(crc_table()[1], 0x7707_3096);
    assert_eq!(crc32(b""), 0);
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

#[test]
fn buffered_bytes_over_protocol_bound_are_rejected() {
    let chunk = vec![0u8; MAX_MESSAGE_BYTES + 1];
    let error = FrameDecoder::default().feed(&chunk).expect_err("oversized buffer must fail");
    assert!(error.contains("exceed the protocol bound"), "{error}");
}

#[test]
fn invalid_frame_bounds_are_rejected() {
    // total_len 小于 prelude + message_crc 下限。
    let mut small = Vec::new();
    small.extend_from_slice(&8u32.to_be_bytes());
    small.extend_from_slice(&0u32.to_be_bytes());
    small.extend_from_slice(&crc32(&small).to_be_bytes());
    let error = FrameDecoder::default().feed(&small).expect_err("tiny total must fail");
    assert!(error.contains("bounds are invalid"), "{error}");
    // headers_len 超过 total_len 余量。
    let mut oversized = Vec::new();
    oversized.extend_from_slice(&64u32.to_be_bytes());
    oversized.extend_from_slice(&128u32.to_be_bytes());
    oversized.extend_from_slice(&crc32(&oversized).to_be_bytes());
    let error = FrameDecoder::default().feed(&oversized).expect_err("oversized headers must fail");
    assert!(error.contains("bounds are invalid"), "{error}");
}

#[test]
fn error_code_header_is_captured_and_unknown_headers_skipped() {
    let bytes = raw_frame(
        &[str_header(":message-type", "error"), str_header(":error-code", "AccessDenied"), str_header("x-custom", "ignored")],
        br#"{"message":"denied"}"#,
    );
    let events = FrameDecoder::default().feed(&bytes).expect("frame with error code");
    assert_eq!(events[0].message_type, "error");
    assert_eq!(events[0].error_code, "AccessDenied");
}

#[test]
fn non_string_header_value_types_are_skipped() {
    let headers = vec![
        str_header(":message-type", "event"),
        (b":flag".to_vec(), 0, vec![]),
        (b":off".to_vec(), 1, vec![]),
        (b":byte".to_vec(), 2, vec![7]),
        (b":short".to_vec(), 3, vec![0, 2]),
        (b":int".to_vec(), 4, vec![0, 0, 0, 4]),
        (b":long".to_vec(), 5, vec![0; 8]),
        (b":ts".to_vec(), 8, vec![0; 8]),
        (b":uuid".to_vec(), 9, vec![0; 16]),
        (b":blob".to_vec(), 6, b"raw-bytes".to_vec()),
        str_header(":event-type", "assistantResponseEvent"),
    ];
    let bytes = raw_frame(&headers, br#"{"content":"ok"}"#);
    let events = FrameDecoder::default().feed(&bytes).expect("typed headers must decode");
    assert_eq!(events[0].event_type, "assistantResponseEvent");
    assert_eq!(events[0].payload.as_ref().and_then(|p| p.get("content")).and_then(Value::as_str), Some("ok"));
}

#[test]
fn unknown_header_value_type_is_rejected() {
    let bytes = raw_frame(&[(b":weird".to_vec(), 42, vec![1, 2])], b"{}");
    let error = FrameDecoder::default().feed(&bytes).expect_err("unknown header type must fail");
    assert!(error.contains("unknown type 42"), "{error}");
}

#[test]
fn header_overrunning_declared_bounds_is_rejected() {
    // headers_len 声明 3 字节，但 name_len 声称 10：游标必须拒绝越界读取。
    let headers = [10u8, b'a', b'b'];
    let total = (PRELUDE_BYTES + headers.len() + CRC_BYTES) as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&total.to_be_bytes());
    bytes.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&crc32(&bytes).to_be_bytes());
    bytes.extend_from_slice(&headers);
    let crc = crc32(&bytes);
    bytes.extend_from_slice(&crc.to_be_bytes());
    let error = FrameDecoder::default().feed(&bytes).expect_err("overrunning header must fail");
    assert!(error.contains("exceeds its declared bounds"), "{error}");
}

#[test]
fn non_utf8_header_name_is_rejected() {
    let bytes = raw_frame(&[(vec![0xFF], 0, vec![])], b"{}");
    let error = FrameDecoder::default().feed(&bytes).expect_err("non-utf8 name must fail");
    assert!(error.contains("header name is not UTF-8"), "{error}");
}

#[test]
fn non_utf8_string_header_value_is_rejected() {
    let bytes = raw_frame(&[(b":message-type".to_vec(), 7, vec![0xFF])], b"{}");
    let error = FrameDecoder::default().feed(&bytes).expect_err("non-utf8 value must fail");
    assert!(error.contains("header value is not UTF-8"), "{error}");
}

#[test]
fn non_utf8_payload_is_rejected() {
    let bytes = raw_frame(&[str_header(":message-type", "event")], &[0xFF, 0xFE]);
    let error = FrameDecoder::default().feed(&bytes).expect_err("non-utf8 payload must fail");
    assert!(error.contains("payload is not UTF-8"), "{error}");
}

#[test]
fn non_json_payload_is_rejected() {
    let bytes = raw_frame(&[str_header(":message-type", "event")], b"not-json{");
    let error = FrameDecoder::default().feed(&bytes).expect_err("non-json payload must fail");
    assert!(error.contains("payload is not valid JSON"), "{error}");
}

#[test]
fn empty_payload_decodes_with_none_payload() {
    let bytes = frame("event", "messageStopEvent", "");
    let events = FrameDecoder::default().feed(&bytes).expect("empty payload frame");
    assert_eq!(events[0].event_type, "messageStopEvent");
    assert!(events[0].payload.is_none());
}
