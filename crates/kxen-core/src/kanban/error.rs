//! Kanban 结构化错误：守卫拒绝（可恢复、调用方可读）与存储/投影失败（数据面）分开命名。

use super::events::Outcome;

#[derive(thiserror::Error, Debug)]
pub enum KanbanError {
    #[error("invalid id: {0}")]
    InvalidId(String),
    #[error("board already created: {0}")]
    BoardExists(String),
    #[error("board not created: {0}")]
    BoardNotCreated(String),
    #[error("column not found: {0}")]
    ColumnNotFound(String),
    #[error("column already exists: {0}")]
    ColumnExists(String),
    #[error("card not found: {0}")]
    CardNotFound(String),
    #[error("invalid transition: card {card_id} in column {from} has no {outcome:?} transition")]
    NoTransition { card_id: String, from: String, outcome: Outcome },
    #[error("wip limit exceeded: column {column} limit {limit}")]
    WipLimit { column: String, limit: u32 },
    #[error("card has a run in progress: {0}")]
    RunInProgress(String),
    #[error("run not found or already closed: {0}")]
    RunNotOpen(String),
    #[error("invalid column definition: {0}")]
    InvalidColumn(String),
    #[error("invalid command: {0}")]
    InvalidCommand(String),
    #[error("event log error: {0}")]
    Log(String),
    /// 投影重放发现事件流自相矛盾：日志已被篡改或写入路径绕过守卫，fail-closed 不继续。
    #[error("projection error: {0}")]
    Projection(String),
}
