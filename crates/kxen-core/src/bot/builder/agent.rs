//! Restricted natural-language Builder. It has no execution tools and can only
//! return a typed BotDefinition draft for deterministic validation.

use std::time::Duration;

use crate::bot::BotDefinition;
use crate::llm::{Message, ModelRef};

pub async fn generate_draft(
    mrm: &crate::llm::mrm::ModelResourceManager,
    store: &crate::auth::credential::AuthStore,
    user_goal: &str,
    conversation: &[super::BuilderMessage],
    current: Option<&BotDefinition>,
    capability_catalog: &crate::agent::capability::CapabilityCatalog,
) -> Result<BotDefinition, String> {
    let resolved = mrm
        .resolve(super::BUILDER_MRM_ROLE, store)
        .await
        .ok_or_else(|| format!("no available model for MRM role {}", super::BUILDER_MRM_ROLE))?;
    let model = resolved.account.map_or_else(
        || ModelRef::new(&resolved.provider, &resolved.model),
        |account| ModelRef::with_account(&resolved.provider, &resolved.model, account),
    );
    let schema_example = serde_json::to_string_pretty(&current.cloned().unwrap_or_else(|| BotDefinition::empty("New Bot")))
        .map_err(|error| error.to_string())?;
    let history = conversation.iter().map(|message| format!("{:?}: {}", message.actor, message.text)).collect::<Vec<_>>().join("\n");
    let capabilities = capability_catalog
        .descriptors()
        .filter(|descriptor| descriptor.availability == crate::agent::capability::CapabilityAvailability::Available)
        .map(|descriptor| {
            format!(
                "{}: {}{}",
                descriptor.id,
                descriptor.display_name,
                if descriptor.requires_approval { " (Owner approval may be required)" } else { "" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let messages = vec![
        Message::system(format!(
            "You are kxen Bot Builder. Return exactly one JSON object matching the BotDefinition shape below, with no markdown. Preserve explicit permission, resource, budget, memory and communication constraints. Never add a capability, Workspace path, peer, connector, approval grant, Routine, Group or publication action that the user did not explicitly request. Provider/model values are forbidden; select only an MRM role. Shared cloud computers, Marketplace, human multi-user chat and ACL are outside scope.\n\nAvailable runtime capabilities, selectable only when explicitly needed:\n{capabilities}\n\nShape and current defaults:\n{schema_example}"
        )),
        Message::user(format!("Builder goal:\n{user_goal}\n\nConversation:\n{history}\n\nGenerate the complete current draft.")),
    ];
    let output = crate::llm::managed::collect_text(mrm, &model, &messages, store, Duration::from_secs(120), None, None).await?;
    let json = strip_json_fence(&output.text);
    let definition: BotDefinition =
        serde_json::from_str(json).map_err(|error| format!("Builder returned invalid BotDefinition JSON: {error}"))?;
    definition.validate_draft().map_err(|error| error.to_string())?;
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
    fn json_fence_is_removed_without_touching_plain_json() {
        assert_eq!(strip_json_fence("```json\n{}\n```"), "{}");
        assert_eq!(strip_json_fence("{}"), "{}");
    }
}
