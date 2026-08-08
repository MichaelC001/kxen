//! AWS event stream 二进制帧解析（CodeWhisperer GenerateAssistantResponse 的响应协议）。
//! 帧布局：total_len u32BE | headers_len u32BE | prelude_crc u32 | headers | payload(JSON) | message_crc u32；
//! 两个 CRC 均为标准 CRC-32（IEEE，反射多项式 0xEDB88320）。帧契约对照 9router open-sse/executors/kiro.js
//! 的 parseEventFrame 翻译，含同样的边界与 CRC 校验（损坏帧无法重同步，直接报错终止）。

use serde_json::Value;

/// 单帧协议上限（同 9router）：防损坏长度字段触发巨量分配。
const MAX_MESSAGE_BYTES: usize = 24 * 1024 * 1024;
const MAX_HEADERS_BYTES: usize = 128 * 1024;
/// total_len + headers_len + prelude_crc。
const PRELUDE_BYTES: usize = 12;
const CRC_BYTES: usize = 4;

/// 一帧解码结果：只保留投影层关心的头（:message-type/:event-type/:error-code）与 JSON payload。
#[derive(Debug)]
pub(super) struct Event {
    pub message_type: String,
    pub event_type: String,
    pub error_code: String,
    pub payload: Option<Value>,
}

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

const CRC_TABLE: [u32; 256] = crc_table();

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc = CRC_TABLE[((crc ^ u32::from(byte)) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

fn be_u32(frame: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(frame[offset..offset + 4].try_into().expect("u32 slice"))
}

/// 增量解码器：帧可跨 TCP 分片，buffer 攒够一帧才解析。
#[derive(Default)]
pub(super) struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub(super) fn feed(&mut self, chunk: &[u8]) -> Result<Vec<Event>, String> {
        if self.buffer.len() + chunk.len() > MAX_MESSAGE_BYTES {
            return Err("kiro eventstream buffered bytes exceed the protocol bound".into());
        }
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while self.buffer.len() >= PRELUDE_BYTES {
            if be_u32(&self.buffer, 8) != crc32(&self.buffer[..8]) {
                return Err("kiro eventstream prelude CRC mismatch".into());
            }
            let total = be_u32(&self.buffer, 0) as usize;
            let headers_len = be_u32(&self.buffer, 4) as usize;
            if !(PRELUDE_BYTES + CRC_BYTES..=MAX_MESSAGE_BYTES).contains(&total)
                || headers_len > MAX_HEADERS_BYTES
                || headers_len > total - PRELUDE_BYTES - CRC_BYTES
            {
                return Err("kiro eventstream frame bounds are invalid".into());
            }
            if self.buffer.len() < total {
                break;
            }
            let frame: Vec<u8> = self.buffer.drain(..total).collect();
            events.push(parse_frame(&frame)?);
        }
        Ok(events)
    }

    /// 传输 EOF 时调用：残留字节即截断帧（协议未完成，必须报错）。
    pub(super) fn finish(&mut self) -> Result<(), String> {
        if self.buffer.is_empty() { Ok(()) } else { Err("kiro eventstream ended with a truncated frame".into()) }
    }
}

fn parse_frame(frame: &[u8]) -> Result<Event, String> {
    let total = frame.len();
    let headers_len = be_u32(frame, 4) as usize;
    if be_u32(frame, total - CRC_BYTES) != crc32(&frame[..total - CRC_BYTES]) {
        return Err("kiro eventstream message CRC mismatch".into());
    }
    let mut event = Event { message_type: String::new(), event_type: String::new(), error_code: String::new(), payload: None };
    let mut cursor = Cursor { frame, offset: PRELUDE_BYTES, end: PRELUDE_BYTES + headers_len };
    while cursor.offset < cursor.end {
        let name = cursor.read_name()?;
        let value = cursor.read_value()?;
        match name.as_str() {
            ":message-type" => event.message_type = value,
            ":event-type" => event.event_type = value,
            ":error-code" => event.error_code = value,
            _ => {}
        }
    }
    let payload = &frame[cursor.end..total - CRC_BYTES];
    let text = std::str::from_utf8(payload).map_err(|_| "kiro eventstream payload is not UTF-8".to_string())?;
    if !text.trim().is_empty() {
        event.payload = Some(serde_json::from_str(text).map_err(|error| format!("kiro eventstream payload is not valid JSON: {error}"))?);
    }
    Ok(event)
}

/// 头区游标：name_len u8 + name + type u8 + value（类型 0-9，全部按 AWS 契约跳过或读取）。
struct Cursor<'a> {
    frame: &'a [u8],
    offset: usize,
    end: usize,
}

impl Cursor<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8], String> {
        if self.offset + count > self.end {
            return Err("kiro eventstream header exceeds its declared bounds".into());
        }
        let slice = &self.frame[self.offset..self.offset + count];
        self.offset += count;
        Ok(slice)
    }

    fn read_name(&mut self) -> Result<String, String> {
        let len = usize::from(self.take(1)?[0]);
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| "kiro eventstream header name is not UTF-8".into())
    }

    /// 返回字符串值（type 7）；其余类型按长度跳过并返回空串。
    fn read_value(&mut self) -> Result<String, String> {
        let kind = self.take(1)?[0];
        let fixed = match kind {
            0 | 1 => 0,          // bool true/false
            2 => 1,              // byte
            3 => 2,              // short
            4 => 4,              // integer
            5 | 8 => 8,          // long / timestamp
            9 => 16,             // uuid
            6 | 7 => usize::MAX, // bytes / string：u16 长度前缀
            other => return Err(format!("kiro eventstream header has unknown type {other}")),
        };
        if fixed != usize::MAX {
            self.take(fixed)?;
            return Ok(String::new());
        }
        let len = u16::from_be_bytes(self.take(2)?.try_into().expect("u16 slice")) as usize;
        let bytes = self.take(len)?;
        if kind == 7 {
            String::from_utf8(bytes.to_vec()).map_err(|_| "kiro eventstream header value is not UTF-8".into())
        } else {
            Ok(String::new())
        }
    }
}

#[cfg(test)]
mod tests;
