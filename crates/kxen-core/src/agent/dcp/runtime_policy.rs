use std::collections::BTreeSet;

use super::{DcpAgentDefinition, DcpAgentLock, DcpRuntimePolicy};

impl DcpRuntimePolicy {
    pub fn validate(&self) -> Result<(), String> {
        let mut seen = BTreeSet::new();
        for name in &self.pass_env {
            if name.is_empty()
                || !name
                    .bytes()
                    .enumerate()
                    .all(|(index, byte)| byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit()))
            {
                return Err(format!("invalid passEnv variable name: {name:?}"));
            }
            if !seen.insert(name) {
                return Err(format!("duplicate passEnv variable: {name}"));
            }
            if is_provider_credential_env(name) {
                return Err(format!("provider credential cannot be exposed to child processes: {name}"));
            }
        }
        Ok(())
    }

    pub fn content_hash(&self) -> Result<crate::core::identity::ContentHash, String> {
        self.validate()?;
        serde_json::to_vec(self).map(|bytes| crate::core::identity::ContentHash::from_bytes(&bytes)).map_err(|error| error.to_string())
    }

    pub fn permitted_catalog(&self, available: &BTreeSet<String>) -> BTreeSet<String> {
        let allowed = self.allowed_capabilities.as_ref().map(|items| items.iter().cloned().collect::<BTreeSet<_>>());
        let denied = self.denied_capabilities.iter().cloned().collect::<BTreeSet<_>>();
        available
            .iter()
            .filter(|name| {
                allowed.as_ref().is_none_or(|set| set.contains(*name))
                    && !denied.contains(*name)
                    && (self.allow_shell || !matches!(name.as_str(), "exec" | "task"))
                    && (self.allow_mcp || !name.starts_with("mcp__"))
                    && (self.allow_code_orchestration || name.as_str() != "workflow")
                    && (self.allow_dynamic_tools || !crate::agent::dynamic::is_dynamic_capability(name))
            })
            .cloned()
            .collect()
    }

    pub fn resolve_lock(&self, definition: DcpAgentDefinition, available: &BTreeSet<String>) -> Result<DcpAgentLock, String> {
        self.validate()?;
        definition.validate()?;
        let permitted_catalog = self.permitted_catalog(available);
        let mut effective = BTreeSet::new();
        for required in &definition.spec.capabilities.required {
            if !permitted_catalog.contains(required) {
                return Err(format!("required capability is unavailable or denied: {required}"));
            }
            effective.insert(required.clone());
        }
        for optional in &definition.spec.capabilities.optional {
            if permitted_catalog.contains(optional) {
                effective.insert(optional.clone());
            }
        }
        let mut definition = definition;
        if let Some(limit) = self.max_turns {
            definition.spec.execution.max_turns = definition.spec.execution.max_turns.min(limit);
        }
        definition.spec.execution.max_wall_clock_ms = minimum(definition.spec.execution.max_wall_clock_ms, self.max_wall_clock_ms);
        let definition_hash = definition.content_hash()?;
        let policy_hash = self.content_hash()?;
        Ok(DcpAgentLock {
            definition,
            definition_hash,
            effective_capabilities: effective.into_iter().collect(),
            policy_hash,
            created_at_ms: crate::core::shared::now_ms(),
        })
    }
}

impl DcpAgentLock {
    pub fn validate(&self) -> Result<(), String> {
        self.definition.validate()?;
        let actual_hash = self.definition.content_hash()?;
        if actual_hash != self.definition_hash {
            return Err(format!(
                "DCPAgent definition hash mismatch: expected {}, actual {}",
                self.definition_hash.as_str(),
                actual_hash.as_str()
            ));
        }
        let requested = self
            .definition
            .spec
            .capabilities
            .required
            .iter()
            .chain(&self.definition.spec.capabilities.optional)
            .cloned()
            .collect::<BTreeSet<_>>();
        let effective = self.effective_capabilities.iter().cloned().collect::<BTreeSet<_>>();
        if effective.len() != self.effective_capabilities.len()
            || effective.iter().cloned().collect::<Vec<_>>() != self.effective_capabilities
        {
            return Err("DCPAgent effective capabilities must be unique and canonically ordered".into());
        }
        if let Some(capability) = effective.iter().find(|capability| !requested.contains(*capability)) {
            return Err(format!("DCPAgent effective capability was not requested: {capability}"));
        }
        if let Some(capability) = self.definition.spec.capabilities.required.iter().find(|capability| !effective.contains(*capability)) {
            return Err(format!("DCPAgent required capability is missing from the lock: {capability}"));
        }
        Ok(())
    }
}

pub(crate) fn is_provider_credential_env(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "ANTHROPIC_API_KEY"
            | "OPENAI_API_KEY"
            | "XAI_API_KEY"
            | "GOOGLE_API_KEY"
            | "OPENROUTER_API_KEY"
            | "GROQ_API_KEY"
            | "MISTRAL_API_KEY"
            | "DEEPSEEK_API_KEY"
    )
}

fn minimum<T: Ord + Copy>(left: Option<T>, right: Option<T>) -> Option<T> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}
