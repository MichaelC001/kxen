use serde_json::Value;

use super::context::AgentContext;

pub(super) fn mount(args: &Value, ctx: &AgentContext) -> Result<String, String> {
    let query = args.get("query").and_then(Value::as_str).ok_or("missing query")?;
    let Some(extras) = &ctx.extras else {
        return Err("tool_search unavailable in this context".into());
    };
    let matches = matching_tools(query, ctx.allowed_tools.as_deref());
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

fn matching_tools(query: &str, allowed: Option<&[String]>) -> Vec<&'static crate::llm::tool::ToolDefinition> {
    crate::agent::tools_deferred::deferred_tool_catalog()
        .iter()
        .filter(|tool| crate::agent::tools_deferred::deferred_tool_enabled(tool))
        .filter(|tool| super::helpers::tool_permitted(&tool.function.name, allowed))
        .filter(|tool| {
            query.split_whitespace().any(|word| {
                contains_ignore_ascii_case(&tool.function.name, word) || contains_ignore_ascii_case(&tool.function.description, word)
            })
        })
        .collect()
}

fn contains_ignore_ascii_case(text: &str, pattern: &str) -> bool {
    pattern.is_empty() || text.as_bytes().windows(pattern.len()).any(|window| window.eq_ignore_ascii_case(pattern.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_never_discloses_tools_outside_the_identity_allowlist() {
        let allowed = vec!["lsp".to_string()];
        let matches = matching_tools("lsp browser agent", Some(&allowed));
        assert_eq!(matches.iter().map(|tool| tool.function.name.as_str()).collect::<Vec<_>>(), ["lsp"]);
    }
}
