//! tool_calls 执行段：连续只读并批并行、写工具串行，协议消息按调用序落历史。

use crate::llm::Message;
use crate::llm::tool::ToolCall;

use super::context::AgentContext;
use super::events::AgentEvent;
use super::execute::execute_tool;
use super::helpers::{is_read_only_tool, result_display, result_text, summarize_args};

/// 执行一轮 tool_calls，返回 (aborted, loop_stop)。
/// assistant.tool_calls 与逐条 tool_result 在此落入历史（中断场景同样落，协议消息顺序不变）。
pub async fn execute_calls(
    ctx: &mut AgentContext,
    text: String,
    calls: Vec<ToolCall>,
    messages: &mut Vec<Message>,
) -> (bool, Option<crate::agent::loop_detect::LoopStop>) {
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
        // 工具执行段：cancel 打断即落 interrupted 终态（不等待执行完成，后续任务由 registry 收尾）
        let cancel = ctx.cancel.clone();
        let cx: &AgentContext = ctx;
        let run_batch = futures::future::join_all(batch.iter().map(|c| execute_tool(&c.name, &c.arguments, cx)));
        let batch_results = match &cancel {
            Some(token) => tokio::select! {
                r = run_batch => r,
                _ = token.wait() => batch.iter().map(|_| Err("(interrupted)".to_string())).collect::<Vec<_>>(),
            },
            None => run_batch.await,
        };
        for (call, result) in batch.iter().zip(batch_results) {
            if matches!(&result, Err(e) if e == "(interrupted)") {
                (ctx.on_event)(AgentEvent::ToolResult {
                    name: call.name.clone(),
                    summary: "interrupted".into(),
                    output: "interrupted".into(),
                });
                results.push(result);
                aborted = true;
                break;
            }
            (ctx.on_event)(AgentEvent::ToolResult {
                name: call.name.clone(),
                summary: result_display(&result),
                output: result_text(&result),
            });
            if let crate::agent::loop_detect::LoopVerdict::Stop(stop) =
                ctx.loop_detector.record(&call.name, &call.arguments, &result_text(&result))
            {
                loop_stop = Some(stop);
                results.push(result);
                break;
            }
            results.push(result);
        }
        if aborted || loop_stop.is_some() {
            break;
        }
        idx = batch_end;
    }
    let assistant_calls: Vec<crate::llm::types::AssistantToolCall> =
        calls.iter().map(|c| crate::llm::types::AssistantToolCall::function(c.id.clone(), c.name.clone(), c.arguments.clone())).collect();
    messages.push(Message::assistant_with_tools(text, assistant_calls));
    for (call, result) in calls.into_iter().zip(results) {
        messages.push(Message::tool_result(call.id, call.name, result_text(&result)));
    }
    (aborted, loop_stop)
}
