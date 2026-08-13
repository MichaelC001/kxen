use std::fs::OpenOptions;
use std::path::Path;

use crate::core::durability;

pub use crate::core::durability::{CommitError as CommitFailure, CommitPhase};

/// 修复返回 `PostCommit` 的消息 append：校验可见消息、sync JSONL、修 meta、sync 父目录。
/// 内存 block 仅在全部耐久步骤成功后才清除。
pub fn repair_message_durability(dir: &Path, message: &super::Message, original: &CommitFailure) -> Result<super::Session, CommitFailure> {
    if !original.committed() {
        return Err(CommitFailure::before(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "only a post-commit message append can be repaired",
        )));
    }
    crate::core::ids::validate_id_io(&message.session_id).map_err(CommitFailure::before)?;
    crate::core::ids::validate_id_io(&message.id).map_err(CommitFailure::before)?;
    let cause = original.to_string();
    let _transaction = super::transaction::acquire_transaction_at(dir, &message.session_id).map_err(CommitFailure::after)?;
    if crate::core::session_recovery::is_tombstoned(dir, &message.session_id)
        .map_err(|error| CommitFailure::after(std::io::Error::other(error)))?
    {
        return Err(CommitFailure::after(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("session deletion in progress: {}", message.session_id),
        )));
    }
    super::transaction::ensure_matching_append_block(&message.session_id, &message.id, &cause).map_err(CommitFailure::after)?;
    let visible =
        super::messages::scan_messages_checked_unlocked(dir, &message.session_id, Some(&message.id)).map_err(CommitFailure::after)?;
    if visible.matching_count != 1 {
        return Err(CommitFailure::after(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("message {} is visible {} times after indeterminate append", message.id, visible.matching_count),
        )));
    }
    if serde_json::to_value(visible.matching.as_ref().expect("one matching message"))
        .map_err(|error| CommitFailure::after(std::io::Error::other(error)))?
        != serde_json::to_value(message).map_err(|error| CommitFailure::after(std::io::Error::other(error)))?
    {
        return Err(CommitFailure::after(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("message id collision during durability repair: {}", message.id),
        )));
    }
    let messages_path = super::messages_path(dir, &message.session_id);
    OpenOptions::new().read(true).write(true).open(&messages_path).and_then(|file| file.sync_all()).map_err(CommitFailure::after)?;
    let session = super::append::repair_meta_after_idempotent_append(dir, message, visible.count).map_err(CommitFailure::after_visible)?;
    durability::sync_directory(dir).map_err(CommitFailure::after)?;
    super::transaction::clear_matching_append_block(&message.session_id, &message.id, &cause).map_err(CommitFailure::after)?;
    Ok(session)
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CommitFailure> {
    durability::atomic_replace(path, bytes)
}

/// 新会话先发布完整 messages+meta 再 sync 共享目录；meta rename 放最后（其存在即 admission 标记）。
pub(super) fn create_session_files(meta: &Path, meta_bytes: &[u8], messages: &Path, message_bytes: &[u8]) -> Result<(), CommitFailure> {
    let parent = meta.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(CommitFailure::before)?;
    let meta_tmp = durability::temporary_path(meta);
    let messages_tmp = durability::temporary_path(messages);
    let staged = durability::write_new_file(&meta_tmp, meta_bytes).and_then(|()| durability::write_new_file(&messages_tmp, message_bytes));
    if let Err(error) = staged {
        cleanup(&[&meta_tmp, &messages_tmp]);
        return Err(error);
    }
    if let Err(error) = durability::before_replace(meta) {
        cleanup(&[&meta_tmp, &messages_tmp]);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&messages_tmp, messages) {
        cleanup(&[&meta_tmp, &messages_tmp]);
        return Err(CommitFailure::before(error));
    }
    if let Err(error) = fail_after_messages_rename(messages) {
        std::fs::remove_file(messages).ok();
        cleanup(&[&meta_tmp]);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&meta_tmp, meta) {
        cleanup(&[&meta_tmp]);
        std::fs::remove_file(messages).ok();
        return Err(CommitFailure::before(std::io::Error::new(error.kind(), format!("publish session metadata after messages: {error}"))));
    }
    durability::sync_directory(parent).map_err(CommitFailure::after)
}

pub(crate) fn append_synced(path: &Path, bytes: &[u8]) -> Result<(), CommitFailure> {
    durability::append_synced(path, bytes)
}

fn cleanup(paths: &[&Path]) {
    for path in paths {
        std::fs::remove_file(path).ok();
    }
}

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_AFTER_MESSAGES_RENAME: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(super) fn inject_before_rename() {
    durability::inject_before_replace("injected session pre-commit failure");
}

#[cfg(test)]
pub(super) fn inject_before_append() {
    durability::inject_before_append("injected session append pre-commit failure");
}

#[cfg(test)]
pub(crate) fn inject_append_sync() {
    durability::inject_append_sync("injected session append sync failure");
}

#[cfg(test)]
pub(super) fn inject_parent_sync() {
    durability::inject_parent_sync("injected session parent sync failure");
}

#[cfg(test)]
pub(super) fn inject_after_messages_rename() {
    FAIL_NEXT_AFTER_MESSAGES_RENAME.with(|fault| fault.set(true));
}

fn fail_after_messages_rename(_path: &Path) -> Result<(), CommitFailure> {
    #[cfg(test)]
    if FAIL_NEXT_AFTER_MESSAGES_RENAME.with(|fault| fault.replace(false)) {
        return Err(CommitFailure::before(std::io::Error::other(format!(
            "injected session publish failure after messages rename: {}",
            _path.display()
        ))));
    }
    Ok(())
}
