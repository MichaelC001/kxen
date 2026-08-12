use crate::agent::dcp::{ContextFrame, ContextLayer, ProviderNeutralPart, TurnRecord, TurnRecordKind};
use crate::core::identity::ResourceId;
use crate::llm::{Message, types::AssistantToolCall};

pub(super) fn from_context(frame: &ContextFrame) -> Vec<Message> {
    let mut system = String::new();
    let mut input = String::new();
    for segment in &frame.segments {
        let rendered = render_parts(&segment.parts);
        if segment.layer == ContextLayer::NewInput {
            append_block(&mut input, &rendered);
        } else {
            append_block(&mut system, &rendered);
        }
    }
    vec![Message::system(system), Message::user(input)]
}

pub(super) fn append_history(messages: &mut Vec<Message>, records: &[TurnRecord]) {
    for record in records.iter().filter(|record| record.kind == TurnRecordKind::Response) {
        let mut text = String::new();
        let mut calls = Vec::new();
        let mut results = Vec::new();
        for part in &record.parts {
            match part {
                ProviderNeutralPart::Text { text: part } => append_block(&mut text, part),
                ProviderNeutralPart::Data { .. } | ProviderNeutralPart::Artifact { .. } => {
                    append_block(&mut text, &render_part(part));
                }
                ProviderNeutralPart::ToolCall { call_id, tool_name, arguments_json } => {
                    calls.push(AssistantToolCall::function(call_id.to_string(), tool_name.to_string(), arguments_json.clone()))
                }
                ProviderNeutralPart::ToolResult { call_id, content, is_error } => {
                    results.push((call_id.clone(), content.clone(), *is_error));
                }
            }
        }
        if calls.is_empty() {
            if !text.is_empty() {
                messages.push(Message::assistant(text));
            }
            continue;
        }
        messages.push(Message::assistant_with_tools(text, calls));
        for (call_id, content, is_error) in results {
            messages.push(Message::tool_result(call_id.to_string(), "tool", if is_error { format!("ERROR: {content}") } else { content }));
        }
    }
}

pub(super) fn append_resume_state(messages: &mut Vec<Message>, run: &crate::bot::run::BotRunState) {
    if !run.bound_inputs.is_empty() {
        messages.push(Message::user(format!("Owner supplied additional input:\n{}", render_parts(&run.bound_inputs))));
    }
    if !run.approved_operations.is_empty() && run.approval.is_none() {
        messages.push(Message::user(
            "Owner approved the pending operation. Retry the exact same tool call so the durable journal can continue it.",
        ));
    }
}

pub(super) fn session_parts(
    run_id: &ResourceId,
    turn: u32,
    parts: Vec<crate::core::session::Part>,
) -> Result<Vec<ProviderNeutralPart>, String> {
    let mut output = Vec::new();
    for (index, part) in parts.into_iter().enumerate() {
        match part {
            crate::core::session::Part::Text { text } | crate::core::session::Part::Context { text } => {
                output.push(ProviderNeutralPart::Text { text: text.to_string() });
            }
            crate::core::session::Part::Reasoning { text } => output.push(ProviderNeutralPart::Text { text }),
            crate::core::session::Part::ToolCall { name, output: result, args, id, .. } => {
                let provider_id = id.unwrap_or_else(|| format!("{run_id}:{turn}:{index}"));
                let call_id = ResourceId::parse(provider_id.clone()).or_else(|_| {
                    crate::bot::ids::deterministic_id("call", &[run_id.as_str(), &turn.to_string(), &index.to_string(), &provider_id])
                })?;
                output.push(ProviderNeutralPart::ToolCall {
                    call_id: call_id.clone(),
                    tool_name: ResourceId::parse(name)?,
                    arguments_json: serde_json::to_string(&args.unwrap_or_else(|| serde_json::json!({})))
                        .map_err(|error| error.to_string())?,
                });
                let result = result.to_string();
                let (content, is_error) = result.strip_prefix("ERROR: ").map_or((result.clone(), false), |value| (value.into(), true));
                output.push(ProviderNeutralPart::ToolResult { call_id, content, is_error });
            }
            crate::core::session::Part::Approval { command, reason, decision } => {
                output.push(ProviderNeutralPart::Text { text: format!("Approval {decision}: {command}: {reason}") })
            }
            crate::core::session::Part::ContextSources { .. } | crate::core::session::Part::Image { .. } => {}
        }
    }
    if output.is_empty() {
        output.push(ProviderNeutralPart::Text { text: "No renderable turn content".into() });
    }
    Ok(output)
}

fn render_parts(parts: &[ProviderNeutralPart]) -> String {
    parts.iter().map(render_part).collect::<Vec<_>>().join("\n")
}

fn render_part(part: &ProviderNeutralPart) -> String {
    match part {
        ProviderNeutralPart::Text { text } => text.clone(),
        ProviderNeutralPart::Data { schema_id, fields } => {
            format!("Data {}: {}", schema_id, serde_json::to_string(fields).unwrap_or_else(|_| "{}".into()))
        }
        ProviderNeutralPart::ToolCall { tool_name, arguments_json, .. } => format!("Tool {tool_name}: {arguments_json}"),
        ProviderNeutralPart::ToolResult { content, is_error, .. } => {
            format!("Tool result{}: {content}", if *is_error { " error" } else { "" })
        }
        ProviderNeutralPart::Artifact { artifact_id, display_name, .. } => format!("Artifact {display_name} ({artifact_id})"),
    }
}

fn append_block(target: &mut String, value: &str) {
    if !target.is_empty() {
        target.push_str("\n\n");
    }
    target.push_str(value);
}
