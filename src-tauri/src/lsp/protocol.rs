//! LSP 标准 framing：`Content-Length: N\r\n\r\n<body>`（与 MCP 的行分隔不同，必须按头解析）。

/// 编码一帧。
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
        loop {
            let Some(header_end) = find_subslice(&self.buf, b"\r\n\r\n") else { break };
            let header = String::from_utf8_lossy(&self.buf[..header_end]).to_string();
            let Some(len) =
                header.split("\r\n").find_map(|line| line.strip_prefix("Content-Length: ").and_then(|v| v.trim().parse::<usize>().ok()))
            else {
                // 无长度头的帧无法恢复同步，丢弃到分隔符为止
                self.buf.drain(..header_end + 4);
                continue;
            };
            let body_start = header_end + 4;
            if self.buf.len() < body_start + len {
                break;
            }
            out.push(String::from_utf8_lossy(&self.buf[body_start..body_start + len]).to_string());
            self.buf.drain(..body_start + len);
        }
        out
    }
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
}
