//! 工具定义与 tool_call 累积（OpenAI 兼容 tool calling 的分片还原）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: FunctionSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl ToolDefinition {
    pub fn function(name: impl Into<String>, description: impl Into<String>, parameters: serde_json::Value) -> Self {
        Self { kind: "function", function: FunctionSpec { name: name.into(), description: description.into(), parameters } }
    }
}

/// 完整的一次工具调用（分片累积后的成品）。
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// 流式 tool_calls 分片的累积器（按 index 归并 id/name/arguments 片段）。
#[derive(Default)]
pub struct ToolCallAccumulator {
    slots: Vec<Partial>,
}

#[derive(Default)]
struct Partial {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkToolCall {
    pub index: Option<usize>,
    pub id: Option<String>,
    pub function: Option<ChunkFunction>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

impl ToolCallAccumulator {
    pub fn push(&mut self, chunks: &[ChunkToolCall]) {
        for chunk in chunks {
            let idx = chunk.index.unwrap_or(0);
            if self.slots.len() <= idx {
                self.slots.resize_with(idx + 1, Partial::default);
            }
            let slot = &mut self.slots[idx];
            if let Some(id) = &chunk.id {
                slot.id = Some(id.clone());
            }
            if let Some(f) = &chunk.function {
                if let Some(name) = &f.name {
                    slot.name = Some(name.clone());
                }
                if let Some(args) = &f.arguments {
                    slot.arguments.push_str(args);
                }
            }
        }
    }

    pub fn take(&mut self) -> Vec<ToolCall> {
        std::mem::take(&mut self.slots)
            .into_iter()
            .filter_map(|p| {
                Some(ToolCall { id: p.id?, name: p.name?, arguments: if p.arguments.is_empty() { "{}".into() } else { p.arguments } })
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_fragments() {
        let mut acc = ToolCallAccumulator::default();
        acc.push(&[ChunkToolCall {
            index: Some(0),
            id: Some("call_1".into()),
            function: Some(ChunkFunction { name: Some("exec".into()), arguments: None }),
        }]);
        acc.push(&[ChunkToolCall {
            index: Some(0),
            id: None,
            function: Some(ChunkFunction { name: None, arguments: Some("{\"command\":\"ls\"".into()) }),
        }]);
        acc.push(&[ChunkToolCall { index: Some(0), id: None, function: Some(ChunkFunction { name: None, arguments: Some("}".into()) }) }]);
        let calls = acc.take();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "exec");
        assert_eq!(calls[0].arguments, "{\"command\":\"ls\"}");
    }
}
