//! loop 事件与运行结果类型。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    Text { text: String },
    Reasoning { text: String },
    ToolCall { name: String, summary: String },
    ToolResult { name: String, summary: String },
    Phase { name: String },
    Done { turns: u32, stats: Option<RunStats> },
    Aborted,
    Error { message: String },
}

/// 单轮运行统计（TTFT / 耗时 / tok/s / tokens）。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RunStats {
    pub ttft_ms: u64,
    pub duration_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tokens_per_sec: u64,
}

#[derive(Debug)]
pub struct AgentOutcome {
    pub final_text: String,
    pub turns: u32,
    pub aborted: bool,
    pub stats: Option<RunStats>,
}
