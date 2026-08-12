//! 文件持久化原语。领域事务负责业务一致性，本模块只负责可见性边界和 commit phase。

use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitPhase {
    PreCommit,
    PostCommit,
}

#[derive(Debug)]
pub struct CommitError {
    phase: CommitPhase,
    operation: &'static str,
    path: Option<PathBuf>,
    source: std::io::Error,
}

impl CommitError {
    pub(crate) fn before(error: impl Into<std::io::Error>) -> Self {
        Self::new(CommitPhase::PreCommit, "domain", None, error.into())
    }

    pub(crate) fn after(error: impl Into<std::io::Error>) -> Self {
        Self::new(CommitPhase::PostCommit, "domain", None, error.into())
    }

    pub(crate) fn after_visible(mut self) -> Self {
        self.phase = CommitPhase::PostCommit;
        self
    }

    fn at(phase: CommitPhase, operation: &'static str, path: &Path, source: std::io::Error) -> Self {
        Self::new(phase, operation, Some(path.to_path_buf()), source)
    }

    fn new(phase: CommitPhase, operation: &'static str, path: Option<PathBuf>, source: std::io::Error) -> Self {
        Self { phase, operation, path, source }
    }

    pub fn phase(&self) -> CommitPhase {
        self.phase
    }

    pub fn committed(&self) -> bool {
        self.phase == CommitPhase::PostCommit
    }

    pub fn operation(&self) -> &'static str {
        self.operation
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn kind(&self) -> std::io::ErrorKind {
        self.source.kind()
    }

    pub fn into_io_error(self) -> std::io::Error {
        self.source
    }
}

impl std::fmt::Display for CommitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.source)
    }
}

impl std::error::Error for CommitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

pub fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), CommitError> {
    let parent = parent(path);
    std::fs::create_dir_all(parent).map_err(|error| CommitError::at(CommitPhase::PreCommit, "create_parent", parent, error))?;
    let temporary = temporary_path(path);
    let result = write_new_file(&temporary, bytes).and_then(|()| {
        before_replace(path)?;
        std::fs::rename(&temporary, path).map_err(|error| CommitError::at(CommitPhase::PreCommit, "replace", path, error))?;
        sync_directory(parent).map_err(|error| CommitError::at(CommitPhase::PostCommit, "sync_parent", parent, error))
    });
    if result.is_err() {
        std::fs::remove_file(&temporary).ok();
    }
    result
}

pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), CommitError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| CommitError::at(CommitPhase::PreCommit, "serialize_json", path, std::io::Error::other(error)))?;
    atomic_replace(path, &bytes)
}

pub fn append_synced(path: &Path, bytes: &[u8]) -> Result<(), CommitError> {
    if !path.exists() {
        return atomic_replace(path, bytes);
    }
    let mut file =
        OpenOptions::new().append(true).open(path).map_err(|error| CommitError::at(CommitPhase::PreCommit, "open_append", path, error))?;
    before_append(path)?;
    file.write_all(bytes).map_err(|error| CommitError::at(CommitPhase::PostCommit, "append", path, error))?;
    before_append_sync(path)?;
    file.sync_data().map_err(|error| CommitError::at(CommitPhase::PostCommit, "sync_append", path, error))
}

pub fn rename_durable(from: &Path, to: &Path) -> Result<(), CommitError> {
    let source_parent = parent(from);
    let target_parent = parent(to);
    std::fs::create_dir_all(target_parent)
        .map_err(|error| CommitError::at(CommitPhase::PreCommit, "create_parent", target_parent, error))?;
    before_replace(to)?;
    std::fs::rename(from, to).map_err(|error| CommitError::at(CommitPhase::PreCommit, "rename", to, error))?;
    sync_directory(target_parent).map_err(|error| CommitError::at(CommitPhase::PostCommit, "sync_target_parent", target_parent, error))?;
    if source_parent != target_parent {
        sync_directory(source_parent)
            .map_err(|error| CommitError::at(CommitPhase::PostCommit, "sync_source_parent", source_parent, error))?;
    }
    Ok(())
}

pub(crate) fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), CommitError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| CommitError::at(CommitPhase::PreCommit, "create_temporary", path, error))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        std::fs::remove_file(path).ok();
        return Err(CommitError::at(CommitPhase::PreCommit, "write_temporary", path, error));
    }
    Ok(())
}

pub(crate) fn temporary_path(path: &Path) -> PathBuf {
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("data");
    path.with_file_name(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()))
}

pub(crate) fn before_replace(path: &Path) -> Result<(), CommitError> {
    injected(FaultPoint::BeforeReplace, path).map_err(|error| CommitError::at(CommitPhase::PreCommit, "replace", path, error))
}

pub(crate) fn sync_directory(path: &Path) -> std::io::Result<()> {
    injected(FaultPoint::ParentSync, path)?;
    #[cfg(unix)]
    {
        std::fs::File::open(path).and_then(|directory| directory.sync_all())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn parent(path: &Path) -> &Path {
    path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."))
}

fn before_append(path: &Path) -> Result<(), CommitError> {
    injected(FaultPoint::BeforeAppend, path).map_err(|error| CommitError::at(CommitPhase::PreCommit, "append", path, error))
}

fn before_append_sync(path: &Path) -> Result<(), CommitError> {
    injected(FaultPoint::AppendSync, path).map_err(|error| CommitError::at(CommitPhase::PostCommit, "sync_append", path, error))
}

#[derive(Clone, Copy)]
enum FaultPoint {
    BeforeReplace,
    BeforeAppend,
    AppendSync,
    ParentSync,
}

#[cfg(test)]
#[derive(Default)]
struct Faults {
    before_replace: Option<&'static str>,
    before_append: Option<&'static str>,
    append_sync: Option<&'static str>,
    parent_sync: Option<&'static str>,
}

#[cfg(test)]
thread_local! {
    static FAULTS: std::cell::RefCell<Faults> = std::cell::RefCell::new(Faults::default());
}

fn injected(point: FaultPoint, path: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if let Some(message) = FAULTS.with(|faults| {
        let mut faults = faults.borrow_mut();
        match point {
            FaultPoint::BeforeReplace => faults.before_replace.take(),
            FaultPoint::BeforeAppend => faults.before_append.take(),
            FaultPoint::AppendSync => faults.append_sync.take(),
            FaultPoint::ParentSync => faults.parent_sync.take(),
        }
    }) {
        return Err(std::io::Error::other(format!("{message}: {}", path.display())));
    }
    let _ = (point, path);
    Ok(())
}

#[cfg(test)]
pub(crate) fn inject_before_replace(message: &'static str) {
    FAULTS.with(|faults| faults.borrow_mut().before_replace = Some(message));
}

#[cfg(test)]
pub(crate) fn inject_before_append(message: &'static str) {
    FAULTS.with(|faults| faults.borrow_mut().before_append = Some(message));
}

#[cfg(test)]
pub(crate) fn inject_append_sync(message: &'static str) {
    FAULTS.with(|faults| faults.borrow_mut().append_sync = Some(message));
}

#[cfg(test)]
pub(crate) fn inject_parent_sync(message: &'static str) {
    FAULTS.with(|faults| faults.borrow_mut().parent_sync = Some(message));
}

#[cfg(test)]
#[path = "durability/tests.rs"]
mod tests;
