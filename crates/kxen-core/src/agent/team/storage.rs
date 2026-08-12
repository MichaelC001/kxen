use serde::Serialize;
use std::path::Path;

use crate::core::durability::{self, CommitPhase};

#[derive(Debug)]
pub(super) struct PersistFailure {
    phase: CommitPhase,
    message: String,
}

impl std::fmt::Display for PersistFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PersistFailure {}

impl PersistFailure {
    fn before(message: String) -> Self {
        Self { phase: CommitPhase::PreCommit, message }
    }

    pub(super) fn committed(&self) -> bool {
        self.phase == CommitPhase::PostCommit
    }

    pub(super) fn into_message(self) -> String {
        self.message
    }
}

pub(super) fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<(), PersistFailure> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| PersistFailure::before(format!("serialize {}: {error}", path.display())))?;
    write_bytes_atomic(path, &bytes)
}

pub(super) fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), PersistFailure> {
    durability::atomic_replace(path, bytes).map_err(|error| PersistFailure { phase: error.phase(), message: error.to_string() })
}

#[cfg(test)]
pub(super) fn inject_before_rename() {
    durability::inject_before_replace("injected team pre-commit failure");
}

#[cfg(test)]
pub(super) fn inject_parent_sync() {
    durability::inject_parent_sync("injected team parent sync failure");
}
