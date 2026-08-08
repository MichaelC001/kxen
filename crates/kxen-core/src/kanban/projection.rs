//! 投影：BoardState 由事件流确定性重建。reduce 是纯函数——不 I/O、不读时钟、不接收 LLM 输出；
//! 所有时间戳与 id 都来自事件本身，同一事件序列重放任意次结果逐字节一致。
//! 集合一律用 BTreeMap：HashMap 迭代序不确定，序列化字节序会破坏可回放断言。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::error::KanbanError;
use super::events::{EventKind, KanbanEvent, Outcome};
use super::model::{AgentDef, CardComment, CardState, CardStatus, ColumnDef, OnEnterKind, PolicySpec, RunState};

/// 生效中的自主授权：spec 来自 policy_set 事件，used 由 auto_approved 事件计数推导。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivePolicy {
    pub spec: PolicySpec,
    #[serde(default)]
    pub used: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardState {
    pub board_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub columns: Vec<ColumnDef>,
    #[serde(default)]
    pub cards: BTreeMap<String, CardState>,
    #[serde(default)]
    pub runs: BTreeMap<String, RunState>,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<ActivePolicy>,
    #[serde(default)]
    pub seq: u64,
}

impl BoardState {
    pub fn new(board_id: &str) -> Self {
        Self {
            board_id: board_id.into(),
            title: None,
            columns: Vec::new(),
            cards: BTreeMap::new(),
            runs: BTreeMap::new(),
            agents: BTreeMap::new(),
            policy: None,
            seq: 0,
        }
    }

    pub fn created(&self) -> bool {
        self.title.is_some()
    }

    pub fn column(&self, column_id: &str) -> Option<&ColumnDef> {
        self.columns.iter().find(|c| c.id == column_id)
    }
}

pub fn replay(board_id: &str, events: &[KanbanEvent]) -> Result<BoardState, KanbanError> {
    let mut state = BoardState::new(board_id);
    for event in events {
        reduce(&mut state, event)?;
    }
    Ok(state)
}

