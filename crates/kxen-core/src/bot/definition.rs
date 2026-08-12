use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Component, Path};

use crate::agent::capability::CapabilitySet;
use crate::agent::runtime::ExecutionBudget;
use crate::core::identity::{ContentHash, ResourceId};

use super::BotError;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSpec {
    pub description: String,
    pub content_type: String,
    pub required_fields: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAccess {
    Read,
    Write,
    Execute,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PathGrantSpec {
    pub relative_path: String,
    pub access: ResourceAccess,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceGrantSpec {
    pub workspace_id: ResourceId,
    pub paths: Vec<PathGrantSpec>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourcePolicy {
    pub workspaces: Vec<WorkspaceGrantSpec>,
    pub connectors: BTreeSet<ResourceId>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    ManualWhenRequired,
    AlwaysManual,
    DenyControlledEffects,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextPolicy {
    pub max_conversation_messages: u32,
    pub max_memory_items: u32,
    pub max_run_turns: u32,
}

impl Default for ContextPolicy {
    fn default() -> Self {
        Self { max_conversation_messages: 100, max_memory_items: 50, max_run_turns: 32 }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryPolicy {
    pub enabled: bool,
    pub max_items: u32,
    pub allow_sensitive: bool,
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self { enabled: true, max_items: 1000, allow_sensitive: false }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommunicationPolicy {
    pub allow_direct: bool,
    pub allow_groups: bool,
    pub allowed_peers: BTreeSet<ResourceId>,
}

impl Default for CommunicationPolicy {
    fn default() -> Self {
        Self { allow_direct: false, allow_groups: true, allowed_peers: BTreeSet::new() }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FailurePolicy {
    pub max_pure_retries: u8,
    pub auto_pause_after_failures: u8,
}

impl Default for FailurePolicy {
    fn default() -> Self {
        Self { max_pure_retries: 1, auto_pause_after_failures: 3 }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BotDefinition {
    pub display_name: String,
    pub description: String,
    pub objective: String,
    pub success_criteria: Vec<String>,
    pub instructions: String,
    pub input_contract: ContractSpec,
    pub output_contract: ContractSpec,
    pub mrm_role: ResourceId,
    pub capabilities: CapabilitySet,
    pub resources: ResourcePolicy,
    pub approval: ApprovalPolicy,
    pub budget: ExecutionBudget,
    pub context: ContextPolicy,
    pub memory: MemoryPolicy,
    pub communication: CommunicationPolicy,
    pub failure: FailurePolicy,
}

impl BotDefinition {
    pub fn empty(display_name: impl Into<String>) -> Self {
        Self {
            display_name: display_name.into(),
            description: String::new(),
            objective: String::new(),
            success_criteria: Vec::new(),
            instructions: String::new(),
            input_contract: ContractSpec { description: String::new(), content_type: "text/plain".into(), required_fields: Vec::new() },
            output_contract: ContractSpec { description: String::new(), content_type: "text/plain".into(), required_fields: Vec::new() },
            mrm_role: ResourceId::parse("execution").expect("built-in role id"),
            capabilities: CapabilitySet::default(),
            resources: ResourcePolicy::default(),
            approval: ApprovalPolicy::ManualWhenRequired,
            budget: ExecutionBudget::default(),
            context: ContextPolicy::default(),
            memory: MemoryPolicy::default(),
            communication: CommunicationPolicy::default(),
            failure: FailurePolicy::default(),
        }
    }

    pub fn content_hash(&self) -> Result<ContentHash, BotError> {
        serde_json::to_vec(self)
            .map(|bytes| ContentHash::from_bytes(&bytes))
            .map_err(|error| BotError::InvalidDefinition(error.to_string()))
    }

    pub fn validate_draft(&self) -> Result<(), BotError> {
        validate_text("display_name", &self.display_name, 1, 120)?;
        validate_text("description", &self.description, 0, 2000)?;
        validate_text("objective", &self.objective, 0, 8000)?;
        validate_text("instructions", &self.instructions, 0, 32_000)?;
        validate_contract(&self.input_contract)?;
        validate_contract(&self.output_contract)?;
        self.budget.validate().map_err(BotError::InvalidDefinition)?;
        if self.context.max_conversation_messages == 0 || self.context.max_run_turns == 0 {
            return Err(BotError::InvalidDefinition("context limits must be greater than zero".into()));
        }
        if self.memory.enabled && self.memory.max_items == 0 {
            return Err(BotError::InvalidDefinition("enabled memory requires max_items greater than zero".into()));
        }
        if self.memory.allow_sensitive {
            return Err(BotError::InvalidDefinition("Bot Memory cannot allow secrets or sensitive credentials".into()));
        }
        if self.resources.workspaces.len() > 1 {
            return Err(BotError::InvalidDefinition("Bot runtime supports exactly one active Workspace boundary".into()));
        }
        if !self.resources.connectors.is_empty() && self.resources.workspaces.is_empty() {
            return Err(BotError::InvalidDefinition(
                "connector grants require an explicit active Workspace binding, which may contain zero path grants".into(),
            ));
        }
        for workspace in &self.resources.workspaces {
            let mut paths = BTreeSet::new();
            for grant in &workspace.paths {
                validate_relative_path(&grant.relative_path)?;
                if !paths.insert((&grant.relative_path, grant.access)) {
                    return Err(BotError::InvalidDefinition(format!("duplicate path grant: {}", grant.relative_path)));
                }
            }
        }
        Ok(())
    }

    pub fn validate_publish(&self) -> Result<(), BotError> {
        self.validate_draft()?;
        validate_text("objective", &self.objective, 1, 8000)?;
        validate_text("instructions", &self.instructions, 1, 32_000)?;
        validate_text("output_contract.description", &self.output_contract.description, 1, 4000)?;
        if self.success_criteria.is_empty() || self.success_criteria.iter().any(|criterion| criterion.trim().is_empty()) {
            return Err(BotError::InvalidDefinition("at least one non-empty success criterion is required".into()));
        }
        Ok(())
    }

    pub fn validate_input(&self, parts: &[crate::agent::dcp::ProviderNeutralPart]) -> Result<(), BotError> {
        self.input_contract.validate_parts(parts, "input")
    }

    pub fn output_parts(&self, text: &str) -> Result<Vec<crate::agent::dcp::ProviderNeutralPart>, BotError> {
        self.output_contract.parts_from_text(text)
    }
}

impl ContractSpec {
    fn validate_parts(&self, parts: &[crate::agent::dcp::ProviderNeutralPart], field: &str) -> Result<(), BotError> {
        match self.content_type.as_str() {
            "text/plain"
                if parts
                    .iter()
                    .any(|part| matches!(part, crate::agent::dcp::ProviderNeutralPart::Text { text } if !text.trim().is_empty())) =>
            {
                Ok(())
            }
            "application/json" => {
                let fields = parts.iter().find_map(|part| match part {
                    crate::agent::dcp::ProviderNeutralPart::Data { fields, .. } => Some(fields),
                    _ => None,
                });
                let Some(fields) = fields else {
                    return Err(BotError::InvalidDefinition(format!("{field} contract requires a structured data part")));
                };
                let missing = self.required_fields.iter().filter(|required| !fields.contains_key(*required)).cloned().collect::<Vec<_>>();
                if missing.is_empty() {
                    Ok(())
                } else {
                    Err(BotError::InvalidDefinition(format!("{field} contract is missing fields: {}", missing.join(", "))))
                }
            }
            _ => Err(BotError::InvalidDefinition(format!("{field} does not satisfy {}", self.content_type))),
        }
    }

    fn parts_from_text(&self, text: &str) -> Result<Vec<crate::agent::dcp::ProviderNeutralPart>, BotError> {
        if self.content_type == "text/plain" {
            let parts = vec![crate::agent::dcp::ProviderNeutralPart::Text { text: text.into() }];
            self.validate_parts(&parts, "output")?;
            return Ok(parts);
        }
        let value = text.trim();
        let value = value.strip_prefix("```json").or_else(|| value.strip_prefix("```")).unwrap_or(value);
        let value = value.strip_suffix("```").unwrap_or(value).trim();
        let object = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(value)
            .map_err(|error| BotError::InvalidDefinition(format!("output contract requires a JSON object: {error}")))?;
        let fields = object
            .into_iter()
            .map(|(key, value)| {
                let value = value.as_str().map(ToOwned::to_owned).unwrap_or_else(|| value.to_string());
                (key, value)
            })
            .collect();
        let parts = vec![crate::agent::dcp::ProviderNeutralPart::Data {
            schema_id: ResourceId::parse("bot_contract_output").expect("built-in contract schema id"),
            fields,
        }];
        self.validate_parts(&parts, "output")?;
        Ok(parts)
    }
}

fn validate_text(field: &str, value: &str, min: usize, max: usize) -> Result<(), BotError> {
    let length = value.trim().chars().count();
    if length < min || length > max {
        return Err(BotError::InvalidDefinition(format!("{field} length must be between {min} and {max}")));
    }
    Ok(())
}

fn validate_contract(contract: &ContractSpec) -> Result<(), BotError> {
    validate_text("contract.description", &contract.description, 0, 4000)?;
    validate_text("contract.content_type", &contract.content_type, 1, 120)?;
    let mut fields = BTreeSet::new();
    for field in &contract.required_fields {
        crate::core::ids::validate_id(field).map_err(BotError::InvalidDefinition)?;
        if !fields.insert(field) {
            return Err(BotError::InvalidDefinition(format!("duplicate contract field: {field}")));
        }
    }
    match contract.content_type.as_str() {
        "text/plain" if contract.required_fields.is_empty() => {}
        "text/plain" => return Err(BotError::InvalidDefinition("text/plain contract cannot declare structured required_fields".into())),
        "application/json" => {}
        other => return Err(BotError::InvalidDefinition(format!("unsupported Bot contract content_type: {other}"))),
    }
    Ok(())
}

fn validate_relative_path(value: &str) -> Result<(), BotError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        return Err(BotError::InvalidDefinition(format!("resource path must be a safe relative path: {value}")));
    }
    Ok(())
}
