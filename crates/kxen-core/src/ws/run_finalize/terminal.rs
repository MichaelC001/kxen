use std::path::Path;
use std::sync::Arc;

use crate::AppState;
use kxen_core::agent::agent_loop::AgentEvent;
use kxen_core::core::session::{self, Message, Part, Role};

pub(super) fn commit_and_publish(
    state: &Arc<AppState>,
    sessions_dir: &Path,
    message: &Message,
    stream_id: &str,
    intended: &AgentEvent,
    schedule_job_id: Option<&str>,
) -> bool {
    commit_and_publish_with(sessions_dir, message, &state.bus, stream_id, intended, |terminal| {
        schedule_job_id.map_or(Ok(()), |job_id| super::schedule::record_schedule_terminal(state, &message.session_id, job_id, terminal))
    })
}

pub(super) fn commit_and_publish_with(
    sessions_dir: &Path,
    message: &Message,
    bus: &kxen_core::core::event::EventBus,
    stream_id: &str,
    intended: &AgentEvent,
    mut record_schedule: impl FnMut(&AgentEvent) -> Result<(), String>,
) -> bool {
    if let Err(message_error) = persist_assistant(sessions_dir, message) {
        let mut terminal = AgentEvent::Error { message: message_error };
        if let Err(schedule_error) = record_schedule(&terminal) {
            terminal = AgentEvent::Error {
                message: format!("{}; schedule failure status persistence also failed: {schedule_error}", terminal_message(&terminal)),
            };
        }
        super::publish_terminal(bus, &message.session_id, stream_id, &terminal, message.model.as_ref());
        return false;
    }
    if let Err(error) = record_schedule(intended) {
        let terminal = AgentEvent::Error { message: format!("schedule terminal persistence failed; queue continuation paused: {error}") };
        super::publish_terminal(bus, &message.session_id, stream_id, &terminal, message.model.as_ref());
        return false;
    }
    super::publish_terminal(bus, &message.session_id, stream_id, intended, message.model.as_ref());
    true
}

fn persist_assistant(sessions_dir: &Path, message: &Message) -> Result<(), String> {
    match session::append_message_durable(sessions_dir, message) {
        Ok(_) => Ok(()),
        Err(error) if error.committed() => match session::repair_message_durability(sessions_dir, message, &error) {
            Ok(_) => {
                tracing::warn!(session = message.session_id, message_id = message.id, %error, "terminal PostCommit durability repaired");
                Ok(())
            }
            Err(repair) => {
                tracing::error!(session = message.session_id, message_id = message.id, %error, %repair, "terminal durability repair failed");
                Err(format!(
                    "session terminal persistence is indeterminate and repair failed; queue continuation paused: {error}; repair: {repair}"
                ))
            }
        },
        Err(error) => {
            tracing::error!(session = message.session_id, message_id = message.id, %error, "terminal persistence failed before commit");
            Err(format!("session terminal persistence failed before commit; queue continuation paused: {error}"))
        }
    }
}

pub(in crate::ws) fn finish_persisted(
    state: &Arc<AppState>,
    sessions_dir: &Path,
    session_id: &str,
    stream_id: &str,
    model: Option<&kxen_core::llm::ModelRef>,
    schedule_job_id: Option<&str>,
    terminal: AgentEvent,
) -> bool {
    let message = early_message(session_id, model, &terminal);
    commit_and_publish(state, sessions_dir, &message, stream_id, &terminal, schedule_job_id)
}

/// run 收尾消息装配：parts + 实际路由模型 + run 统计快照一次组好（stats 仅收尾消息携带）。
pub(super) fn finalize_message(
    session_id: &str,
    parts: Vec<Part>,
    model: Option<kxen_core::llm::ModelRef>,
    stats: Option<kxen_core::agent::agent_loop::RunStats>,
) -> Message {
    let mut message = session::new_message(session_id, Role::Assistant, parts);
    message.model = model;
    message.stats = stats.map(|s| session::MessageRunStats {
        ttft_ms: s.ttft_ms,
        duration_ms: s.duration_ms,
        input_tokens: s.input_tokens,
        output_tokens: s.output_tokens,
        tokens_per_sec: s.tokens_per_sec,
        usage_complete: s.usage_complete,
    });
    message
}

