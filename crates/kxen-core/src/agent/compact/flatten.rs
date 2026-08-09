//! stored 消息压平成模型消息：tool 交互完全重建（assistant_with_tools + 按序 tool_result）。
//! llm_task 构建历史、compact_session 蒸馏输入与 save_run_checkpoint 边界配对同口径，只此一份。

use crate::core::session::{Message as StoredMessage, Part, Role as StoredRole};
use crate::llm::Message;
use crate::llm::types::AssistantToolCall;

/// tool id 回放时一律确定性合成，绝不透传存量 provider id：各 provider 的 id 字符集
/// 约束没有净化矩阵，透传会让跨 provider 切换后的历史被 400 拒绝。
/// part_index 用 part 在消息内的下标（而非 tool 序号）：与消息内容一一绑定，同一消息
/// 重复 flatten 结果逐字节一致。
fn synthesized_call_id(message_id: &str, part_index: usize) -> String {
    format!("call_{message_id}_{part_index}")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

pub fn flatten_stored(view: &[StoredMessage]) -> Vec<Message> {
    view.iter().flat_map(flatten_one).collect()
}

fn flatten_one(stored: &StoredMessage) -> Vec<Message> {
    // Text/Context 口径不变：回放给模型，其余 part（Reasoning/Approval/Image）不回放。
    let mut text = String::new();
    for part in &stored.parts {
        if let Part::Text { text: part } | Part::Context { text: part } = part {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(part);
        }
    }
    if stored.role != StoredRole::Assistant {
        if text.is_empty() {
            return Vec::new();
        }
        return vec![match stored.role {
            StoredRole::User => Message::user(text),
            StoredRole::System => Message::system(text),
            StoredRole::Assistant => unreachable!(),
        }];
    }
    let tool_calls: Vec<(usize, &str, &serde_json::Value, &crate::core::shared::SharedText)> = stored
        .parts
        .iter()
        .enumerate()
        .filter_map(|(index, part)| match part {
            // 存量消息可能无 args 字段：退回 input（摘要 JSON），wire 合法但参数有损，属存量上限
            Part::ToolCall { name, input, output, args, .. } => Some((index, name.as_str(), args.as_ref().unwrap_or(input), output)),
            _ => None,
        })
        .collect();
    if tool_calls.is_empty() {
        if text.is_empty() {
            return Vec::new();
        }
        return vec![Message::assistant(text)];
    }
    let calls: Vec<AssistantToolCall> = tool_calls
        .iter()
        .map(|(index, name, args, _)| {
            AssistantToolCall::function(synthesized_call_id(&stored.id, *index), *name, serde_json::to_string(args).unwrap_or_default())
        })
        .collect();
    let mut out = vec![Message::assistant_with_tools(text, calls)];
    // 每个 call 必须有配对 tool_result（按调用序），否则 wire 被 provider 拒收且不可自愈
    out.extend(
        tool_calls
            .iter()
            .map(|(index, name, _, output)| Message::tool_result(synthesized_call_id(&stored.id, *index), *name, (*output).clone())),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::session::{Message as StoredMessage, Role as StoredRole};
    use crate::llm::types::Role;

    fn stored(id: &str, role: StoredRole, parts: Vec<Part>) -> StoredMessage {
        StoredMessage { id: id.into(), session_id: "ses".into(), role, parts, model: None, created_at: 0 }
    }

    fn tool(name: &str, output: &str, args: Option<serde_json::Value>, id: Option<String>) -> Part {
        Part::ToolCall { name: name.into(), input: serde_json::json!(format!("run {name}")), output: output.into(), args, id }
    }

    #[test]
    fn new_iteration_message_rebuilds_wire_legal_sequence() {
        let view = vec![stored(
            "run-1-0001:t1",
            StoredRole::Assistant,
            vec![
                Part::Text { text: "先看下文件".into() },
                tool("read", "file contents", Some(serde_json::json!({"path": "a.rs"})), Some("provider_x".to_string())),
                tool("exec", "ok", Some(serde_json::json!({"cmd": "ls"})), Some("provider_y".to_string())),
            ],
        )];
        let out = flatten_stored(&view);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].role, Role::Assistant);
        assert_eq!(out[0].content, "先看下文件");
        assert_eq!(out[0].tool_calls.len(), 2);
        assert_eq!(out[1].role, Role::Tool);
        assert_eq!(out[2].role, Role::Tool);
        // 每个 call 有配对 result、顺序保持、id 确定性合成且不透传 provider id
        for (index, call) in out[0].tool_calls.iter().enumerate() {
            assert_eq!(out[index + 1].tool_call_id.as_deref(), Some(call.id.as_str()));
            assert!(call.id.starts_with("call_run-1-0001_t1_"), "provider id 不透传: {}", call.id);
            assert!(call.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "安全字符集: {}", call.id);
        }
        assert_eq!(out[0].tool_calls[0].function.name, "read");
        assert_eq!(out[0].tool_calls[0].function.arguments, r#"{"path":"a.rs"}"#);
        assert_eq!(out[1].content, "file contents");
        assert_eq!(out[2].content, "ok");
        // 确定性：同一输入两次 flatten 逐字节一致
        assert_eq!(flatten_stored(&view)[0].tool_calls[0].id, out[0].tool_calls[0].id);
    }

    #[test]
    fn legacy_packed_message_without_part_ids_rebuilds_the_same_way() {
        // 存量打包消息：一条 Assistant 多个 ToolCall、无 args、无 id、output 曾被 10k 截断
        let view = vec![stored(
            "msg_legacy",
            StoredRole::Assistant,
            vec![Part::Reasoning { text: "old thinking".into() }, tool("exec", "capped output", None, None)],
        )];
        let out = flatten_stored(&view);
        assert_eq!(out.len(), 2, "reasoning 不回放；一条打包消息 -> assistant_with_tools + result");
        assert_eq!(out[0].tool_calls.len(), 1);
        assert_eq!(out[0].tool_calls[0].id, "call_msg_legacy_1");
        assert_eq!(
            out[0].tool_calls[0].function.arguments,
            serde_json::to_string(&serde_json::json!("run exec")).unwrap(),
            "无 args 退化用 input"
        );
        assert_eq!(out[1].tool_call_id.as_deref(), Some("call_msg_legacy_1"));
        assert_eq!(out[1].content, "capped output");
    }

    #[test]
    fn assistant_with_only_tool_calls_and_no_text_still_rebuilds() {
        let view = vec![stored("run-2-0003:t2", StoredRole::Assistant, vec![tool("read", "data", Some(serde_json::json!({})), None)])];
        let out = flatten_stored(&view);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].content, "");
        assert_eq!(out[0].tool_calls.len(), 1);
    }

    #[test]
    fn text_context_and_roles_keep_previous_semantics() {
        let view = vec![
            stored("u1", StoredRole::User, vec![Part::Text { text: "问".into() }, Part::Context { text: "上下文".into() }]),
            stored("a1", StoredRole::Assistant, vec![Part::Text { text: "答".into() }]),
            stored("s1", StoredRole::System, vec![Part::Text { text: "系统".into() }]),
            stored(
                "a2",
                StoredRole::Assistant,
                vec![Part::Approval { command: "rm".into(), reason: "r".into(), decision: "allow".into() }],
            ),
        ];
        let out = flatten_stored(&view);
        assert_eq!(out.len(), 3, "approval 不回放");
        assert_eq!(out[0].role, Role::User);
        assert_eq!(out[0].content, "问\n上下文");
        assert_eq!(out[1].role, Role::Assistant);
        assert_eq!(out[2].role, Role::System);
    }

    #[test]
    fn legacy_tool_call_part_without_id_and_args_deserializes() {
        // 存量 JSONL 无 id/args 字段：serde 缺省兼容，回放按无 id 处理（确定性合成）
        let part: Part = serde_json::from_str(r#"{"type":"tool_call","name":"exec","input":"ls","output":"ok"}"#).unwrap();
        assert!(matches!(part, Part::ToolCall { id: None, args: None, .. }));
    }

    #[test]
    fn empty_messages_and_empty_parts_flatten_to_nothing() {
        assert!(flatten_stored(&[]).is_empty());
        let view = vec![
            stored("r1", StoredRole::Assistant, vec![Part::Reasoning { text: "only".into() }]),
            stored("i1", StoredRole::User, vec![Part::Image { media_type: "image/png".into(), data: "x".into() }]),
        ];
        assert!(flatten_stored(&view).is_empty(), "reasoning/image 不回放");
    }
}
