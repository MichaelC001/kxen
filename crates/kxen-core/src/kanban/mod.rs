//! Kanban Agent 核心：append-only 事件流 + 确定性投影 + 命令守卫（P1）+
//! DCP Agent 定义、列执行 driver、触发器 runner 与完成协议（P2a）。
//! BoardState 的唯一真源是 `<workspace>/.kxen/kanban/<board_id>/events.jsonl`，快照只是可删缓存。
//! 本阶段不含模型工具面（P2b）、审批授权（P3）、worktree（P4）、RPC/前端（P5）。

mod agents;
mod command;
mod context;
pub(crate) mod driver;
mod error;
mod events;
mod land;
mod model;
mod projection;
mod render;
mod runner;
mod store;

pub use agents::{
    AgentDefinition, agents_dir, load as load_agent_definition, parse as parse_agent_definition, profile_tools,
    save as save_agent_definition, to_markdown as agent_definition_to_markdown,
};
pub use command::{Board, board_lock};
pub use driver::{DEFAULT_RUN_TIMEOUT_MS, DriverDeps, ExecuteFailure, LandingKind, RunLanding, execute, parse_verdict, turns_path};
pub use error::KanbanError;
pub use events::{
    AgentDefinedPayload, BoardCreatePayload, CardCommentPayload, CardCreatePayload, CardMovePayload, ColumnAddPayload, EventKind,
    KanbanCommand, KanbanEvent, Outcome, RunFinishedPayload, RunStartedPayload, RunTimeoutPayload,
};
pub use model::{
    AgentDef, CardComment, CardState, CardStatus, ColumnDef, OnEnter, OnEnterKind, RunState, Transitions, default_template,
    validate_columns,
};
pub use projection::{BoardState, reduce, replay};
pub use render::render_card_context;
pub use runner::{Runner, tick};
pub use store::{append_event, board_dir, events_path, load_events, load_state, save_snapshot, snapshot_path};
