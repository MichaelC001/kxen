//! LSP 标准 framing：`Content-Length: N\r\n\r\n<body>`（与 MCP 的行分隔不同，必须按头解析）。

pub fn encode(body: &str) -> Vec<u8> {
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
}

/// 增量解码器：跨 read 边界的半帧、同 chunk 多帧都要扛住。
#[derive(Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    /// 喂入新字节，返回本轮补齐的所有完整帧。
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        let mut consumed = 0;
        while let Some(relative_header_end) = find_subslice(&self.buf[consumed..], b"\r\n\r\n") {
            let header_end = consumed + relative_header_end;
            let Some(len) = content_length(&self.buf[consumed..header_end]) else {
                // 无长度头的帧无法恢复同步，丢弃到分隔符为止
                consumed = header_end + 4;
                continue;
            };
            let body_start = header_end + 4;
            let Some(body_end) = body_start.checked_add(len) else {
                consumed = body_start;
                continue;
            };
            if self.buf.len() < body_end {
                break;
            }
            out.push(String::from_utf8_lossy(&self.buf[body_start..body_end]).into_owned());
            consumed = body_end;
        }
        if consumed == self.buf.len() {
            self.buf.clear();
        } else if consumed > 0 {
            self.buf.drain(..consumed);
        }
        out
    }
}

fn content_length(header: &[u8]) -> Option<usize> {
    header.split(|byte| *byte == b'\n').find_map(|line| {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let value = line.strip_prefix(b"Content-Length:")?;
        std::str::from_utf8(value).ok()?.trim().parse().ok()
    })
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_format() {
        let frame = encode("{}");
        assert_eq!(frame, b"Content-Length: 2\r\n\r\n{}");
    }

    #[test]
    fn decode_single_frame() {
        let mut d = FrameDecoder::default();
        let frames = d.feed(&encode("{\"id\":1}"));
        assert_eq!(frames, vec!["{\"id\":1}"]);
    }

    #[test]
    fn decode_split_across_feeds() {
        let mut d = FrameDecoder::default();
        let full = encode("hello world");
        assert!(d.feed(&full[..5]).is_empty());
        assert!(d.feed(&full[5..17]).is_empty());
        let frames = d.feed(&full[17..]);
        assert_eq!(frames, vec!["hello world"]);
    }

    #[test]
    fn decode_multiple_frames_one_chunk() {
        let mut d = FrameDecoder::default();
        let mut chunk = encode("a");
        chunk.extend_from_slice(&encode("bb"));
        chunk.extend_from_slice(&encode("ccc"));
        assert_eq!(d.feed(&chunk), vec!["a", "bb", "ccc"]);
    }

    #[test]
    fn decode_utf8_body_by_byte_length() {
        let body = "{\"text\":\"中文\"}";
        let mut d = FrameDecoder::default();
        let frame = encode(body);
        let half = frame.len() / 2;
        assert!(d.feed(&frame[..half]).is_empty());
        assert_eq!(d.feed(&frame[half..]), vec![body]);
    }

    #[test]
    fn malformed_header_recovers_without_shifting_each_complete_frame() {
        let mut d = FrameDecoder::default();
        let mut chunk = b"X-Test: no-length\r\n\r\n".to_vec();
        chunk.extend_from_slice(&encode("first"));
        chunk.extend_from_slice(&encode("second"));
        assert_eq!(d.feed(&chunk), vec!["first", "second"]);
    }
}
