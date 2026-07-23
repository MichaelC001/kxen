//! agent loop 单轮实现（LLM 流式 -> tool_call 累积 -> 工具执行 -> 结果回传 -> 继续）。

mod context;
mod events;
mod execute;
mod goal_tool;
mod helpers;
mod run;

pub use context::{AgentContext, SessionExtras};
pub use events::{AgentEvent, AgentOutcome, RunStats};
pub use execute::{dispatch_tool, execute_task_tool, execute_tool};
pub use goal_tool::execute_goal_tool;
pub use helpers::{first_line, parse_shell, resolve_path, result_display, result_text, summarize_args};
pub use run::run_turn;
