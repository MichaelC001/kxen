//! 投影：BoardState 由事件流确定性重建。reduce 是纯函数——不 I/O、不读时钟、不接收 LLM 输出；
//! 所有时间戳与 id 都来自事件本身，同一事件序列重放任意次结果逐字节一致。
//! 集合一律用 BTreeMap：HashMap 迭代序不确定，序列化字节序会破坏可回放断言。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::error::KanbanError;
use super::events::{EventKind, KanbanEvent, Outcome};
use super::model::{AgentDef, CardComment, CardState, CardStatus, ColumnDef, OnEnterKind, RunState};

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
mod tests {
    use super::*;
    use crate::kanban::events::*;
    use crate::kanban::model::default_template;

    fn event(seq: u64, kind: EventKind) -> KanbanEvent {
        KanbanEvent { id: format!("kev_{seq}"), board_id: "board_t".into(), seq, created_at: 1_000 + seq, kind }
    }

    /// 覆盖全部事件类型的固定序列（时间戳固定，与挂钟无关）。
    fn sample_events() -> Vec<KanbanEvent> {
        vec![
            event(1, EventKind::BoardCreate(BoardCreatePayload { title: "看板".into(), columns: default_template() })),
            event(
                2,
                EventKind::AgentDefined(AgentDefinedPayload {
                    name: "qa".into(),
                    role: "review".into(),
                    model: "auto".into(),
                    permission_profile: "readonly+test".into(),
                }),
            ),
            event(
                3,
                EventKind::CardCreate(CardCreatePayload {
                    card_id: "card_a".into(),
                    column_id: "requirements".into(),
                    title: "加登录".into(),
                    body: "支持邮箱登录".into(),
                }),
            ),
            event(
                4,
                EventKind::CardComment(CardCommentPayload {
                    card_id: "card_a".into(), author: "human".into(), body: "先做这个".into()
                }),
            ),
            event(
                5,
                EventKind::CardMove(CardMovePayload {
                    card_id: "card_a".into(),
                    from: "requirements".into(),
                    to: "implementing".into(),
                    outcome: Outcome::Success,
                }),
            ),
            event(
                6,
                EventKind::RunStarted(RunStartedPayload {
                    run_id: "board_t:card_a:implementing:1".into(),
                    card_id: "card_a".into(),
                    column_id: "implementing".into(),
                    attempt: 1,
                }),
            ),
            event(
                7,
                EventKind::RunFinished(RunFinishedPayload { run_id: "board_t:card_a:implementing:1".into(), outcome: Outcome::Success }),
            ),
            event(
                8,
                EventKind::RunStarted(RunStartedPayload {
                    run_id: "board_t:card_a:testing:1".into(),
                    card_id: "card_a".into(),
                    column_id: "testing".into(),
                    attempt: 1,
                }),
            ),
            event(9, EventKind::RunTimeout(RunTimeoutPayload { run_id: "board_t:card_a:testing:1".into() })),
            event(
                10,
                EventKind::ColumnAdd(ColumnAddPayload {
                    column: ColumnDef {
                        id: "archive".into(),
                        title: "归档".into(),
                        on_enter: Default::default(),
                        transitions: Default::default(),
                        wip_limit: Some(10),
                    },
                }),
            ),
        ]
    }

    #[test]
    fn replay_twice_is_byte_identical() {
        let events = sample_events();
        let first = serde_json::to_string(&replay("board_t", &events).unwrap()).unwrap();
        let second = serde_json::to_string(&replay("board_t", &events).unwrap()).unwrap();
        assert_eq!(first, second, "同一事件序列重放两次必须逐字节一致");
    }

    #[test]
    fn incremental_reduce_matches_full_replay() {
        let events = sample_events();
        let mut state = BoardState::new("board_t");
        for e in &events {
            reduce(&mut state, e).unwrap();
        }
        assert_eq!(serde_json::to_string(&state).unwrap(), serde_json::to_string(&replay("board_t", &events).unwrap()).unwrap());
    }

    #[test]
    fn replay_applies_semantics() {
        let state = replay("board_t", &sample_events()).unwrap();
        assert_eq!(state.seq, 10);
        assert_eq!(state.title.as_deref(), Some("看板"));
        let card = &state.cards["card_a"];
        // run_timeout 后卡片停在原列 blocked，绝不留在 running
        assert_eq!(card.column_id, "testing");
        assert_eq!(card.status, CardStatus::Blocked);
        assert!(card.block_reason.as_deref().unwrap().contains("timeout"));
        assert_eq!(card.comments.len(), 1);
        assert_eq!(state.runs["board_t:card_a:testing:1"].outcome, Some(Outcome::Timeout));
        assert!(state.agents.contains_key("qa"));
        assert!(state.column("archive").is_some());
    }

    #[test]
    fn replay_fails_closed_on_contradictory_stream() {
        // card_move 的 from 与投影不符：事件流被篡改，必须报错而非猜测
        let mut events = sample_events();
        events[4] = event(
            5,
            EventKind::CardMove(CardMovePayload {
                card_id: "card_a".into(),
                from: "done".into(),
                to: "implementing".into(),
                outcome: Outcome::Success,
            }),
        );
        assert!(matches!(replay("board_t", &events), Err(KanbanError::Projection(_))));
    }
}
