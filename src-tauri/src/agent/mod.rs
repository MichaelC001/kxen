//! kxen-agent：agent loop（LLM -> tool_call -> 工具执行 -> 回传 -> 循环）、subagent、workflow（后续里程碑）。

pub mod activity;
pub mod agent_loop;
pub mod approval;
pub mod cancel;
pub mod commands;
pub mod compact;
pub mod context;
pub mod loop_detect;
pub mod prompt;
pub mod skills;
pub mod subagent;
pub mod team;
pub mod tools_spec;
pub mod workflow;
pub mod workflow_journal;

pub use agent_loop::{run_turn, AgentEvent, AgentOutcome};
