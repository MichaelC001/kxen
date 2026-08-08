//! tool_calls 执行段：连续只读并批并行、写工具串行，协议消息按调用序落历史。

use crate::llm::Message;
use crate::llm::tool::ToolCall;

use super::context::AgentContext;
use super::events::AgentEvent;
use super::execute::execute_tool;
use super::helpers::{is_read_only_tool, result_display, result_text, summarize_args};

/// 执行一轮 tool_calls，返回 (aborted, loop_stop, 本迭代的持久化 parts)。
/// assistant.tool_calls 与逐条 tool_result 在此落入内存历史（中断场景同样落，协议消息顺序不变）；
/// parts（Text? + ToolCall×N，output 已填）交给 run loop 在下一次 LLM 请求前落盘。
pub async fn execute_calls(
    ctx: &mut AgentContext,
    text: String,
    calls: Vec<ToolCall>,
    messages: &mut Vec<Message>,
) -> (bool, Option<crate::agent::loop_detect::LoopStop>, Vec<crate::core::session::Part>) {
    // assistant 消息带标准 tool_calls，结果用 Role::Tool 回传。
    // 同一 call 数据要进两条协议消息（assistant.tool_calls + tool_result），arguments 只克隆一次。
    let mut results = Vec::with_capacity(calls.len());
    let mut loop_stop: Option<crate::agent::loop_detect::LoopStop> = None;
    let mut aborted = false;
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
        for (call, result) in batch.iter().zip(batch_results) {
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
        }
        // 工具跑完后才观察到取消（execute_one 保留了真实结果）：不再启动后续批次
        if !aborted && ctx.cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            aborted = true;
        }
        if aborted || loop_stop.is_some() {
            break;
        }
        idx = batch_end;
    }
    let assistant_calls: Vec<crate::llm::types::AssistantToolCall> =
        calls.iter().map(|c| crate::llm::types::AssistantToolCall::function(c.id.clone(), c.name.clone(), c.arguments.clone())).collect();
    messages.push(Message::assistant_with_tools(text.clone(), assistant_calls));
    let outputs = push_tool_results(&calls, results, messages);
    let parts = iteration_parts(text, &calls, outputs);
    (aborted, loop_stop, parts)
}

const CANCEL_CLEANUP_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// 中断占位统一前缀：execute_calls 据此识别中断并停出。占位文本同时回传模型，
/// 必须写明副作用是否可知，否则模型会把已生效的写操作当作未执行而盲目重试。
const INTERRUPTED: &str = "(interrupted)";

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
    matches!(name, "exec" | "agent" | "workflow" | "goal" | "websearch") || name.starts_with("mcp__")
}

