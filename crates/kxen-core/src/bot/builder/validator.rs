use std::collections::BTreeSet;

use crate::agent::capability::CapabilityCatalog;
use crate::core::identity::{ContentHash, ResourceId};

use super::types::{PermissionGrant, TestEvidence, ValidationFinding, ValidationReport, ValidationStatus};
use crate::bot::BotDefinition;

pub struct ValidationContext<'a> {
    pub catalog: &'a CapabilityCatalog,
    pub mrm_roles: &'a BTreeSet<ResourceId>,
    pub connectors: &'a BTreeSet<ResourceId>,
    pub grant: Option<&'a PermissionGrant>,
    pub tests: &'a [TestEvidence],
}

pub fn validate(
    report_id: ResourceId,
    definition: &BotDefinition,
    context: ValidationContext<'_>,
    created_at_ms: u64,
) -> Result<ValidationReport, String> {
    let draft_hash = definition.content_hash().map_err(|error| error.to_string())?;
    let mut findings = Vec::new();
    findings.push(match definition.validate_publish() {
        Ok(()) => finding(
            "definition_schema",
            ValidationStatus::Pass,
            "Definition schema and required behavior are valid",
            Some(draft_hash.as_str()),
        ),
        Err(error) => finding("definition_schema", ValidationStatus::Fail, &error.to_string(), None),
    });
    findings.push(if context.mrm_roles.contains(&definition.mrm_role) {
        finding("mrm_role", ValidationStatus::Pass, "MRM role is available", Some(definition.mrm_role.as_str()))
    } else {
        finding("mrm_role", ValidationStatus::Fail, "MRM role is unavailable", Some(definition.mrm_role.as_str()))
    });
    findings.push(match context.catalog.resolve(&definition.capabilities) {
        Ok(resolved) => finding(
            "capabilities",
            ValidationStatus::Pass,
            "Every requested capability is available",
            Some(&resolved.iter().map(|item| item.id.as_str()).collect::<Vec<_>>().join(",")),
        ),
        Err(error) => finding("capabilities", ValidationStatus::Fail, &error.to_string(), None),
    });
    let unavailable_connectors =
        definition.resources.connectors.difference(context.connectors).map(ToString::to_string).collect::<Vec<_>>();
    findings.push(if unavailable_connectors.is_empty() {
        finding(
            "connectors",
            ValidationStatus::Pass,
            "Every requested connector is configured in the active Workspace",
            Some(&definition.resources.connectors.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")),
        )
    } else {
        finding(
            "connectors",
            ValidationStatus::Fail,
            &format!("Unavailable Workspace connectors: {}", unavailable_connectors.join(", ")),
            None,
        )
    });
    let permission_hash = permission_hash(definition)?;
    findings.push(match context.grant {
        Some(grant) if grant.draft_hash == draft_hash && grant.permission_hash == permission_hash => finding(
            "permission_grant",
            ValidationStatus::Pass,
            "Owner grant matches the exact draft permission surface",
            Some(grant.grant_id.as_str()),
        ),
        Some(_) => finding("permission_grant", ValidationStatus::Fail, "Permission grant is stale for this draft", None),
        None => finding("permission_grant", ValidationStatus::Fail, "Owner permission grant is required", None),
    });
    let evidence = context.tests.iter().rev().find(|item| item.draft_hash == draft_hash);
    findings.push(match evidence {
        Some(evidence)
            if evidence.passed && definition.success_criteria.iter().all(|criterion| evidence.criteria.get(criterion) == Some(&true)) =>
        {
            finding("test_evidence", ValidationStatus::Pass, "Test evidence covers every success criterion", Some(evidence.run_id.as_str()))
        }
        Some(evidence) => finding("test_evidence", ValidationStatus::Fail, &evidence.summary, Some(evidence.run_id.as_str())),
        None => {
            finding("test_evidence", ValidationStatus::Unknown, "A test run is required before publish eligibility can be proven", None)
        }
    });
    let publish_eligible = findings.iter().all(|finding| finding.status == ValidationStatus::Pass);
    Ok(ValidationReport { report_id, draft_hash, findings, publish_eligible, created_at_ms })
}

pub fn permission_hash(definition: &BotDefinition) -> Result<ContentHash, String> {
    #[derive(serde::Serialize)]
    struct Surface<'a> {
        capabilities: &'a crate::agent::capability::CapabilitySet,
        resources: &'a crate::bot::ResourcePolicy,
        approval: crate::bot::ApprovalPolicy,
        communication: &'a crate::bot::CommunicationPolicy,
        memory: &'a crate::bot::MemoryPolicy,
    }
    serde_json::to_vec(&Surface {
        capabilities: &definition.capabilities,
        resources: &definition.resources,
        approval: definition.approval,
        communication: &definition.communication,
        memory: &definition.memory,
    })
    .map(|bytes| ContentHash::from_bytes(&bytes))
    .map_err(|error| error.to_string())
}

fn finding(code: &str, status: ValidationStatus, message: &str, evidence: Option<&str>) -> ValidationFinding {
    ValidationFinding { code: code.into(), status, message: message.into(), evidence: evidence.map(ToOwned::to_owned) }
}
