use super::*;

fn fixture(name: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("kxen-durability-{name}-{}", uuid::Uuid::new_v4()));
    let path = dir.join("state.json");
    (dir, path)
}

#[test]
fn atomic_replace_reports_precommit_before_visibility() {
    let (dir, path) = fixture("precommit");
    atomic_replace(&path, b"old").unwrap();
    inject_before_replace("injected common pre-commit failure");

    let error = atomic_replace(&path, b"new").unwrap_err();

    assert_eq!(error.phase(), CommitPhase::PreCommit);
    assert_eq!(error.operation(), "replace");
    assert_eq!(std::fs::read(&path).unwrap(), b"old");
    assert!(std::fs::read_dir(&dir).unwrap().all(|entry| !entry.unwrap().file_name().to_string_lossy().ends_with(".tmp")));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn atomic_replace_reports_postcommit_after_parent_sync_failure() {
    let (dir, path) = fixture("postcommit");
    inject_parent_sync("injected common parent sync failure");

    let error = atomic_replace(&path, b"visible").unwrap_err();

    assert_eq!(error.phase(), CommitPhase::PostCommit);
    assert_eq!(error.operation(), "sync_parent");
    assert_eq!(error.path(), Some(dir.as_path()));
    assert_eq!(std::fs::read(&path).unwrap(), b"visible");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn append_failure_after_write_is_postcommit() {
    let (dir, path) = fixture("append");
    atomic_replace(&path, b"first\n").unwrap();
    inject_append_sync("injected common append sync failure");

    let error = append_synced(&path, b"second\n").unwrap_err();

    assert_eq!(error.phase(), CommitPhase::PostCommit);
    assert_eq!(error.operation(), "sync_append");
    assert_eq!(std::fs::read(&path).unwrap(), b"first\nsecond\n");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn append_precommit_failure_does_not_change_file() {
    let (dir, path) = fixture("append-precommit");
    atomic_replace(&path, b"first\n").unwrap();
    inject_before_append("injected common append pre-commit failure");

    let error = append_synced(&path, b"second\n").unwrap_err();

    assert_eq!(error.phase(), CommitPhase::PreCommit);
    assert_eq!(std::fs::read(&path).unwrap(), b"first\n");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn durable_rename_moves_to_recoverable_location() {
    let root = std::env::temp_dir().join(format!("kxen-durability-rename-{}", uuid::Uuid::new_v4()));
    let from = root.join("active/item");
    let to = root.join("trash/item");
    std::fs::create_dir_all(&from).unwrap();
    atomic_replace(&from.join("data"), b"value").unwrap();
    rename_durable(&from, &to).unwrap();
    assert!(!from.exists());
    assert_eq!(std::fs::read(to.join("data")).unwrap(), b"value");
    std::fs::remove_dir_all(root).ok();
}
