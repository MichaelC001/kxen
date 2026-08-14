use super::{ToolBoundaryAction, ToolBoundaryJournal};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const TOOL_JOURNAL_SCHEMA: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DcpToolPhase {
    Started,
    OutcomeKnown,
    Settled,
    OutcomeUnknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DcpToolOperation {
    pub operation_id: String,
    pub call_ids: Vec<String>,
    pub tool_name: String,
    pub arguments_json: String,
    pub phase: DcpToolPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown_reason: Option<String>,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DcpToolJournalSnapshot {
    pub schema_version: u32,
    pub operations: Vec<DcpToolOperation>,
}

impl Default for DcpToolJournalSnapshot {
    fn default() -> Self {
        Self { schema_version: TOOL_JOURNAL_SCHEMA, operations: Vec::new() }
    }
}

impl DcpToolJournalSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != TOOL_JOURNAL_SCHEMA {
            return Err(format!("unsupported DCP tool journal schema {}", self.schema_version));
        }
        let mut operation_ids = std::collections::BTreeSet::new();
        let mut call_ids = std::collections::BTreeSet::new();
        for operation in &self.operations {
            crate::core::ids::validate_id(&operation.operation_id)?;
            if !operation_ids.insert(operation.operation_id.as_str()) {
                return Err(format!("duplicate DCP tool operation id: {}", operation.operation_id));
            }
            if operation.call_ids.is_empty() || operation.call_ids.iter().any(|call_id| call_id.trim().is_empty()) {
                return Err(format!("DCP tool operation {} has an invalid call id", operation.operation_id));
            }
            for call_id in &operation.call_ids {
                if !call_ids.insert(call_id.as_str()) {
                    return Err(format!("duplicate DCP tool call id: {call_id}"));
                }
            }
            if operation.tool_name.trim().is_empty() || serde_json::from_str::<serde_json::Value>(&operation.arguments_json).is_err() {
                return Err(format!("DCP tool operation {} has an invalid tool call", operation.operation_id));
            }
            if operation.updated_at_ms < operation.started_at_ms {
                return Err(format!("DCP tool operation {} has decreasing timestamps", operation.operation_id));
            }
            let valid_outcome = match operation.phase {
                DcpToolPhase::Started => operation.output.is_none() && operation.unknown_reason.is_none() && !operation.is_error,
                DcpToolPhase::OutcomeKnown | DcpToolPhase::Settled => operation.output.is_some() && operation.unknown_reason.is_none(),
                DcpToolPhase::OutcomeUnknown => {
                    operation.output.is_none()
                        && operation.unknown_reason.as_ref().is_some_and(|reason| !reason.trim().is_empty())
                        && !operation.is_error
                }
            };
            if !valid_outcome {
                return Err(format!("DCP tool operation {} has inconsistent phase data", operation.operation_id));
            }
        }
        Ok(())
    }
}

pub struct DcpRunToolJournal {
    path: PathBuf,
    state: Mutex<DcpToolJournalSnapshot>,
    _lease: std::fs::File,
}

impl Drop for DcpRunToolJournal {
    fn drop(&mut self) {
        let _ = self._lease.unlock();
    }
}

