use super::context::AgentContext;
use super::events::AgentEvent;
use super::usage::{UsageAcc, record_goal_turn, run_stats};
use crate::llm::Message;
use crate::llm::tool::ToolCallAccumulator;

pub(super) enum TurnResolution {
    Continue,
    Stop { final_text: String, terminal: Option<AgentEvent>, aborted: bool },
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn resolve(
    ctx: &mut AgentContext,
    messages: &mut Vec<Message>,
    calls: &mut ToolCallAccumulator,
    text: String,
    usage: &mut UsageAcc,
    turns: u32,
    started: std::time::Instant,
    ttft: Option<std::time::Duration>,
    wall_stopped: bool,
) -> TurnResolution {
    if wall_stopped {
        let message = record_goal_turn(ctx, usage, None).unwrap_or_else(|| "goal wall 预算耗尽，停止执行".to_string());
        return stop_with_error(ctx, message);
    }
    let calls = calls.take();
    if calls.is_empty() {
        if let Some(message) = record_goal_turn(ctx, usage, None) {
            return stop_with_error(ctx, message);
        }
        if !text.is_empty() {
            messages.push(Message::assistant(text.clone()));
        }
        let event = AgentEvent::Done { turns, stats: run_stats(started, ttft, usage) };
        (ctx.on_event)(event.clone());
        return TurnResolution::Stop { final_text: text, terminal: Some(event), aborted: false };
    }

    let (exec_aborted, loop_stop, journal_failure, parts) = super::run_calls::execute_calls(ctx, text, calls, messages).await;
    ctx.auxiliary_usage.drain_into(usage);
    // 迭代持久化在下一次 LLM 请求前完成，abort 迭代同样落盘（副作用记录缺口不许继续）。
    // 失败 fail-closed 终止 run，与 commit_user/commit_and_publish 同口径，不能静默吞。
    if let Some(persist) = &ctx.persist_turn
        && let Err(error) = persist(turns, parts)
    {
        return stop_with_error(ctx, format!("turn persistence failed: {error}"));
    }
    if let Some(error) = journal_failure {
        return stop_with_error(ctx, format!("DCP tool journal failed: {error}"));
    }
    if ctx.tool_journal.as_ref().is_some_and(|journal| journal.should_pause()) {
        return TurnResolution::Stop { final_text: String::new(), terminal: None, aborted: false };
    }
    if exec_aborted {
        return TurnResolution::Stop { final_text: String::new(), terminal: None, aborted: true };
    }
    if let Some(message) = record_goal_turn(ctx, usage, loop_stop.as_ref().map(ToString::to_string)) {
        return stop_with_error(ctx, message);
    }
    match loop_stop {
        Some(stop) => stop_with_error(ctx, stop.to_string()),
        None => TurnResolution::Continue,
    }
}

fn stop_with_error(ctx: &AgentContext, message: String) -> TurnResolution {
    let event = AgentEvent::Error { message: message.clone() };
    (ctx.on_event)(event.clone());
    TurnResolution::Stop { final_text: message, terminal: Some(event), aborted: false }
}

/// run 新增的末轮 assistant 文本（member/subagent 落 wake final 用）：
/// 只在 messages 尾消息确为本 run 新推的纯文本时返回（stop_with_error/max_turns 不推消息，
/// final_text 与尾消息对不上即放弃）；fatal_stream_error 的部分产出 final_text 带错误后缀，
/// starts_with 仍命中。落盘内容取尾消息原文，与内存 history 严格一致。
pub(crate) fn new_final_text(messages: &[Message], outcome: &super::events::AgentOutcome) -> Option<String> {
    if outcome.aborted {
        return None;
    }
    let last = messages.last()?;
    if last.role != crate::llm::types::Role::Assistant || !last.tool_calls.is_empty() || last.content.is_empty() {
        return None;
    }
    outcome.final_text.starts_with(last.content.as_str()).then(|| last.content.to_string())
}