pub(super) fn early_message(session_id: &str, model: Option<&kxen_core::llm::ModelRef>, terminal: &AgentEvent) -> Message {
    let text = match terminal {
        AgentEvent::Error { message } => format!("(错误: {message})"),
        AgentEvent::Aborted => "(已中断)".to_string(),
        _ => "(run 在启动前结束)".to_string(),
    };
    let mut message = session::new_message(session_id, Role::Assistant, vec![Part::Text { text: text.into() }]);
    message.model = model.cloned();
    message
}

pub(in crate::ws) fn publish_direct_scheduled(
    state: &Arc<AppState>,
    session_id: &str,
    stream_id: &str,
    schedule_job_id: Option<&str>,
    terminal: AgentEvent,
) -> bool {
    if let Some(job_id) = schedule_job_id
        && let Err(error) = super::schedule::record_schedule_terminal(state, session_id, job_id, &terminal)
    {
        let error = AgentEvent::Error { message: format!("schedule terminal persistence failed; queue continuation paused: {error}") };
        super::publish_terminal(&state.bus, session_id, stream_id, &error, None);
        return false;
    }
    super::publish_terminal(&state.bus, session_id, stream_id, &terminal, None);
    true
}

fn terminal_message(terminal: &AgentEvent) -> &str {
    match terminal {
        AgentEvent::Error { message } => message,
        _ => "terminal persistence failed",
    }
}

/// finalize 消息 parts：transcript 只剩 Reasoning（tool 交互已逐迭代落盘，这里再组 ToolCall
/// 就是双写）+ 最终文本 + 中断标记。兜底只在「本 run 无迭代落盘且无任何文本」时触发——
/// reasoning 不算输出；迭代消息已落盘的 run 无最终文本也不是无声结束。
pub(super) fn assemble_parts(transcript: Vec<Part>, final_text: String, aborted: bool, iterations_persisted: bool) -> Vec<Part> {
    let mut parts = transcript;
    if !final_text.is_empty() {
        parts.push(Part::Text { text: final_text.into() });
    }
    if aborted {
        parts.push(Part::Text { text: "(已中断)".into() });
    }
    if !iterations_persisted && !parts.iter().any(|part| matches!(part, Part::Text { .. })) {
        parts.push(Part::Text { text: "(run 异常结束，无输出——请重试或发送「继续」)".into() });
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_parts_never_emits_tool_calls_and_skips_fallback_when_iterations_persisted() {
        let reasoning = vec![Part::Reasoning { text: "thinking".into() }];
        let parts = assemble_parts(reasoning, String::new(), false, true);
        assert_eq!(parts.len(), 1, "迭代已落盘的 run 无最终文本不兜底，reasoning 原样保留");
        assert!(!parts.iter().any(|part| matches!(part, Part::ToolCall { .. })), "finalize 不得重复落 ToolCall");
    }

    #[test]
    fn assemble_parts_falls_back_only_without_iterations_and_text() {
        let parts = assemble_parts(vec![], String::new(), false, false);
        assert!(matches!(&parts[0], Part::Text { text } if text.contains("无输出")), "无迭代且无文本必须兜底");

        let reasoning_only = assemble_parts(vec![Part::Reasoning { text: "r".into() }], String::new(), false, false);
        assert!(reasoning_only.iter().any(|part| matches!(part, Part::Text { text } if text.contains("无输出"))), "reasoning 不算输出");

        let aborted = assemble_parts(vec![], String::new(), true, false);
        assert!(aborted.iter().any(|part| matches!(part, Part::Text { text } if text == "(已中断)")));
        assert!(!aborted.iter().any(|part| matches!(part, Part::Text { text } if text.contains("无输出"))), "中断标记已是文本");

        let done = assemble_parts(vec![], "答案".into(), false, true);
        assert!(matches!(done.as_slice(), [Part::Text { text }] if text == "答案"));
    }
}
