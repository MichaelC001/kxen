use serde_json::Value;

use super::context::AgentContext;

pub(super) fn mount(args: &Value, ctx: &AgentContext) -> Result<String, String> {
    let query = args.get("query").and_then(Value::as_str).ok_or("missing query")?;
    let Some(extras) = &ctx.extras else {
        return Err("tool_search unavailable in this context".into());
    };
    let matches: Vec<_> = crate::agent::tools_deferred::deferred_tool_catalog()
        .iter()
        .filter(|tool| crate::agent::tools_deferred::deferred_tool_enabled(tool))
        .filter(|tool| {
            query.split_whitespace().any(|word| {
                contains_ignore_ascii_case(&tool.function.name, word) || contains_ignore_ascii_case(&tool.function.description, word)
            })
        })
        .collect();
    if matches.is_empty() {
        return Ok("no deferred tools match the query".into());
    }
    let mut enabled = crate::core::shared::lock(&extras.extra_tools);
    let mut names = String::new();
    for tool in &matches {
        enabled.insert(tool.function.name.clone());
        if !names.is_empty() {
            names.push_str(", ");
        }
        names.push_str(&tool.function.name);
    }
    Ok(format!("mounted for this session: {names}\n{}", serde_json::to_string_pretty(&matches).unwrap_or_default()))
}

fn contains_ignore_ascii_case(text: &str, pattern: &str) -> bool {
    pattern.is_empty() || text.as_bytes().windows(pattern.len()).any(|window| window.eq_ignore_ascii_case(pattern.as_bytes()))
}