impl DcpRunToolJournal {
    pub fn open(run_dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(run_dir).map_err(|error| format!("create DCPRun directory {}: {error}", run_dir.display()))?;
        let lock_path = run_dir.join("tools.lock");
        let lease = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|error| format!("open DCPRun tool lock {}: {error}", lock_path.display()))?;
        lease.try_lock().map_err(|error| format!("DCPRun tool journal is already active at {}: {error}", run_dir.display()))?;
        let path = run_dir.join("tools.json");
        let state = load_snapshot(&path)?;
        Ok(Self { path, state: Mutex::new(state), _lease: lease })
    }

    pub fn snapshot(&self) -> DcpToolJournalSnapshot {
        crate::core::shared::lock(&self.state).clone()
    }

    pub fn reconcile(&self, messages: &[crate::core::session::Message]) -> Result<Vec<DcpToolOperation>, String> {
        let persisted = messages
            .iter()
            .flat_map(|message| &message.parts)
            .filter_map(|part| match part {
                crate::core::session::Part::ToolCall { id: Some(id), .. } => Some(id.as_str()),
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>();
        let mut state = crate::core::shared::lock(&self.state);
        let mut changed = false;
        for operation in &mut state.operations {
            match operation.phase {
                DcpToolPhase::Started => {
                    operation.phase = DcpToolPhase::OutcomeUnknown;
                    operation.unknown_reason = Some("process stopped after durable tool start without a durable outcome".into());
                    operation.updated_at_ms = crate::core::shared::now_ms();
                    changed = true;
                }
                DcpToolPhase::OutcomeKnown if operation.call_ids.iter().any(|id| persisted.contains(id.as_str())) => {
                    operation.phase = DcpToolPhase::Settled;
                    operation.updated_at_ms = crate::core::shared::now_ms();
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            save_snapshot(&self.path, &state)?;
        }
        Ok(state.operations.iter().filter(|operation| operation.phase == DcpToolPhase::OutcomeUnknown).cloned().collect())
    }

    pub fn settle_parts(&self, parts: &[crate::core::session::Part]) -> Result<(), String> {
        let ids = parts
            .iter()
            .filter_map(|part| match part {
                crate::core::session::Part::ToolCall { id: Some(id), .. } => Some(id.as_str()),
                _ => None,
            })
            .collect::<std::collections::HashSet<_>>();
        let mut state = crate::core::shared::lock(&self.state);
        let mut changed = false;
        for operation in &mut state.operations {
            if operation.phase == DcpToolPhase::OutcomeKnown && operation.call_ids.iter().any(|id| ids.contains(id.as_str())) {
                operation.phase = DcpToolPhase::Settled;
                operation.updated_at_ms = crate::core::shared::now_ms();
                changed = true;
            }
        }
        if changed {
            save_snapshot(&self.path, &state)?;
        }
        Ok(())
    }

    pub fn unrecorded_outcomes(&self) -> Vec<DcpToolOperation> {
        crate::core::shared::lock(&self.state)
            .operations
            .iter()
            .filter(|operation| operation.phase == DcpToolPhase::OutcomeKnown)
            .cloned()
            .collect()
    }

    pub fn settle_operations(&self, operation_ids: &[String]) -> Result<(), String> {
        let ids = operation_ids.iter().map(String::as_str).collect::<std::collections::HashSet<_>>();
        let mut state = crate::core::shared::lock(&self.state);
        let mut changed = false;
        for operation in &mut state.operations {
            if operation.phase == DcpToolPhase::OutcomeKnown && ids.contains(operation.operation_id.as_str()) {
                operation.phase = DcpToolPhase::Settled;
                operation.updated_at_ms = crate::core::shared::now_ms();
                changed = true;
            }
        }
        if changed {
            save_snapshot(&self.path, &state)?;
        }
        Ok(())
    }

    pub fn resolve_unknown(&self, operation_id: &str, output: &str, is_error: bool) -> Result<DcpToolOperation, String> {
        crate::core::ids::validate_id(operation_id)?;
        let mut state = crate::core::shared::lock(&self.state);
        let operation = state
            .operations
            .iter_mut()
            .find(|operation| operation.operation_id == operation_id)
            .ok_or_else(|| format!("DCP tool operation not found: {operation_id}"))?;
        if operation.phase != DcpToolPhase::OutcomeUnknown {
            return Err(format!("DCP tool operation is not UNKNOWN: {operation_id}"));
        }
        operation.phase = DcpToolPhase::OutcomeKnown;
        operation.output = Some(output.into());
        operation.is_error = is_error;
        operation.unknown_reason = None;
        operation.updated_at_ms = crate::core::shared::now_ms();
        let resolved = operation.clone();
        save_snapshot(&self.path, &state)?;
        Ok(resolved)
    }

    fn assign_existing(
        state: &mut DcpToolJournalSnapshot,
        call_id: &str,
        tool_name: &str,
        arguments_json: &str,
    ) -> Result<Option<ToolBoundaryAction>, String> {
        if let Some(operation) = state.operations.iter_mut().find(|operation| operation.call_ids.iter().any(|id| id == call_id)) {
            validate_call(operation, tool_name, arguments_json)?;
            return action_for(operation).map(Some);
        }
        Ok(None)
    }
}

impl ToolBoundaryJournal for DcpRunToolJournal {
    fn before(&self, call_id: &str, tool_name: &str, arguments_json: &str, at_ms: u64) -> Result<ToolBoundaryAction, String> {
        let mut state = crate::core::shared::lock(&self.state);
        if let Some(action) = Self::assign_existing(&mut state, call_id, tool_name, arguments_json)? {
            save_snapshot(&self.path, &state)?;
            return Ok(action);
        }
        let operation = DcpToolOperation {
            operation_id: crate::core::ids::new_id("op"),
            call_ids: vec![call_id.into()],
            tool_name: tool_name.into(),
            arguments_json: arguments_json.into(),
            phase: DcpToolPhase::Started,
            output: None,
            is_error: false,
            unknown_reason: None,
            started_at_ms: at_ms,
            updated_at_ms: at_ms,
        };
        state.operations.push(operation);
        save_snapshot(&self.path, &state)?;
        Ok(ToolBoundaryAction::Execute)
    }

    fn after(&self, call_id: &str, tool_name: &str, arguments_json: &str, output: &str, is_error: bool, at_ms: u64) -> Result<(), String> {
        let mut state = crate::core::shared::lock(&self.state);
        let operation = state
            .operations
            .iter_mut()
            .find(|operation| operation.call_ids.iter().any(|id| id == call_id))
            .ok_or_else(|| format!("DCP tool operation assignment is missing for call {call_id}"))?;
        validate_call(operation, tool_name, arguments_json)?;
        match operation.phase {
            DcpToolPhase::Started => {
                operation.phase = DcpToolPhase::OutcomeKnown;
                operation.output = Some(output.into());
                operation.is_error = is_error;
                operation.updated_at_ms = at_ms;
            }
            DcpToolPhase::OutcomeKnown | DcpToolPhase::Settled
                if operation.output.as_deref() == Some(output) && operation.is_error == is_error => {}
            phase => return Err(format!("invalid DCP tool outcome transition from {phase:?}")),
        }
        save_snapshot(&self.path, &state)
    }

    fn mark_unknown(&self, call_id: &str, reason: &str, at_ms: u64) -> Result<(), String> {
        let mut state = crate::core::shared::lock(&self.state);
        let operation = state
            .operations
            .iter_mut()
            .find(|operation| operation.call_ids.iter().any(|id| id == call_id))
            .ok_or_else(|| format!("DCP tool operation assignment is missing for call {call_id}"))?;
        match operation.phase {
            DcpToolPhase::Started => {
                operation.phase = DcpToolPhase::OutcomeUnknown;
                operation.unknown_reason = Some(reason.into());
                operation.updated_at_ms = at_ms;
            }
            DcpToolPhase::OutcomeUnknown if operation.unknown_reason.as_deref() == Some(reason) => {}
            phase => return Err(format!("invalid DCP tool UNKNOWN transition from {phase:?}")),
        }
        save_snapshot(&self.path, &state)
    }

    fn should_pause(&self) -> bool {
        crate::core::shared::lock(&self.state).operations.iter().any(|operation| operation.phase == DcpToolPhase::OutcomeUnknown)
    }
}

fn validate_call(operation: &DcpToolOperation, tool_name: &str, arguments_json: &str) -> Result<(), String> {
    if operation.tool_name != tool_name || operation.arguments_json != arguments_json {
        return Err(format!("provider tool call id collision for {}", operation.call_ids.join(",")));
    }
    Ok(())
}

fn action_for(operation: &DcpToolOperation) -> Result<ToolBoundaryAction, String> {
    match operation.phase {
        DcpToolPhase::OutcomeKnown | DcpToolPhase::Settled => Ok(ToolBoundaryAction::Replay {
            output: operation.output.clone().ok_or("known DCP tool outcome has no output")?,
            is_error: operation.is_error,
        }),
        DcpToolPhase::OutcomeUnknown => Err(format!(
            "tool outcome is UNKNOWN for {}: {}",
            operation.operation_id,
            operation.unknown_reason.as_deref().unwrap_or("no recovery evidence")
        )),
        DcpToolPhase::Started => Err(format!("tool operation {} is already running", operation.operation_id)),
    }
}

fn load_snapshot(path: &Path) -> Result<DcpToolJournalSnapshot, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(DcpToolJournalSnapshot::default()),
        Err(error) => return Err(format!("read DCP tool journal {}: {error}", path.display())),
    };
    let snapshot: DcpToolJournalSnapshot =
        serde_json::from_slice(&bytes).map_err(|error| format!("parse DCP tool journal {}: {error}", path.display()))?;
    snapshot.validate()?;
    Ok(snapshot)
}

fn save_snapshot(path: &Path, snapshot: &DcpToolJournalSnapshot) -> Result<(), String> {
    snapshot.validate()?;
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(|error| error.to_string())?;
    crate::core::durability::atomic_replace(path, &bytes).map_err(|error| format!("write DCP tool journal {}: {error}", path.display()))
}

#[cfg(test)]
#[path = "agent_journal_tests.rs"]
mod tests;
