//! 卡片上下文渲染（DCP 重建的纯函数段）：卡片事件切片 -> prompt 文本。
//! 确定性契约：不读时钟、不读文件系统、不接收投影之外的任何输入，同一事件切片渲染任意次
//! 逐字节一致——列执行 Agent 的上下文因此可回放、可审计（design.md「确定性边界」：
//! 投影与渲染在边界内，LLM 生成在边界外）。
//! 文本面向模型消费，一律英文（提示词规则，与 subagent role brief 同约定）。

use std::collections::HashMap;
use std::fmt::Write as _;

use super::events::{EventKind, KanbanEvent, Outcome};

fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Success => "success",
        Outcome::Failure => "failure",
        Outcome::Timeout => "timeout",
    }
}

/// 渲染一张卡的完整历史：身份/正文、迁移轨迹、列执行 run 史（含 outcome）、评论。
/// 卡片不存在返回 None（调用方守卫已先行校验，此处是类型层兜底而非正常路径）。
pub fn render_card_context(events: &[KanbanEvent], card_id: &str) -> Option<String> {
    let mut created: Option<(&str, &str)> = None;
    let mut moves: Vec<String> = Vec::new();
    let mut comments: Vec<String> = Vec::new();
    let mut runs: Vec<(&str, String)> = Vec::new();
    // run_id -> (outcome, ended_at)：只按 key 查询不参与迭代，HashMap 序不确定不影响输出
    let mut outcomes: HashMap<&str, (Outcome, u64)> = HashMap::new();
    for event in events {
        match &event.kind {
            EventKind::CardCreate(payload) if payload.card_id == card_id => {
                created = Some((&payload.title, &payload.body));
            }
            EventKind::CardMove(payload) if payload.card_id == card_id => {
                moves.push(format!("- {} -> {} ({} @ {})", payload.from, payload.to, outcome_name(payload.outcome), event.created_at));
            }
            EventKind::CardComment(payload) if payload.card_id == card_id => {
                comments.push(format!("- [{}] {}: {}", event.created_at, payload.author, payload.body));
            }
            EventKind::RunStarted(payload) if payload.card_id == card_id => {
                runs.push((
                    payload.run_id.as_str(),
                    format!("- {} column={} attempt={} started={}", payload.run_id, payload.column_id, payload.attempt, event.created_at),
                ));
            }
            EventKind::RunFinished(payload) => {
                outcomes.insert(payload.run_id.as_str(), (payload.outcome, event.created_at));
            }
            EventKind::RunTimeout(payload) => {
                outcomes.insert(payload.run_id.as_str(), (Outcome::Timeout, event.created_at));
            }
            _ => {}
        }
    }
    let (title, body) = created?;
    let mut out = format!("# Card {card_id}: {title}\n");
    if !body.is_empty() {
        let _ = write!(out, "\n{body}\n");
    }
    if !moves.is_empty() {
        out.push_str("\n## Moves\n");
        out.push_str(&moves.join("\n"));
        out.push('\n');
    }
    if !runs.is_empty() {
        out.push_str("\n## Column runs\n");
        for (run_id, line) in runs {
            match outcomes.get(run_id) {
                Some((outcome, ended)) => writeln!(out, "{line} outcome={} ended={ended}", outcome_name(*outcome)),
                None => writeln!(out, "{line} outcome=open"),
            }
            .expect("writing to String cannot fail");
        }
    }
    if !comments.is_empty() {
        out.push_str("\n## Comments\n");
        out.push_str(&comments.join("\n"));
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kanban::events::*;

    fn event(seq: u64, kind: EventKind) -> KanbanEvent {
        KanbanEvent { id: format!("kev_{seq}"), board_id: "board_t".into(), seq, created_at: 1_000 + seq, kind }
    }

    fn events() -> Vec<KanbanEvent> {
        vec![
            event(
                1,
                EventKind::CardCreate(CardCreatePayload {
                    card_id: "card_a".into(),
                    column_id: "requirements".into(),
                    title: "Add login".into(),
                    body: "Email login".into(),
                }),
            ),
            event(
                2,
                EventKind::CardComment(CardCommentPayload {
                    card_id: "card_a".into(),
                    author: "human".into(),
                    body: "do this first".into(),
                }),
            ),
            event(
                3,
                EventKind::CardMove(CardMovePayload {
                    card_id: "card_a".into(),
                    from: "requirements".into(),
                    to: "implementing".into(),
                    outcome: Outcome::Success,
                }),
            ),
            event(
                4,
                EventKind::RunStarted(RunStartedPayload {
                    run_id: "board_t:card_a:implementing:1".into(),
                    card_id: "card_a".into(),
                    column_id: "implementing".into(),
                    attempt: 1,
                }),
            ),
            event(
                5,
                EventKind::RunFinished(RunFinishedPayload { run_id: "board_t:card_a:implementing:1".into(), outcome: Outcome::Failure }),
            ),
            event(
                6,
                EventKind::RunStarted(RunStartedPayload {
                    run_id: "board_t:card_a:implementing:2".into(),
                    card_id: "card_a".into(),
                    column_id: "implementing".into(),
                    attempt: 2,
                }),
            ),
            // 其它卡片的事件不得混入
            event(7, EventKind::CardComment(CardCommentPayload { card_id: "card_b".into(), author: "human".into(), body: "noise".into() })),
        ]
    }

    #[test]
    fn render_is_deterministic_and_complete() {
        let events = events();
        let first = render_card_context(&events, "card_a").unwrap();
        let second = render_card_context(&events, "card_a").unwrap();
        assert_eq!(first, second, "同一事件切片两次渲染必须逐字节一致");
        assert!(first.contains("# Card card_a: Add login"));
        assert!(first.contains("Email login"));
        assert!(first.contains("requirements -> implementing (success @ 1003)"));
        assert!(first.contains("board_t:card_a:implementing:1 column=implementing attempt=1 started=1004 outcome=failure ended=1005"));
        assert!(first.contains("board_t:card_a:implementing:2 column=implementing attempt=2 started=1006 outcome=open"));
        assert!(first.contains("[1002] human: do this first"));
        assert!(!first.contains("noise"), "其它卡片的事件不得混入渲染");
    }

    #[test]
    fn render_unknown_card_returns_none() {
        assert!(render_card_context(&events(), "card_nope").is_none());
    }
}
