use super::*;
use crate::agent::dcp::{
    DCP_AGENT_API_VERSION, DcpAgentCapabilities, DcpAgentDefinition, DcpAgentExecution, DcpAgentMetadata, DcpAgentOutput, DcpAgentSpec,
    DcpRuntimePolicy,
};

fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("kxen-dcp-store-{tag}-{}", uuid::Uuid::new_v4()))
}

fn lock() -> DcpAgentLock {
    let definition = DcpAgentDefinition {
        api_version: DCP_AGENT_API_VERSION.into(),
        kind: "DCPAgent".into(),
        metadata: DcpAgentMetadata { name: "test_agent".into(), description: None },
        spec: DcpAgentSpec {
            objective: "Test".into(),
            instructions: vec!["Return".into()],
            success_criteria: Vec::new(),
            capabilities: DcpAgentCapabilities { required: vec!["read".into()], optional: Vec::new() },
            execution: DcpAgentExecution::default(),
            output: DcpAgentOutput::default(),
        },
    };
    DcpRuntimePolicy::default().resolve_lock(definition, &["read".into()].into_iter().collect()).unwrap()
}

#[test]
fn session_run_and_bundle_roundtrip() {
    let source_root = temp_dir("workspace");
    let sessions = temp_dir("sessions");
    std::fs::create_dir_all(&source_root).unwrap();
    let store = DcpStore::new(&sessions);
    let mut session = store.create_session(WorkspaceBinding::capture(&source_root).unwrap(), lock()).unwrap();
    let run = store.create_run(&mut session, "do it".into()).unwrap();
    assert_eq!(store.load_run(&session.session_id, &run.run_id).unwrap().input, "do it");
    let bundle = store.export_bundle(&session.session_id).unwrap();

    let imported_sessions = temp_dir("imported");
    let imported_workspace = temp_dir("imported-workspace");
    std::fs::create_dir_all(&imported_workspace).unwrap();
    let imported = DcpStore::new(&imported_sessions).import_bundle(bundle, &imported_workspace).unwrap();
    assert_eq!(imported.session_id, session.session_id);
    assert_eq!(imported.workspace.root, std::fs::canonicalize(&imported_workspace).unwrap().to_string_lossy());
    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(sessions).ok();
    std::fs::remove_dir_all(imported_sessions).ok();
    std::fs::remove_dir_all(imported_workspace).ok();
}

#[test]
fn tampered_agent_lock_is_rejected_on_load_and_import() {
    let source_root = temp_dir("tampered-workspace");
    let sessions = temp_dir("tampered-sessions");
    std::fs::create_dir_all(&source_root).unwrap();
    let store = DcpStore::new(&sessions);
    let session = store.create_session(WorkspaceBinding::capture(&source_root).unwrap(), lock()).unwrap();
    let mut bundle = store.export_bundle(&session.session_id).unwrap();
    bundle.dcp.agent.definition.spec.objective = "Tampered objective".into();

    let imported_sessions = temp_dir("tampered-imported");
    let imported_workspace = temp_dir("tampered-imported-workspace");
    std::fs::create_dir_all(&imported_workspace).unwrap();
    let error = DcpStore::new(&imported_sessions).import_bundle(bundle, &imported_workspace).unwrap_err();
    assert!(error.contains("definition hash mismatch"), "{error}");
    assert!(!imported_sessions.join(format!("{}.json", session.session_id)).exists());

    let state_path = store.session_state_path(&session.session_id);
    let mut state: DcpSessionState = serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
    state.agent.definition.spec.objective = "Locally tampered objective".into();
    std::fs::write(&state_path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();
    assert!(store.load_session(&session.session_id).unwrap_err().contains("definition hash mismatch"));

    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(sessions).ok();
    std::fs::remove_dir_all(imported_sessions).ok();
    std::fs::remove_dir_all(imported_workspace).ok();
}

#[test]
fn bundle_cross_references_and_tool_schema_are_validated_before_import() {
    let source_root = temp_dir("invalid-bundle-workspace");
    let sessions = temp_dir("invalid-bundle-sessions");
    std::fs::create_dir_all(&source_root).unwrap();
    let store = DcpStore::new(&sessions);
    let mut session = store.create_session(WorkspaceBinding::capture(&source_root).unwrap(), lock()).unwrap();
    let run = store.create_run(&mut session, "do it".into()).unwrap();
    let bundle = store.export_bundle(&session.session_id).unwrap();
    let imported_workspace = temp_dir("invalid-bundle-imported-workspace");
    std::fs::create_dir_all(&imported_workspace).unwrap();

    let mut wrong_revision = bundle.clone();
    wrong_revision.runs.get_mut(&run.run_id).unwrap().state.agent_definition_hash =
        crate::core::identity::ContentHash::from_bytes(b"wrong");
    let wrong_revision_sessions = temp_dir("wrong-revision");
    let error = DcpStore::new(&wrong_revision_sessions).import_bundle(wrong_revision, &imported_workspace).unwrap_err();
    assert!(error.contains("different DCPAgent revision"), "{error}");

    let mut wrong_tools = bundle;
    wrong_tools.runs.get_mut(&run.run_id).unwrap().tools.schema_version = 99;
    let wrong_tools_sessions = temp_dir("wrong-tools");
    let error = DcpStore::new(&wrong_tools_sessions).import_bundle(wrong_tools, &imported_workspace).unwrap_err();
    assert!(error.contains("unsupported DCP tool journal schema 99"), "{error}");

    std::fs::remove_dir_all(source_root).ok();
    std::fs::remove_dir_all(sessions).ok();
    std::fs::remove_dir_all(imported_workspace).ok();
    std::fs::remove_dir_all(wrong_revision_sessions).ok();
    std::fs::remove_dir_all(wrong_tools_sessions).ok();
}

#[test]
fn second_session_run_lease_is_rejected() {
    let sessions = temp_dir("lease");
    let first = SessionRunLease::try_acquire(&sessions, "ses_one").unwrap();
    assert!(SessionRunLease::try_acquire(&sessions, "ses_one").is_err());
    drop(first);
    assert!(SessionRunLease::try_acquire(&sessions, "ses_one").is_ok());
    std::fs::remove_dir_all(sessions).ok();
}
