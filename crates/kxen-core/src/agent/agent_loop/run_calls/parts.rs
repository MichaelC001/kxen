//! 迭代持久化 parts 装配与 tool_result 落历史（run_calls 拆分，350 行门禁）。

use crate::llm::Message;
use crate::llm::tool::ToolCall;

use super::super::helpers::{result_text, summarize_args};

/// 中断/截断时 results 短于 calls：provider 要求每个 tool_call 都有配对 tool_result，
/// 否则历史被毒化、下一次请求被 400 拒绝且不可自愈（P1-1）。未执行的 call 补占位结果。
/// 返回按调用序对齐的输出文本：迭代持久化 parts 与内存 wire 必须共用同一份，
/// 否则落盘记录与模型当轮所见分叉。
pub(super) fn push_tool_results(calls: &[ToolCall], results: Vec<Result<String, String>>, messages: &mut Vec<Message>) -> Vec<String> {
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
/// 工具自身已有输出上限，10k 转录截断是唯一有损点。timings 与 calls 对齐，None = 未执行（unknown）。
pub(super) fn iteration_parts(
    text: String,
    calls: &[ToolCall],
    outputs: Vec<String>,
    timings: &[Option<(u64, u64)>],
) -> Vec<crate::core::session::Part> {
    use crate::core::session::Part;
    let mut parts = Vec::with_capacity(calls.len() + 1);
    if !text.is_empty() {
        parts.push(Part::Text { text: text.into() });
    }
    for (index, (call, output)) in calls.iter().zip(outputs).enumerate() {
        let (started_at, finished_at) = timings.get(index).copied().flatten().unzip();
        parts.push(Part::ToolCall {
            name: call.name.clone(),
            input: serde_json::json!(summarize_args(&call.name, &call.arguments)),
            output: output.into(),
            args: Some(serde_json::from_str(&call.arguments).unwrap_or_else(|_| serde_json::json!(call.arguments))),
            id: Some(call.id.clone()),
            started_at,
            finished_at,
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
        let parts = iteration_parts("该轮文本".into(), &calls, outputs, &[Some((100, 250)), None]);

        assert!(matches!(&parts[0], crate::core::session::Part::Text { text } if text == "该轮文本"));
        assert!(
            matches!(&parts[1], crate::core::session::Part::ToolCall { name, output, id, args: Some(args), started_at: Some(100), finished_at: Some(250), .. }
            if name == "read" && output.len() == 20_000 && id.as_deref() == Some("c1") && *args == serde_json::json!({})),
            "output 全量内联不截断，id 存 provider call id，计时如实落盘"
        );
        assert!(
            matches!(&parts[2], crate::core::session::Part::ToolCall { output, id, started_at: None, finished_at: None, .. }
            if output == "o2" && id.as_deref() == Some("c2")),
            "未执行/无计时的 call 起止保持 None = unknown"
        );

        let no_text = iteration_parts(String::new(), &calls, vec!["x".into(), "y".into()], &[None, None]);
        assert!(no_text.iter().all(|p| matches!(p, crate::core::session::Part::ToolCall { .. })), "无文本时不产生空 Text part");
    }
}
