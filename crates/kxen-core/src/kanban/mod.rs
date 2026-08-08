//! Kanban Agent 核心：append-only 事件流 + 确定性投影 + 命令守卫（P1）+
//! DCP Agent 定义、列执行 driver、触发器 runner 与完成协议（P2a）+ 看板级自主授权（P3）+ 每卡 worktree 隔离（P4）+
//! 摘要 digest（P5，overview 卡片与 boards RPC 共用口径；RPC handler 在 crate 根 kanban_rpc.rs）。
//! BoardState 的唯一真源是 `<workspace>/.kxen/kanban/<board_id>/events.jsonl`，快照只是可删缓存。

mod agents;
mod command;
mod context;
mod digest;
pub(crate) mod driver;
#[cfg(test)]
mod e2e_tests;
mod error;
mod events;
mod land;
mod model;
mod policy;
mod projection;
mod render;
mod runner;
mod store;
mod worktree;

pub use agents::{
    AgentDefinition, agents_dir, load as load_agent_definition, parse as parse_agent_definition, resolve_allowed_tools,
    save as save_agent_definition, to_markdown as agent_definition_to_markdown, validate_custom_tools,
};
pub use command::{Board, board_lock};
pub use digest::{KanbanDigest, collect};
pub use driver::{DEFAULT_RUN_TIMEOUT_MS, DriverDeps, ExecuteFailure, LandingKind, RunLanding, execute, parse_verdict, turns_path};
pub use error::KanbanError;
pub use events::{
    AgentDefinedPayload, AutoApprovedPayload, BoardCreatePayload, CardCommentPayload, CardCreatePayload, CardMovePayload, ColumnAddPayload,
    EventKind, KanbanCommand, KanbanEvent, Outcome, PolicySetPayload, RunFinishedPayload, RunStartedPayload, RunTimeoutPayload,
};
pub use model::{
    AgentDef, CardComment, CardState, CardStatus, ColumnDef, OnEnter, OnEnterKind, PolicySpec, RunState, Transitions, default_template,
    validate_columns,
};
pub(crate) use policy::BoardAutoApprove;
pub use projection::{ActivePolicy, BoardState, reduce, replay};
pub use render::render_card_context;
pub use runner::{Runner, tick};
pub use store::{append_event, board_dir, events_path, load_events, load_state, save_snapshot, snapshot_path};
