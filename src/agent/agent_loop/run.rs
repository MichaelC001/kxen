//! 单轮 run loop：LLM 流式 -> tool_call 累积 -> 工具执行 -> 结果回传 -> 继续。

use futures::StreamExt;
use crate::llm::tool::ToolCallAccumulator;
use crate::llm::{Delta, LlmClient, Message};

use super::context::AgentContext;
use super::events::{AgentEvent, AgentOutcome, RunStats};
use super::execute::execute_tool;
use super::helpers::{result_summary, result_text, summarize_args};

pub async fn run_turn(ctx: &mut AgentContext, mut messages: Vec<Message>) -> AgentOutcome {
    let base_tools = match ctx.allowed_tools {
        Some(allowed) => crate::agent::tools_spec::core_tools()
            .into_iter()
            .filter(|t| allowed.contains(&t.function.name.as_str()))
            .collect(),
        None => crate::agent::tools_spec::core_tools(),
    };
    let mut turns = 0u32;
    let mut final_text = String::new();
    let mut aborted = false;

    // 统计：TTFT（首个 Text/Reasoning delta）/ 总耗时 / tokens
    let started = std::time::Instant::now();
    let mut ttft: Option<std::time::Duration> = None;
    let mut usage: Option<(u64, u64)> = None;
    let stats = |ttft: Option<std::time::Duration>, usage: Option<(u64, u64)>| {
        let (input, output) = usage.unwrap_or((0, 0));
        let duration = started.elapsed();
        let gen_ms = duration.as_millis() as u64;
        Some(RunStats {
            ttft_ms: ttft.map(|t| t.as_millis() as u64).unwrap_or(0),
            duration_ms: gen_ms,
            input_tokens: input,
            output_tokens: output,
            tokens_per_sec: if gen_ms > 0 { output * 1000 / gen_ms } else { 0 },
        })
    };

    // 系统提示由 loop 统一注入（身份 + 工具策略 + write-goal + 焦点 goal），调用方不重复造。
    let system_owned = !matches!(messages.first(), Some(m) if m.role == crate::llm::types::Role::System);
    let mut last_involved: Vec<std::path::PathBuf> = Vec::new();
    if system_owned {
        let involved = ctx.tracker.files();
        last_involved = involved.clone();
        messages.insert(0, Message::system(crate::agent::prompt::system_prompt(&ctx.workdir, &involved)));
    }

    'outer: loop {
        turns += 1;
        if turns > ctx.max_turns {
            (ctx.on_event)(AgentEvent::Error { message: format!("max turns ({}) reached", ctx.max_turns) });
            break;
        }
        if ctx.cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            aborted = true;
            break 'outer;
        }

        // 渐进披露 + 身份过滤：每轮重建（tool_search 挂载下轮可见；team 系工具按身份开关）
        let mut tools = base_tools.clone();
        tools.retain(|t| match t.function.name.as_str() {
            "team" => ctx.team.is_some() && ctx.team_identity.is_none(),
            "send_message" | "team_task" => ctx.team_identity.is_some(),
            _ => true,
        });
        if let Some(extras) = &ctx.extras {
            let enabled = crate::core::shared::lock(&extras.extra_tools);
            tools.extend(crate::agent::tools_spec::deferred_tools().into_iter().filter(|t| enabled.contains(&t.function.name)));
        }

        // mid-turn 刷新：涉及文件变化时重建系统提示（OKF globs 激活 / goal 状态 / 多层就近）
        if system_owned {
            let involved = ctx.tracker.files();
            if involved != last_involved {
                messages[0] = Message::system(crate::agent::prompt::system_prompt(&ctx.workdir, &involved));
                last_involved = involved;
            }
        }

        let mut acc = ToolCallAccumulator::default();
        let mut text = String::new();
        let mut stream = LlmClient::stream_with_tools(&ctx.model, &messages, &tools, &ctx.store);

        // stream 消费：cancel 即时打断（select 轮询 Delta 与取消令牌的等待）
        loop {
            let delta = match &ctx.cancel {
                Some(token) => tokio::select! {
                    d = stream.next() => d,
                    _ = token.wait() => { aborted = true; break; }
                },
                None => stream.next().await,
            };
            let Some(delta) = delta else { break };
            match delta {
                Delta::Text(t) => {
                    if ttft.is_none() {
                        ttft = Some(started.elapsed());
                    }
                    text.push_str(&t);
                    (ctx.on_event)(AgentEvent::Text { text: t });
                }
                Delta::Reasoning(r) => {
                    if ttft.is_none() {
                        ttft = Some(started.elapsed());
                    }
                    (ctx.on_event)(AgentEvent::Reasoning { text: r });
                }
                Delta::ToolFragments(fragments) => acc.push(&fragments),
                Delta::Usage { input, output } => usage = Some((input, output)),
                Delta::Done => break,
                Delta::Error(e) => {
                    (ctx.on_event)(AgentEvent::Error { message: e });
                    return AgentOutcome { final_text, turns, aborted, stats: stats(ttft, usage) };
                }
                Delta::ToolCall { .. } => {}
            }
        }
        if aborted {
            break 'outer;
        }

        let calls = acc.take();
        if calls.is_empty() {
            final_text = text;
            (ctx.on_event)(AgentEvent::Done { turns, stats: stats(ttft, usage) });
            break;
        }

        // assistant 消息带标准 tool_calls，结果用 Role::Tool 回传。
        // 同一 call 数据要进两条协议消息（assistant.tool_calls + tool_result），arguments 只克隆一次。
        let mut results = Vec::with_capacity(calls.len());
        let mut loop_stop: Option<crate::agent::loop_detect::LoopStop> = None;
        for call in &calls {
            (ctx.on_event)(AgentEvent::ToolCall { name: call.name.clone(), summary: summarize_args(&call.arguments) });
            // 工具执行段：cancel 打断即落 interrupted 终态（不等待执行完成，后续任务由 registry 收尾）
            let cancel = ctx.cancel.clone();
            let result = match &cancel {
                Some(token) => tokio::select! {
                    r = execute_tool(&call.name, &call.arguments, ctx) => r,
                    _ = token.wait() => Err("(interrupted)".to_string()),
                },
                None => execute_tool(&call.name, &call.arguments, ctx).await,
            };
            let interrupted = matches!(&result, Err(e) if e == "(interrupted)");
            if interrupted {
                (ctx.on_event)(AgentEvent::ToolResult { name: call.name.clone(), summary: "interrupted".into() });
                results.push(result);
                aborted = true;
                break;
            }
            (ctx.on_event)(AgentEvent::ToolResult { name: call.name.clone(), summary: result_summary(&call.name, &result) });
            if let crate::agent::loop_detect::LoopVerdict::Stop(stop) = ctx.loop_detector.record(&call.name, &call.arguments, &result_text(&result)) {
                loop_stop = Some(stop);
                results.push(result);
                break;
            }
            results.push(result);
        }
        let assistant_calls: Vec<crate::llm::types::AssistantToolCall> = calls
            .iter()
            .map(|c| crate::llm::types::AssistantToolCall::function(c.id.clone(), c.name.clone(), c.arguments.clone()))
            .collect();
        messages.push(Message::assistant_with_tools(text, assistant_calls));
        for (call, result) in calls.into_iter().zip(results) {
            messages.push(Message::tool_result(call.id, call.name, result_text(&result)));
        }
        if aborted {
            break 'outer;
        }
        if let Some(stop) = loop_stop {
            // 中断空转：硬停本轮，原因作为结果带出（事件已通知前端）
            let reason = stop.to_string();
            (ctx.on_event)(AgentEvent::Error { message: reason.clone() });
            final_text = reason;
            break;
        }
    }

    if aborted {
        (ctx.on_event)(AgentEvent::Aborted);
    }
    AgentOutcome { final_text, turns, aborted, stats: stats(ttft, usage) }
}
