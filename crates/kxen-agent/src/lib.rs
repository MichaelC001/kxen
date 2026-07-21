//! kxen-agent：agent loop（LLM -> tool_call -> 工具执行 -> 回传 -> 循环）、subagent、workflow（后续里程碑）。

pub mod agent_loop;
pub mod loop_detect;
pub mod prompt;
pub mod subagent;
pub mod tools_spec;
pub mod workflow;

pub use agent_loop::{run_turn, AgentEvent, AgentOutcome};
