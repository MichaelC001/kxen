//! tool_calls 执行段：连续只读并批并行、写工具串行，协议消息按调用序落历史。

use crate::agent::cancel::CancelToken;
use crate::llm::Message;
use crate::llm::tool::ToolCall;

use super::context::AgentContext;
use super::events::AgentEvent;
use super::execute::execute_tool;
use super::helpers::{is_read_only_tool, result_display, result_text, summarize_args};

#[path = "run_calls/parts.rs"]
mod parts;
use parts::{iteration_parts, push_tool_results};

/// 执行一轮 tool_calls，返回 (aborted, loop_stop, 本迭代的持久化 parts)。
/// assistant.tool_calls 与逐条 tool_result 在此落入内存历史（中断场景同样落，协议消息顺序不变）；
/// parts（Text? + ToolCall×N，output 已填）交给 run loop 在下一次 LLM 请求前落盘。
pub async fn execute_calls(
    ctx: &mut AgentContext,
    text: String,
    calls: Vec<ToolCall>,
    messages: &mut Vec<Message>,
) -> (bool, Option<crate::agent::loop_detect::LoopStop>, Option<String>, Vec<crate::core::session::Part>) {
    // assistant 消息带标准 tool_calls，结果用 Role::Tool 回传。
    // 同一 call 数据要进两条协议消息（assistant.tool_calls + tool_result），arguments 只克隆一次。
    let mut results = Vec::with_capacity(calls.len());
    // 与 calls 对齐的执行计时（未执行的 call 保持 None = unknown，不许用批次边界虚构）
    let mut timings: Vec<Option<(u64, u64)>> = vec![None; calls.len()];
    let mut loop_stop: Option<crate::agent::loop_detect::LoopStop> = None;
    let mut aborted = false;
    let mut journal_failure = None;
    // 连续只读调用并批并行执行（P2-04）：read/glob/grep/search 类互不依赖，串行白等 IO；
    // 写工具保持顺序。事件与结果始终按调用序落出，协议消息顺序不变。
    let mut idx = 0usize;
    while idx < calls.len() {
        let batch_end = if !is_read_only_tool(&calls[idx].name, ctx) {
            idx + 1
        } else {
            let mut e = idx + 1;
            while e < calls.len() && is_read_only_tool(&calls[e].name, ctx) {
                e += 1;
            }
            e
        };
        let batch = &calls[idx..batch_end];
        for call in batch {
            (ctx.on_event)(AgentEvent::ToolCall {
                name: call.name.clone(),
                summary: summarize_args(&call.name, &call.arguments),
                arguments: call.arguments.clone(),
            });
        }
        // 工具执行段逐项挂 cancel。会持有 Provider/进程/子 run 状态的工具先获得短暂清理窗口，
        // 避免直接 drop future 造成 UNKNOWN 用量未记账、子代理状态悬挂或前台进程泄漏。
        let cancel = ctx.cancel.clone();
        let cx: &AgentContext = ctx;
        let batch_results = futures::future::join_all(batch.iter().map(|call| execute_one(call, cx, cancel.clone()))).await;
        for (offset, (call, (result, started, finished))) in batch.iter().zip(batch_results).enumerate() {
            timings[idx + offset] = Some((started, finished));
            if let Err(error) = &result
                && let Some(error) = error.strip_prefix(DCP_JOURNAL_FATAL)
            {
                journal_failure = Some(error.trim_start_matches(':').trim().to_string());
            }
            if is_interrupted(&result) {
                (ctx.on_event)(AgentEvent::ToolResult {
                    name: call.name.clone(),
                    summary: "interrupted".into(),
                    output: "interrupted".into(),
                });
                results.push(result);
                aborted = true;
                continue;
            }
            (ctx.on_event)(AgentEvent::ToolResult {
                name: call.name.clone(),
                summary: result_display(&result),
                output: result_text(&result),
            });
            // join_all 已把同批其余调用跑完：中断后它们的真实结果仍要按序落历史，
            // 否则已完成的写工具会被占位符覆盖，模型按「未执行」盲目重试
            if !aborted
                && let crate::agent::loop_detect::LoopVerdict::Stop(stop) =
                    ctx.loop_detector.record(&call.name, &call.arguments, &result_text(&result))
            {
                loop_stop = Some(stop);
                results.push(result);
                break;
            }
            results.push(result);
            if journal_failure.is_some() {
                break;
            }
        }
        // 工具跑完后才观察到取消（execute_one 保留了真实结果）：不再启动后续批次
        if !aborted && ctx.cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            aborted = true;
        }
        if aborted || loop_stop.is_some() || journal_failure.is_some() {
            break;
        }
        idx = batch_end;
    }
    let assistant_calls: Vec<crate::llm::types::AssistantToolCall> =
        calls.iter().map(|c| crate::llm::types::AssistantToolCall::function(c.id.clone(), c.name.clone(), c.arguments.clone())).collect();
    messages.push(Message::assistant_with_tools(text.clone(), assistant_calls));
    let outputs = push_tool_results(&calls, results, messages);
    let parts = iteration_parts(text, &calls, outputs, &timings);
    (aborted, loop_stop, journal_failure, parts)
}

const CANCEL_CLEANUP_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// 中断占位统一前缀：execute_calls 据此识别中断并停出。占位文本同时回传模型，
/// 必须写明副作用是否可知，否则模型会把已生效的写操作当作未执行而盲目重试。
const INTERRUPTED: &str = "(interrupted)";
const DCP_JOURNAL_FATAL: &str = "DCP_TOOL_JOURNAL_FATAL";

