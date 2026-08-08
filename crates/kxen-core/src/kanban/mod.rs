//! Kanban Agent 核心（P1）：append-only 事件流 + 确定性投影 + 命令守卫。
//! BoardState 的唯一真源是 `<workspace>/.kxen/kanban/<board_id>/events.jsonl`，快照只是可删缓存。
//! 本阶段不含 RPC、工具面、列执行器、审批授权、worktree（P2+ 挂载点）。

mod command;
mod error;
mod events;
mod model;
mod projection;
mod store;

pub use command::{Board, board_lock};
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
pub use store::{append_event, board_dir, events_path, load_events, load_state, save_snapshot, snapshot_path};
