//! 自写 SSE 解析（~120 行，pi_agent_rust 模式）。
//! 输入：任意字节流的增量；输出：完整的 SSE data 载荷帧。
//! 处理：行缓冲、跨块行拼接、`data:` 前缀、心跳注释（`:` 开头）、`[DONE]`。

#[derive(Debug, Default)]
pub struct SseParser {
    /// 未完成的行残片（跨 chunk）
    pending: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseFrame {
    /// `data: ...` 载荷（不含前缀）
    Data(String),
    /// `data: [DONE]`
    Done,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂入一块字节流，返回本块解析出的完整帧。
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseFrame> {
        let text = String::from_utf8_lossy(chunk);
        self.pending.push_str(&text);

        let mut frames = Vec::new();
        // 逐行取出完整行；最后一段残片留在 pending
        while let Some(pos) = self.pending.find('\n') {
            let line = self.pending[..pos].trim_end_matches(['\r', '\n']).to_string();
            self.pending.drain(..=pos);
            if let Some(frame) = parse_line(&line) {
                frames.push(frame);
            }
        }
        frames
    }

    /// 流结束时冲刷（残片若构成完整 data 行也产出）。
    pub fn finish(&mut self) -> Vec<SseFrame> {
        let line = std::mem::take(&mut self.pending);
        let line = line.trim_end_matches(['\r', '\n']).to_string();
        parse_line(&line).into_iter().collect()
    }
}

fn parse_line(line: &str) -> Option<SseFrame> {
    if line.is_empty() || line.starts_with(':') {
        return None; // 空行分隔 / 心跳注释
    }
    let data = line.strip_prefix("data:")?.trim_start();
    if data == "[DONE]" {
        Some(SseFrame::Done)
    } else {
        Some(SseFrame::Data(data.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frames_across_chunks() {
        let mut p = SseParser::new();
        let mut frames = p.feed(b"data: {\"a\":1}\n\nda");
        frames.extend(p.feed(b"ta: {\"b\":2}\n"));
        frames.extend(p.feed(b"data: [DONE]\n"));
        let datas: Vec<_> = frames.iter().filter_map(|f| match f {
            SseFrame::Data(d) => Some(d.as_str()),
            _ => None,
        }).collect();
        assert_eq!(datas, vec!["{\"a\":1}", "{\"b\":2}"]);
        assert!(frames.iter().any(|f| matches!(f, SseFrame::Done)));
    }

    #[test]
    fn skips_heartbeat_and_empty() {
        let mut p = SseParser::new();
        let frames = p.feed(b": ping\n\n\ndata: x\n");
        assert_eq!(frames, vec![SseFrame::Data("x".into())]);
    }
}