pub fn reduce(state: &mut BoardState, event: &KanbanEvent) -> Result<(), KanbanError> {
    let invariant = |message: String| KanbanError::Projection(format!("event {} seq {}: {message}", event.id, event.seq));
    match &event.kind {
        EventKind::BoardCreate(payload) => {
            if state.created() {
                return Err(invariant("duplicate board_create".into()));
            }
            state.title = Some(payload.title.clone());
            state.columns = payload.columns.clone();
        }
        EventKind::ColumnAdd(payload) => {
            if state.column(&payload.column.id).is_some() {
                return Err(invariant(format!("duplicate column {}", payload.column.id)));
            }
            state.columns.push(payload.column.clone());
        }
        EventKind::CardCreate(payload) => {
            let column = state.column(&payload.column_id).ok_or_else(|| invariant(format!("unknown column {}", payload.column_id)))?;
            if state.cards.contains_key(&payload.card_id) {
                return Err(invariant(format!("duplicate card {}", payload.card_id)));
            }
            state.cards.insert(
                payload.card_id.clone(),
                CardState {
                    id: payload.card_id.clone(),
                    column_id: payload.column_id.clone(),
                    title: payload.title.clone(),
                    body: payload.body.clone(),
                    status: status_on_enter(column.on_enter.kind),
                    created_at: event.created_at,
                    updated_at: event.created_at,
                    current_run: None,
                    block_reason: None,
                    comments: Vec::new(),
                },
            );
        }
        EventKind::CardMove(payload) => {
            move_card(state, &payload.card_id, &payload.from, &payload.to, event.created_at).map_err(invariant)?;
        }
        EventKind::CardComment(payload) => {
            let card = state.cards.get_mut(&payload.card_id).ok_or_else(|| invariant(format!("unknown card {}", payload.card_id)))?;
            card.comments.push(CardComment { author: payload.author.clone(), body: payload.body.clone(), at: event.created_at });
            card.updated_at = event.created_at;
        }
        EventKind::RunStarted(payload) => {
            let card = state.cards.get_mut(&payload.card_id).ok_or_else(|| invariant(format!("unknown card {}", payload.card_id)))?;
            if state.runs.contains_key(&payload.run_id) {
                return Err(invariant(format!("duplicate run {}", payload.run_id)));
            }
            card.status = CardStatus::Running;
            card.current_run = Some(payload.run_id.clone());
            card.block_reason = None;
            card.updated_at = event.created_at;
            state.runs.insert(
                payload.run_id.clone(),
                RunState {
                    id: payload.run_id.clone(),
                    card_id: payload.card_id.clone(),
                    column_id: payload.column_id.clone(),
                    attempt: payload.attempt,
                    started_at: event.created_at,
                    ended_at: None,
                    outcome: None,
                },
            );
        }
        EventKind::RunFinished(payload) => {
            let run = state.runs.get_mut(&payload.run_id).ok_or_else(|| invariant(format!("unknown run {}", payload.run_id)))?;
            if run.outcome.is_some() {
                return Err(invariant(format!("run {} already closed", payload.run_id)));
            }
            run.outcome = Some(payload.outcome);
            run.ended_at = Some(event.created_at);
            let (card_id, column_id) = (run.card_id.clone(), run.column_id.clone());
            let column = state.column(&column_id).ok_or_else(|| invariant(format!("unknown column {column_id}")))?;
            // 迁移目标由列 transitions 推导（与守卫同一收口）；无出边 = 停车 blocked，不猜去向
            match column.transitions.target(payload.outcome).map(str::to_string) {
                Some(to) => move_card(state, &card_id, &column_id, &to, event.created_at).map_err(invariant)?,
                None => {
                    block_card(state, &card_id, event.created_at, format!("no {:?} transition from column {column_id}", payload.outcome))
                        .map_err(invariant)?
                }
            }
        }
        EventKind::RunTimeout(payload) => {
            let run = state.runs.get_mut(&payload.run_id).ok_or_else(|| invariant(format!("unknown run {}", payload.run_id)))?;
            if run.outcome.is_some() {
                return Err(invariant(format!("run {} already closed", payload.run_id)));
            }
            run.outcome = Some(Outcome::Timeout);
            run.ended_at = Some(event.created_at);
            let card_id = run.card_id.clone();
            block_card(state, &card_id, event.created_at, format!("run {} timeout", payload.run_id)).map_err(invariant)?;
        }
        EventKind::AgentDefined(payload) => {
            state.agents.insert(
                payload.name.clone(),
                AgentDef {
                    name: payload.name.clone(),
                    role: payload.role.clone(),
                    model: payload.model.clone(),
                    permission_profile: payload.permission_profile.clone(),
                    defined_at: event.created_at,
                },
            );
        }
        EventKind::PolicySet(payload) => {
            // 重设即重置计数（显式续期语义）：授权内容本身即新事实，不做差异合并
            state.policy = Some(ActivePolicy { spec: payload.policy.clone(), used: 0 });
        }
        EventKind::AutoApproved(_) => {
            // 无授权却出现放行事件 = 事件流自相矛盾（绕过守卫写入），fail-closed 不猜
            let policy = state.policy.as_mut().ok_or_else(|| invariant("auto_approved without active policy".into()))?;
            // 超放同语义：command 守卫是第一道，reduce 是最后一道——锁外写入者绕过 command 时，
            // 事件流重放也必须能发现 used 已达 max_uses 的放行事件
            if let Some(max) = policy.spec.max_uses
                && policy.used >= max
            {
                return Err(invariant(format!("auto_approved exceeds max_uses ({}/{max})", policy.used)));
            }
            policy.used += 1;
        }
    }
    state.seq = event.seq;
    Ok(())
}

fn status_on_enter(kind: OnEnterKind) -> CardStatus {
    match kind {
        OnEnterKind::HumanGate => CardStatus::WaitingHuman,
        OnEnterKind::None | OnEnterKind::AgentRun | OnEnterKind::Workflow => CardStatus::Ready,
    }
}

fn move_card(state: &mut BoardState, card_id: &str, from: &str, to: &str, at: u64) -> Result<(), String> {
    let column = state.column(to).ok_or_else(|| format!("unknown column {to}"))?;
    let status = status_on_enter(column.on_enter.kind);
    let card = state.cards.get_mut(card_id).ok_or_else(|| format!("unknown card {card_id}"))?;
    if card.column_id != from {
        return Err(format!("card {card_id} is in column {} not {from}", card.column_id));
    }
    card.column_id = to.to_string();
    card.status = status;
    card.current_run = None;
    card.block_reason = None;
    card.updated_at = at;
    Ok(())
}

fn block_card(state: &mut BoardState, card_id: &str, at: u64, reason: String) -> Result<(), String> {
    let card = state.cards.get_mut(card_id).ok_or_else(|| format!("unknown card {card_id}"))?;
    card.status = CardStatus::Blocked;
    card.current_run = None;
    card.block_reason = Some(reason);
    card.updated_at = at;
    Ok(())
}

#[cfg(test)]
mod tests;
