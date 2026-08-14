use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{DcpAgentLock, DcpRunState, DcpSessionState, DcpToolJournalSnapshot, WorkspaceBinding};

const DCP_SESSION_SCHEMA: u32 = 1;
const DCP_BUNDLE_SCHEMA: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DcpRunBundle {
    pub state: DcpRunState,
    pub tools: DcpToolJournalSnapshot,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DcpSessionBundle {
    pub schema_version: u32,
    pub session: crate::core::session::Session,
    pub messages: Vec<crate::core::session::Message>,
    pub dcp: DcpSessionState,
    pub runs: BTreeMap<String, DcpRunBundle>,
}

#[derive(Clone, Debug)]
pub struct DcpStore {
    sessions_dir: PathBuf,
}

pub struct SessionRunLease {
    _file: std::fs::File,
}

impl Drop for SessionRunLease {
    fn drop(&mut self) {
        // Release before the descriptor is closed. This is explicit because
        // lock lifetime semantics differ across supported Unix platforms and
        // the next process must be able to resume immediately.
        let _ = self._file.unlock();
    }
}

impl SessionRunLease {
    pub fn try_acquire(sessions_dir: &Path, session_id: &str) -> Result<Self, String> {
        crate::core::ids::validate_id(session_id)?;
        let root = sessions_dir.join(".run-locks");
        std::fs::create_dir_all(&root).map_err(|error| format!("create Session run lock directory {}: {error}", root.display()))?;
        let path = root.join(format!("{session_id}.lock"));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| format!("open Session run lock {}: {error}", path.display()))?;
        file.try_lock().map_err(|error| format!("Session already has an active run: {session_id}: {error}"))?;
        Ok(Self { _file: file })
    }
}

impl DcpStore {
    pub fn new(sessions_dir: impl Into<PathBuf>) -> Self {
        Self { sessions_dir: sessions_dir.into() }
    }

    pub fn sessions_dir(&self) -> &Path {
        &self.sessions_dir
    }

