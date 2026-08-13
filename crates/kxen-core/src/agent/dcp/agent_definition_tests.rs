use std::collections::BTreeSet;

use super::workspace_binding::sanitize_remote;
use super::*;

fn definition() -> DcpAgentDefinition {
    DcpAgentDefinition {
        api_version: DCP_AGENT_API_VERSION.into(),
        kind: "DCPAgent".into(),
        metadata: DcpAgentMetadata { name: "fixer".into(), description: None },
        spec: DcpAgentSpec {
            objective: "Fix the requested repository problem".into(),
            instructions: vec!["Inspect before editing".into()],
            success_criteria: vec!["Relevant checks pass".into()],
            capabilities: DcpAgentCapabilities { required: vec!["read".into()], optional: vec!["write".into()] },
            execution: DcpAgentExecution::default(),
            output: DcpAgentOutput::default(),
        },
    }
}

#[test]
fn yaml_roundtrip_preserves_definition() {
    let expected = definition();
    let yaml = expected.to_yaml().unwrap();
    assert_eq!(DcpAgentDefinition::parse_yaml(&yaml).unwrap(), expected);
}

#[test]
fn policy_cannot_grant_an_unavailable_required_capability() {
    let error = DcpRuntimePolicy::default().resolve_lock(definition(), &BTreeSet::new()).unwrap_err();
    assert!(error.contains("required capability"));
}

#[test]
fn optional_capability_is_intersected_with_runtime_policy() {
    let available = BTreeSet::from(["read".into(), "write".into()]);
    let policy = DcpRuntimePolicy { denied_capabilities: vec!["write".into()], ..Default::default() };
    let lock = policy.resolve_lock(definition(), &available).unwrap();
    assert_eq!(lock.effective_capabilities, ["read"]);
}

#[test]
fn remote_credentials_are_not_part_of_workspace_identity() {
    assert_eq!(sanitize_remote("https://token@example.com/o/r.git"), "https://example.com/o/r.git");
}

#[test]
fn provider_credentials_cannot_be_passed_to_tools() {
    let policy = DcpRuntimePolicy { pass_env: vec!["OPENAI_API_KEY".into()], ..Default::default() };
    assert!(policy.validate().unwrap_err().contains("provider credential"));
}
