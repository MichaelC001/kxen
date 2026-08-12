//! JSONL journals with reliability policy encoded by type.

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use crate::core::durability::{self, CommitError};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JournalCursor {
    pub byte_offset: u64,
    pub record_index: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppendReceipt {
    pub start: JournalCursor,
    pub end: JournalCursor,
}

#[derive(Debug)]
pub struct StrictScan<T> {
    pub records: Vec<T>,
    pub next: JournalCursor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalDiagnostic {
    pub record_index: u64,
    pub message: String,
}

#[derive(Debug)]
pub struct BestEffortScan<T> {
    pub records: Vec<T>,
    pub diagnostics: Vec<JournalDiagnostic>,
    pub next: JournalCursor,
}

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("read journal {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("journal cursor {offset} is not a record boundary in {path}")]
    InvalidCursor { path: PathBuf, offset: u64 },
    #[error("unterminated JSONL record in {path} at record {record_index}")]
    Unterminated { path: PathBuf, record_index: u64 },
    #[error("parse {path} record {record_index}: {source}")]
    Parse { path: PathBuf, record_index: u64, source: serde_json::Error },
    #[error(transparent)]
    Commit(#[from] CommitError),
    #[error("serialize journal record for {path}: {source}")]
    Serialize { path: PathBuf, source: serde_json::Error },
}

pub struct StrictJsonl<T> {
    path: PathBuf,
    marker: PhantomData<fn() -> T>,
}

pub struct BestEffortJsonl<T> {
    path: PathBuf,
    marker: PhantomData<fn() -> T>,
}

impl<T> StrictJsonl<T> {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), marker: PhantomData }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl<T: Serialize> StrictJsonl<T> {
    pub fn append(&self, record: &T, current_index: u64) -> Result<AppendReceipt, JournalError> {
        let mut bytes = serde_json::to_vec(record).map_err(|source| JournalError::Serialize { path: self.path.clone(), source })?;
        bytes.push(b'\n');
        let start_offset = file_len(&self.path)?;
        durability::append_synced(&self.path, &bytes)?;
        Ok(AppendReceipt {
            start: JournalCursor { byte_offset: start_offset, record_index: current_index },
            end: JournalCursor { byte_offset: start_offset + bytes.len() as u64, record_index: current_index + 1 },
        })
    }
}

impl<T: DeserializeOwned> StrictJsonl<T> {
    pub fn scan(&self, cursor: JournalCursor) -> Result<StrictScan<T>, JournalError> {
        let bytes = read_from_cursor(&self.path, cursor)?;
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            return Err(JournalError::Unterminated {
                path: self.path.clone(),
                record_index: cursor.record_index + bytes.iter().filter(|byte| **byte == b'\n').count() as u64 + 1,
            });
        }
        let mut records = Vec::new();
        for (offset, line) in bytes.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()).enumerate() {
            let record_index = cursor.record_index + offset as u64 + 1;
            records.push(serde_json::from_slice(line).map_err(|source| JournalError::Parse {
                path: self.path.clone(),
                record_index,
                source,
            })?);
        }
        Ok(StrictScan {
            next: JournalCursor {
                byte_offset: cursor.byte_offset + bytes.len() as u64,
                record_index: cursor.record_index + records.len() as u64,
            },
            records,
        })
    }
}

impl<T> BestEffortJsonl<T> {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), marker: PhantomData }
    }
}

impl<T: DeserializeOwned> BestEffortJsonl<T> {
    pub fn scan(&self, cursor: JournalCursor) -> Result<BestEffortScan<T>, JournalError> {
        let bytes = read_from_cursor(&self.path, cursor)?;
        let mut records = Vec::new();
        let mut diagnostics = Vec::new();
        let mut offset = 0u64;
        for line in bytes.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {
            offset += 1;
            let record_index = cursor.record_index + offset;
            match serde_json::from_slice(line) {
                Ok(record) => records.push(record),
                Err(source) => diagnostics.push(JournalDiagnostic { record_index, message: source.to_string() }),
            }
        }
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            diagnostics
                .push(JournalDiagnostic { record_index: cursor.record_index + offset.max(1), message: "unterminated final record".into() });
        }
        Ok(BestEffortScan {
            records,
            diagnostics,
            next: JournalCursor { byte_offset: cursor.byte_offset + bytes.len() as u64, record_index: cursor.record_index + offset },
        })
    }
}

fn file_len(path: &Path) -> Result<u64, JournalError> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(source) => Err(JournalError::Read { path: path.to_path_buf(), source }),
    }
}

fn read_from_cursor(path: &Path, cursor: JournalCursor) -> Result<Vec<u8>, JournalError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(JournalError::Read { path: path.to_path_buf(), source }),
    };
    let offset = usize::try_from(cursor.byte_offset)
        .ok()
        .filter(|offset| *offset <= bytes.len())
        .ok_or_else(|| JournalError::InvalidCursor { path: path.to_path_buf(), offset: cursor.byte_offset })?;
    if offset > 0 && bytes[offset - 1] != b'\n' {
        return Err(JournalError::InvalidCursor { path: path.to_path_buf(), offset: cursor.byte_offset });
    }
    Ok(bytes[offset..].to_vec())
}

#[cfg(test)]
#[path = "journal/tests.rs"]
mod tests;
