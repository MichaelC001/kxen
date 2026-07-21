//! 统一类型（零分配热路径：&str 切片与 String 复用）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<AssistantToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

impl AssistantToolCall {
    pub fn function(id: impl Into<String>, name: impl Into<String>, arguments: impl Into<String>) -> Self {
        Self { id: id.into(), kind: "function".to_string(), function: FunctionCall { name: name.into(), arguments: arguments.into() } }
    }
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into(), tool_calls: vec![], tool_call_id: None, name: None }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into(), tool_calls: vec![], tool_call_id: None, name: None }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into(), tool_calls: vec![], tool_call_id: None, name: None }
    }
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<AssistantToolCall>) -> Self {
        Self { role: Role::Assistant, content: content.into(), tool_calls, tool_call_id: None, name: None }
    }
    pub fn tool_result(id: impl Into<String>, name: impl Into<String>, content: impl Into<String>) -> Self {
        Self { role: Role::Tool, content: content.into(), tool_calls: vec![], tool_call_id: Some(id.into()), name: Some(name.into()) }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelRef {
    pub provider: String,
    pub model: String,
}

impl ModelRef {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self { provider: provider.into(), model: model.into() }
    }
}

/// 流式增量（SSE 事件的统一投影）。
#[derive(Debug, Clone)]
pub enum Delta {
    /// 正文增量
    Text(String),
    /// 推理/thinking 增量
    Reasoning(String),
    /// 工具调用分片（tool_calls chunk，由调用方累积）
    ToolFragments(Vec<crate::llm::tool::ChunkToolCall>),
    /// 工具调用（累积完成后一次性给出）
    ToolCall { name: String, input: serde_json::Value },
    /// 用量（流末尾给出，可能缺省）
    Usage { input: u64, output: u64 },
    /// 流正常结束
    Done,
    /// 错误（HTTP 非 2xx 或解析失败）
    Error(String),
}