fn is_interrupted(result: &Result<String, String>) -> bool {
    matches!(result, Err(e) if e.starts_with(INTERRUPTED))
}

/// 工具跑完后才观察到取消：副作用已发生，保留真实结果并标注终态，模型不会当中断重试。
fn completed_before_cancel(result: Result<String, String>) -> Result<String, String> {
    result.map(|output| {
        format!("{output}\n\n{INTERRUPTED} run cancelled after this tool completed; the result above is final, do not retry this call")
    })
}

fn needs_cancel_cleanup(name: &str) -> bool {
    // tool_define/tool_undefine 挂审批等待（broker 处理 abort），dyn__ 持有沙箱线程/子工具调用：都要清理窗口
    matches!(name, "exec" | "agent" | "workflow" | "goal" | "websearch" | "tool_define" | "tool_undefine")
        || name.starts_with("mcp__")
        || name.starts_with(crate::agent::dynamic::NAME_PREFIX)
}

/// 单调用计时包装：started/finished 取 execute_one 边界（含 journal 前后记），中断占位同样有真实结束点。
async fn execute_one(call: &ToolCall, ctx: &AgentContext, cancel: Option<CancelToken>) -> (Result<String, String>, u64, u64) {
    let started = crate::core::shared::now_ms();
    let result = execute_one_timed(call, ctx, cancel).await;
    (result, started, crate::core::shared::now_ms())
}

async fn execute_one_timed(call: &ToolCall, ctx: &AgentContext, cancel: Option<CancelToken>) -> Result<String, String> {
    let now = crate::core::shared::now_ms();
    if let Some(journal) = &ctx.tool_journal {
        match journal
            .before(&call.id, &call.name, &call.arguments, now)
            .map_err(|error| format!("{DCP_JOURNAL_FATAL}: before tool execution: {error}"))?
        {
            crate::agent::dcp::ToolBoundaryAction::Execute => {}
            crate::agent::dcp::ToolBoundaryAction::Replay { output, is_error } => {
                return if is_error { Err(output) } else { Ok(output) };
            }
            crate::agent::dcp::ToolBoundaryAction::Pause { reason } => {
                return Err(format!("approval required: {reason}"));
            }
        }
    }
    let result = execute_one_inner(call, ctx, cancel).await;
    if let Some(journal) = &ctx.tool_journal {
        let now = crate::core::shared::now_ms();
        if matches!(&result, Err(error) if error.contains("tool may still have taken effect")) {
            journal
                .mark_unknown(&call.id, result_text(&result).as_str(), now)
                .map_err(|error| format!("{DCP_JOURNAL_FATAL}: mark UNKNOWN tool outcome: {error}"))?;
        } else {
            journal
                .after(&call.id, &call.name, &call.arguments, result_text(&result).as_str(), result.is_err(), now)
                .map_err(|error| format!("{DCP_JOURNAL_FATAL}: after tool execution: {error}"))?;
        }
    }
    result
}

async fn execute_one_inner(call: &ToolCall, ctx: &AgentContext, cancel: Option<CancelToken>) -> Result<String, String> {
    let run = execute_tool(&call.name, &call.arguments, ctx);
    tokio::pin!(run);
    let Some(cancel) = cancel else { return run.await };
    if !needs_cancel_cleanup(&call.name) {
        return tokio::select! {
            result = &mut run => result,
            _ = cancel.wait() => Err(INTERRUPTED.to_string()),
        };
    }

    let result = tokio::select! {
        result = &mut run => result,
        _ = cancel.wait() => {
            return match tokio::time::timeout(CANCEL_CLEANUP_GRACE, &mut run).await {
                // 清理窗口内完成：结果可知，必须留给模型
                Ok(result) => completed_before_cancel(result),
                // 窗口耗尽仍在跑：结果 UNKNOWN，提示先核实状态再决定重试
                Err(_) => Err(format!("{INTERRUPTED} cancelled during execution; the tool may still have taken effect, verify state before retrying")),
            };
        },
    };
    if cancel.is_cancelled() { completed_before_cancel(result) } else { result }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_and_process_tools_receive_a_cancel_cleanup_window() {
        for name in
            ["exec", "agent", "workflow", "goal", "websearch", "tool_define", "tool_undefine", "mcp__server__tool", "dyn__demo_01234567"]
        {
            assert!(needs_cancel_cleanup(name), "{name}");
        }
        for name in ["read", "glob", "grep", "webfetch", "browser", "mcp_server_tool"] {
            assert!(!needs_cancel_cleanup(name), "{name}");
        }
    }

    #[test]
    fn completed_tool_result_survives_late_cancel_with_annotation() {
        let result = completed_before_cancel(Ok("file written".to_string()));
        let text = result.as_ref().expect("completed result must survive cancellation").clone();
        assert!(text.starts_with("file written"));
        assert!(text.contains("do not retry"));
        assert!(!is_interrupted(&result), "annotated completion must not read as interrupted");
    }

    #[test]
    fn completed_tool_error_passes_through_unchanged() {
        let result = completed_before_cancel(Err("disk full".to_string()));
        assert_eq!(result, Err("disk full".to_string()));
        assert!(!is_interrupted(&result));
    }

    #[test]
    fn interrupted_markers_are_detected_by_prefix() {
        assert!(is_interrupted(&Err("(interrupted)".to_string())));
        assert!(is_interrupted(&Err(
            "(interrupted) cancelled during execution; the tool may still have taken effect, verify state before retrying".to_string()
        )));
        assert!(!is_interrupted(&Err("ERROR: disk full".to_string())));
        assert!(!is_interrupted(&Ok("(interrupted) appeared in normal output".to_string())));
    }
}
