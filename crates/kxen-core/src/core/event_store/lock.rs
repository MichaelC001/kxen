use std::path::Path;

use super::EventStoreError;

#[cfg(not(test))]
const LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(test)]
const LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(300);

pub(super) fn acquire(root: &Path) -> Result<std::fs::File, EventStoreError> {
    std::fs::create_dir_all(root)?;
    let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(root.join("events.lock"))?;
    let deadline = std::time::Instant::now() + LOCK_TIMEOUT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(std::fs::TryLockError::WouldBlock) => return Err(EventStoreError::Locked),
            Err(std::fs::TryLockError::Error(error)) => return Err(EventStoreError::Io(error)),
        }
    }
}
