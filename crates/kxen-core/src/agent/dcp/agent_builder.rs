use std::collections::BTreeSet;
use std::time::Duration;

use crate::llm::{Message, ModelRef};

use super::DcpAgentDefinition;

pub async fn build_agent_definition(
    task: &str,
    available_capabilities: &BTreeSet<String>,
    mrm: &crate::llm::mrm::ModelResourceManager,
    store: &crate::auth::credential::AuthStore,
    cancel: Option<&crate::agent::cancel::CancelToken>,
) -> Result<DcpAgentDefinition, String> {
    if task.trim().is_empty() {
        return Err("DCPAgent Builder task must not be empty".into());
    }
    let resolved = mrm.resolve("planning", store).await.ok_or("no available model for DCPAgent Builder role planning")?;
    let model = resolved.account.map_or_else(
        || ModelRef::new(&resolved.provider, &resolved.model),
        |account| ModelRef::with_account(&resolved.provider, &resolved.model, account),
    );
    let catalog = available_capabilities.iter().map(|name| format!("- {name}")).collect::<Vec<_>>().join("\n");
    let messages = vec![
        Message::system(format!(
            "You are the restricted DCPAgent Builder. Understand the task and create exactly one provider-neutral DCPAgent definition. Return only one JSON object matching the supplied schema, without markdown. You may request capabilities from the catalog, but you cannot grant permissions, select a provider/model, add credentials, execute tools, or introduce GitHub/GitLab/Issue/PR-specific core fields. Use modelRole only. Keep instructions generally useful for this task while making success criteria observable. Every requested capability must exactly match the catalog. apiVersion must be `kxen.ai/v1alpha1` and kind must be `DCPAgent`. JSON schema example: {{\"apiVersion\":\"kxen.ai/v1alpha1\",\"kind\":\"DCPAgent\",\"metadata\":{{\"name\":\"repository_fixer\",\"description\":\"...\"}},\"spec\":{{\"objective\":\"...\",\"instructions\":[\"...\"],\"successCriteria\":[\"...\"],\"capabilities\":{{\"required\":[\"read\"],\"optional\":[\"write\"]}},\"execution\":{{\"modelRole\":\"execution\",\"maxTurns\":32,\"maxPureRetries\":1}},\"output\":{{\"format\":\"text\",\"requiredFields\":[]}}}}}}.\n\nAvailable capabilities:\n{catalog}"
        )),
        Message::user(format!("Create the DCPAgent definition for this task:\n\n{task}")),
    ];
    let output = crate::llm::managed::collect_text(mrm, &model, &messages, store, Duration::from_secs(120), None, cancel).await?;
    let json = strip_json_fence(&output.text);
    let definition: DcpAgentDefinition =
        serde_json::from_str(json).map_err(|error| format!("DCPAgent Builder returned invalid JSON: {error}"))?;
    definition.validate()?;
    let requested = definition
        .spec
        .capabilities
        .required
        .iter()
        .chain(&definition.spec.capabilities.optional)
        .filter(|name| !available_capabilities.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    if !requested.is_empty() {
        return Err(format!("DCPAgent Builder requested unknown capabilities: {}", requested.join(", ")));
    }
    Ok(definition)
}

fn strip_json_fence(value: &str) -> &str {
    let value = value.trim();
    let value = value.strip_prefix("```json").or_else(|| value.strip_prefix("```")).unwrap_or(value);
    value.strip_suffix("```").unwrap_or(value).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_fence_is_removed() {
        assert_eq!(strip_json_fence("```json\n{}\n```"), "{}");
        assert_eq!(strip_json_fence("{}"), "{}");
    }
}