    pub fn create_session(&self, workspace: WorkspaceBinding, agent: DcpAgentLock) -> Result<DcpSessionState, String> {
        agent.validate()?;
        let session = crate::core::session::create(&self.sessions_dir, &workspace.root).map_err(|error| error.to_string())?;
        let now = crate::core::shared::now_ms();
        let state = DcpSessionState {
            schema_version: DCP_SESSION_SCHEMA,
            session_id: session.id,
            agent,
            workspace,
            run_ids: Vec::new(),
            forked_from_session_id: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.save_session(&state)?;
        Ok(state)
    }

    pub fn load_session(&self, session_id: &str) -> Result<DcpSessionState, String> {
        crate::core::ids::validate_id(session_id)?;
        let path = self.session_state_path(session_id);
        let bytes = std::fs::read(&path).map_err(|error| format!("read DCP Session {}: {error}", path.display()))?;
        let state: DcpSessionState =
            serde_json::from_slice(&bytes).map_err(|error| format!("parse DCP Session {}: {error}", path.display()))?;
        validate_session_state(&state, session_id)?;
        Ok(state)
    }

    pub fn save_session(&self, state: &DcpSessionState) -> Result<(), String> {
        validate_session_state(state, &state.session_id)?;
        write_json(&self.session_state_path(&state.session_id), state)
    }

    pub fn create_run(&self, session: &mut DcpSessionState, input: String) -> Result<DcpRunState, String> {
        if input.trim().is_empty() {
            return Err("DCPRun input must not be empty".into());
        }
        let run_id = crate::core::ids::new_id("run");
        let now = crate::core::shared::now_ms();
        let run = DcpRunState {
            run_id: run_id.clone(),
            session_id: session.session_id.clone(),
            agent_definition_hash: session.agent.definition_hash.clone(),
            status: super::DcpRunStatus::Queued,
            input,
            input_message_id: format!("{run_id}_input"),
            model: None,
            turns: 0,
            final_text: String::new(),
            error: None,
            settled: false,
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.save_run(&run)?;
        session.run_ids.push(run_id);
        session.updated_at_ms = now;
        self.save_session(session)?;
        Ok(run)
    }

    pub fn load_run(&self, session_id: &str, run_id: &str) -> Result<DcpRunState, String> {
        crate::core::ids::validate_id(session_id)?;
        crate::core::ids::validate_id(run_id)?;
        let path = self.run_state_path(session_id, run_id);
        let bytes = std::fs::read(&path).map_err(|error| format!("read DCPRun {}: {error}", path.display()))?;
        let run: DcpRunState = serde_json::from_slice(&bytes).map_err(|error| format!("parse DCPRun {}: {error}", path.display()))?;
        validate_run_state(&run, session_id, run_id, None)?;
        Ok(run)
    }

    pub fn save_run(&self, run: &DcpRunState) -> Result<(), String> {
        validate_run_state(run, &run.session_id, &run.run_id, None)?;
        write_json(&self.run_state_path(&run.session_id, &run.run_id), run)
    }

    pub fn run_dir(&self, session_id: &str, run_id: &str) -> Result<PathBuf, String> {
        crate::core::ids::validate_id(session_id)?;
        crate::core::ids::validate_id(run_id)?;
        Ok(self.dcp_dir(session_id).join("runs").join(run_id))
    }

    pub fn acquire_session(&self, session_id: &str) -> Result<SessionRunLease, String> {
        SessionRunLease::try_acquire(&self.sessions_dir, session_id)
    }

    pub fn fork_session(
        &self,
        session_id: &str,
        message_id: &str,
        position: crate::core::session::ForkPosition,
        kind: crate::core::session::ForkKind,
        workspace: Option<WorkspaceBinding>,
    ) -> Result<DcpSessionState, String> {
        let _lease = self.acquire_session(session_id)?;
        let source = self.load_session(session_id)?;
        let fork = crate::core::session::fork_with_options(&self.sessions_dir, session_id, message_id, position, kind)
            .map_err(|error| error.to_string())?;
        let now = crate::core::shared::now_ms();
        let selected_workspace = workspace.unwrap_or(source.workspace);
        let mut fork = fork;
        if fork.directory != selected_workspace.root {
            fork.directory = selected_workspace.root.clone();
            crate::core::session::save_meta(&self.sessions_dir, &fork).map_err(|error| error.to_string())?;
        }
        let state = DcpSessionState {
            schema_version: DCP_SESSION_SCHEMA,
            session_id: fork.id,
            agent: source.agent,
            workspace: selected_workspace,
            run_ids: Vec::new(),
            forked_from_session_id: Some(session_id.into()),
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.save_session(&state)?;
        Ok(state)
    }

    pub fn export_bundle(&self, session_id: &str) -> Result<DcpSessionBundle, String> {
        let _lease = self.acquire_session(session_id)?;
        let session = crate::core::session::load_meta(&self.sessions_dir, session_id).map_err(|error| error.to_string())?;
        let messages = crate::core::session::load_history_checked(&self.sessions_dir, session_id).map_err(|error| error.to_string())?;
        let dcp = self.load_session(session_id)?;
        let mut runs = BTreeMap::new();
        for run_id in &dcp.run_ids {
            let state = self.load_run(session_id, run_id)?;
            let tools_path = self.run_dir(session_id, run_id)?.join("tools.json");
            let tools = match std::fs::read(&tools_path) {
                Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| format!("parse {}: {error}", tools_path.display()))?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => DcpToolJournalSnapshot::default(),
                Err(error) => return Err(format!("read {}: {error}", tools_path.display())),
            };
            runs.insert(run_id.clone(), DcpRunBundle { state, tools });
        }
        Ok(DcpSessionBundle { schema_version: DCP_BUNDLE_SCHEMA, session, messages, dcp, runs })
    }

    pub fn import_bundle(&self, mut bundle: DcpSessionBundle, workspace: &Path) -> Result<DcpSessionState, String> {
        validate_bundle(&bundle)?;
        let session_id = bundle.session.id.clone();
        let _lease = self.acquire_session(&session_id)?;
        if self.sessions_dir.join(format!("{session_id}.json")).exists() {
            return Err(format!("Session already exists: {session_id}"));
        }
        let workspace = WorkspaceBinding::capture(workspace)?;
        bundle.session.directory = workspace.root.clone();
        bundle.dcp.workspace = workspace;
        bundle.dcp.updated_at_ms = crate::core::shared::now_ms();
        for (run_id, run) in &bundle.runs {
            self.save_run(&run.state)?;
            write_json(&self.run_dir(&session_id, run_id)?.join("tools.json"), &run.tools)?;
        }
        self.save_session(&bundle.dcp)?;
        let mut jsonl = Vec::new();
        for message in &bundle.messages {
            serde_json::to_writer(&mut jsonl, message).map_err(|error| error.to_string())?;
            jsonl.push(b'\n');
        }
        std::fs::create_dir_all(&self.sessions_dir).map_err(|error| error.to_string())?;
        crate::core::durability::atomic_replace(&self.sessions_dir.join(format!("{session_id}.jsonl")), &jsonl)
            .map_err(|error| error.to_string())?;
        let meta = serde_json::to_vec_pretty(&bundle.session).map_err(|error| error.to_string())?;
        crate::core::durability::atomic_replace(&self.sessions_dir.join(format!("{session_id}.json")), &meta)
            .map_err(|error| error.to_string())?;
        Ok(bundle.dcp)
    }

    pub fn list_sessions(&self) -> Vec<DcpSessionState> {
        crate::core::session::list(&self.sessions_dir).into_iter().filter_map(|session| self.load_session(&session.id).ok()).collect()
    }

    fn dcp_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_dir.join(session_id).join("dcp")
    }

    fn session_state_path(&self, session_id: &str) -> PathBuf {
        self.dcp_dir(session_id).join("session.json")
    }

    fn run_state_path(&self, session_id: &str, run_id: &str) -> PathBuf {
        self.dcp_dir(session_id).join("runs").join(run_id).join("run.json")
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    crate::core::durability::atomic_replace(path, &bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

fn validate_session_state(state: &DcpSessionState, expected_session_id: &str) -> Result<(), String> {
    crate::core::ids::validate_id(expected_session_id)?;
    if state.schema_version != DCP_SESSION_SCHEMA || state.session_id != expected_session_id {
        return Err(format!("DCP Session identity or schema mismatch: {expected_session_id}"));
    }
    state.agent.validate()?;
    if let Some(source) = &state.forked_from_session_id {
        crate::core::ids::validate_id(source)?;
    }
    let mut run_ids = std::collections::BTreeSet::new();
    for run_id in &state.run_ids {
        crate::core::ids::validate_id(run_id)?;
        if !run_ids.insert(run_id) {
            return Err(format!("duplicate DCPRun id in Session: {run_id}"));
        }
    }
    Ok(())
}

fn validate_run_state(
    run: &DcpRunState,
    expected_session_id: &str,
    expected_run_id: &str,
    expected_definition_hash: Option<&crate::core::identity::ContentHash>,
) -> Result<(), String> {
    crate::core::ids::validate_id(expected_session_id)?;
    crate::core::ids::validate_id(expected_run_id)?;
    if run.session_id != expected_session_id || run.run_id != expected_run_id {
        return Err(format!("DCPRun identity mismatch: {expected_run_id}"));
    }
    if run.input.trim().is_empty() || run.input_message_id.trim().is_empty() {
        return Err(format!("DCPRun {expected_run_id} has an invalid input contract"));
    }
    if run.settled && !run.status.is_terminal() {
        return Err(format!("non-terminal DCPRun {expected_run_id} cannot be settled"));
    }
    if expected_definition_hash.is_some_and(|hash| hash != &run.agent_definition_hash) {
        return Err(format!("DCPRun {expected_run_id} is bound to a different DCPAgent revision"));
    }
    Ok(())
}

fn validate_bundle(bundle: &DcpSessionBundle) -> Result<(), String> {
    if bundle.schema_version != DCP_BUNDLE_SCHEMA {
        return Err(format!("unsupported DCP Session bundle schema {}", bundle.schema_version));
    }
    let session_id = bundle.session.id.as_str();
    validate_session_state(&bundle.dcp, session_id)?;
    if bundle.messages.iter().any(|message| message.session_id != session_id) {
        return Err("DCP Session bundle message identity mismatch".into());
    }
    let mut message_ids = std::collections::BTreeSet::new();
    for message in &bundle.messages {
        if message.id.trim().is_empty() || !message_ids.insert(message.id.as_str()) {
            return Err(format!("duplicate or empty DCP Session message id: {:?}", message.id));
        }
    }
    let run_ids = bundle.dcp.run_ids.iter().cloned().collect::<std::collections::BTreeSet<_>>();
    let bundled_run_ids = bundle.runs.keys().cloned().collect::<std::collections::BTreeSet<_>>();
    if run_ids != bundled_run_ids {
        return Err("DCP Session bundle run index mismatch".into());
    }
    for (run_id, run) in &bundle.runs {
        validate_run_state(&run.state, session_id, run_id, Some(&bundle.dcp.agent.definition_hash))?;
        run.tools.validate()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