async fn execute_one(call: &ToolCall, ctx: &AgentContext, cancel: Option<crate::agent::cancel::CancelToken>) -> Result<String, String> {
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

/// 中断/截断时 results 短于 calls：provider 要求每个 tool_call 都有配对 tool_result，
/// 否则历史被毒化、下一次请求被 400 拒绝且不可自愈（P1-1）。未执行的 call 补占位结果。
/// 返回按调用序对齐的输出文本：迭代持久化 parts 与内存 wire 必须共用同一份，
/// 否则落盘记录与模型当轮所见分叉。
fn push_tool_results(calls: &[ToolCall], results: Vec<Result<String, String>>, messages: &mut Vec<Message>) -> Vec<String> {
    let mut results = results.into_iter();
    let mut outputs = Vec::with_capacity(calls.len());
    for call in calls {
        let text = results.next().map(|r| result_text(&r)).unwrap_or_else(|| "(interrupted: aborted before execution)".to_string());
        messages.push(Message::tool_result(call.id.clone(), call.name.clone(), text.clone()));
        outputs.push(text);
    }
    outputs
}

/// 本迭代的持久化 parts：Text?（该轮文本）+ ToolCall×N（按调用序，output 已填，id 存 provider
/// call id 仅供审计配对；回放时 flatten 一律重新合成，不透传）。output 全量内联不截断——
/// 工具自身已有输出上限，10k 转录截断是唯一有损点。
fn iteration_parts(text: String, calls: &[ToolCall], outputs: Vec<String>) -> Vec<crate::core::session::Part> {
    use crate::core::session::Part;
    let mut parts = Vec::with_capacity(calls.len() + 1);
    if !text.is_empty() {
        parts.push(Part::Text { text });
    }
    for (call, output) in calls.iter().zip(outputs) {
        parts.push(Part::ToolCall {
            name: call.name.clone(),
            input: serde_json::json!(summarize_args(&call.name, &call.arguments)),
            output,
            args: Some(serde_json::from_str(&call.arguments).unwrap_or_else(|_| serde_json::json!(call.arguments))),
            id: Some(call.id.clone()),
        });
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::types::Role;

    fn call(id: &str) -> ToolCall {
        ToolCall { id: id.to_string(), name: "read".to_string(), arguments: "{}".to_string() }
    }

    #[test]
    fn aborted_run_pads_placeholder_results_for_unexecuted_calls() {
        // 模拟 abort：4 个 call 只产 1 条结果（中断占位），其余 3 条未执行
        let calls = vec![call("c1"), call("c2"), call("c3"), call("c4")];
        let results = vec![Err("(interrupted)".to_string())];
        let mut messages = Vec::new();
        let outputs = push_tool_results(&calls, results, &mut messages);

        assert_eq!(messages.len(), 4);
        assert!(messages.iter().all(|m| m.role == Role::Tool && m.tool_call_id.is_some()));
        assert_eq!(messages[0].tool_call_id.as_deref(), Some("c1"));
        assert_eq!(messages[0].content, "ERROR: (interrupted)");
        for (msg, id) in messages[1..].iter().zip(["c2", "c3", "c4"]) {
            assert_eq!(msg.tool_call_id.as_deref(), Some(id));
            assert_eq!(msg.content, "(interrupted: aborted before execution)");
        }
        assert_eq!(outputs.len(), 4);
    }

    #[test]
    fn normal_run_pairs_every_call_with_its_result() {
        let calls = vec![call("c1"), call("c2")];
        let results = vec![Ok("a".to_string()), Ok("b".to_string())];
        let mut messages = Vec::new();
        let outputs = push_tool_results(&calls, results, &mut messages);
        assert_eq!(messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(outputs, ["a", "b"], "持久化 parts 与内存 wire 必须共用同一份输出");
    }

    #[test]
    fn aborted_run_padded_outputs_are_returned_for_persistence() {
        let calls = vec![call("c1"), call("c2")];
        let results = vec![Err("(interrupted)".to_string())];
        let mut messages = Vec::new();
        let outputs = push_tool_results(&calls, results, &mut messages);
        assert_eq!(outputs.len(), 2, "未执行的 call 也要在持久化 parts 中有占位输出");
        assert_eq!(outputs[1], "(interrupted: aborted before execution)");
    }

    #[test]
    fn iteration_parts_carry_text_then_calls_with_provider_id_and_full_output() {
        let calls = vec![call("c1"), call("c2")];
        let outputs = vec!["x".repeat(20_000), "o2".to_string()];
        let parts = iteration_parts("该轮文本".into(), &calls, outputs);

        assert!(matches!(&parts[0], crate::core::session::Part::Text { text } if text == "该轮文本"));
        assert!(
            matches!(&parts[1], crate::core::session::Part::ToolCall { name, output, id, args: Some(args), .. }
            if name == "read" && output.len() == 20_000 && id.as_deref() == Some("c1") && *args == serde_json::json!({})),
            "output 全量内联不截断，id 存 provider call id"
        );
        assert!(matches!(&parts[2], crate::core::session::Part::ToolCall { output, id, .. }
            if output == "o2" && id.as_deref() == Some("c2")));

        let no_text = iteration_parts(String::new(), &calls, vec!["x".into(), "y".into()]);
        assert!(no_text.iter().all(|p| matches!(p, crate::core::session::Part::ToolCall { .. })), "无文本时不产生空 Text part");
    }

    #[test]
    fn provider_and_process_tools_receive_a_cancel_cleanup_window() {
        for name in ["exec", "agent", "workflow", "goal", "websearch", "mcp__server__tool"] {
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
