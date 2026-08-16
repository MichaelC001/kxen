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

#[test]
fn workspace_binding_verifies_identity_and_directory_type() {
    let root = std::env::temp_dir().join(format!("kxen-dcp-binding-{}", uuid::Uuid::new_v4()));
    let first = root.join("first");
    let second = root.join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let binding = WorkspaceBinding::capture(&first).unwrap();
    assert_eq!(binding.verify(&first, false).unwrap(), binding);
    assert!(binding.verify(&second, true).unwrap_err().contains("identity mismatch"));

    let file = root.join("file");
    std::fs::write(&file, "not a directory").unwrap();
    assert!(WorkspaceBinding::capture(&file).unwrap_err().contains("not a directory"));
    assert_eq!(sanitize_remote("ssh://git@example.com/o/r.git"), "ssh://example.com/o/r.git");
    assert_eq!(sanitize_remote("git@example.com:o/r.git"), "git@example.com:o/r.git");

    std::fs::remove_file(file).unwrap();
    std::fs::remove_dir(first).unwrap();
    std::fs::remove_dir(second).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn workflow_capability_is_gated_by_code_orchestration_policy() {
    let available = BTreeSet::from(["read".into(), "workflow".into()]);
    let mut def = definition();
    def.spec.capabilities = DcpAgentCapabilities { required: vec!["read".into()], optional: vec!["workflow".into()] };

    // 默认关闭（与 allow_shell/allow_mcp 同构的闭集特例）：workflow 出集，optional 申请被静默裁掉
    let policy = DcpRuntimePolicy::default();
    assert!(!policy.allow_code_orchestration);
    assert!(!policy.permitted_catalog(&available).contains("workflow"));
    let lock = policy.resolve_lock(def.clone(), &available).unwrap();
    assert_eq!(lock.effective_capabilities, ["read"]);
    // required 申请 workflow 在关闭时直接锁定失败
    let mut def_required = definition();
    def_required.spec.capabilities = DcpAgentCapabilities { required: vec!["workflow".into()], optional: vec![] };
    let error = policy.resolve_lock(def_required.clone(), &available).unwrap_err();
    assert!(error.contains("required capability"), "{error}");

    // 开启：workflow 入集并可入锁
    let policy = DcpRuntimePolicy { allow_code_orchestration: true, ..Default::default() };
    assert!(policy.permitted_catalog(&available).contains("workflow"));
    let lock = policy.resolve_lock(def, &available).unwrap();
    assert_eq!(lock.effective_capabilities, ["read", "workflow"]);
    let lock = policy.resolve_lock(def_required, &available).unwrap();
    assert_eq!(lock.effective_capabilities, ["workflow"]);
}

#[test]
fn code_orchestration_flag_roundtrips_in_policy_json() {
    // 旧 policy 文件无该字段：serde default false（向后兼容）；显式开启可解析
    let policy: DcpRuntimePolicy = serde_json::from_str("{}").unwrap();
    assert!(!policy.allow_code_orchestration);
    let policy: DcpRuntimePolicy = serde_json::from_str(r#"{"allowCodeOrchestration": true}"#).unwrap();
    assert!(policy.allow_code_orchestration);
}
