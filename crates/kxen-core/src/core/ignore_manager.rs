//! Kxen 项目运行态目录的 ignore 管理。路径只由 `KxenPaths` 提供。

use std::io::Write as _;
use std::path::{Path, PathBuf};

pub(crate) const PROJECT_GITIGNORE: &str = "/.agents/kxen/";
pub(crate) const PROJECT_GIT_PATHSPEC_EXCLUDE: &str = ":(exclude).agents/kxen";
const USER_GITIGNORE: &str = "/kxen/";

pub fn prepare_user(paths: &super::paths::UserPaths) -> Result<(), String> {
    paths.ensure_base_dirs()?;
    match paths.ignore_root() {
        Some(root) => ensure_ignore_file(root, USER_GITIGNORE, &["kxen/", "/kxen/"]),
        None => Ok(()),
    }
}

/// 一次完成项目运行态基础目录创建和 Git ignore 维护。
pub fn prepare_project(paths: &super::paths::ProjectPaths) -> Result<(), String> {
    paths.ensure_base_dir()?;
    if paths.workspace().ancestors().any(|ancestor| ancestor.join(".git").exists()) { ensure_gitignore(paths.workspace()) } else { Ok(()) }
}

pub fn ensure_gitignore(workspace: &Path) -> Result<(), String> {
    ensure_ignore_file(workspace, PROJECT_GITIGNORE, &[".kxen/", "/.kxen/", ".agents/kxen/", "/.agents/kxen/"])
}

fn ensure_ignore_file(root: &Path, canonical_rule: &str, equivalent_rules: &[&str]) -> Result<(), String> {
    static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = super::shared::lock(&WRITE_LOCK);
    let path = root.join(".gitignore");
    reject_symlink(&path)?;
    let current = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("read {}: {error}", path.display())),
    };
    let updated = normalize_gitignore(&current, canonical_rule, equivalent_rules);
    if updated == current {
        return Ok(());
    }
    atomic_write_preserving_permissions(&path, updated.as_bytes())
}

fn normalize_gitignore(content: &str, canonical_rule: &str, equivalent_rules: &[&str]) -> String {
    let mut output = String::new();
    let mut inserted = false;
    for line in content.lines() {
        if equivalent_rules.contains(&line.trim()) {
            if !inserted {
                output.push_str(canonical_rule);
                output.push('\n');
                inserted = true;
            }
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    if !inserted {
        output.push_str(canonical_rule);
        output.push('\n');
    }
    output
}

fn atomic_write_preserving_permissions(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    let temporary = temporary_path(path);
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|error| format!("open {}: {error}", temporary.display()))?;
        if let Ok(metadata) = std::fs::metadata(path) {
            std::fs::set_permissions(&temporary, metadata.permissions())
                .map_err(|error| format!("set permissions {}: {error}", temporary.display()))?;
        }
        file.write_all(bytes).map_err(|error| format!("write {}: {error}", temporary.display()))?;
        file.sync_all().map_err(|error| format!("sync {}: {error}", temporary.display()))?;
        drop(file);
        replace_file(&temporary, path).map_err(|error| format!("replace {}: {error}", path.display()))?;
        sync_dir(parent)
    })();
    if result.is_err() {
        std::fs::remove_file(&temporary).ok();
    }
    result
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    std::fs::rename(temporary, path)
}

#[cfg(windows)]
fn replace_file(temporary: &Path, path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW};

    let temporary = temporary.as_os_str().encode_wide().chain(std::iter::once(0)).collect::<Vec<_>>();
    let path = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect::<Vec<_>>();
    // std::fs::rename 不能在 Windows 覆盖已有文件，MoveFileExW 提供同一卷内原子替换语义。
    if unsafe { MoveFileExW(temporary.as_ptr(), path.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) } == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(".gitignore.agents-kxen-{}.tmp", uuid::Uuid::new_v4()))
}

fn reject_symlink(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!("refusing symlinked ignore file: {}", path.display())),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect {}: {error}", path.display())),
    }
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<(), String> {
    std::fs::File::open(path).and_then(|directory| directory.sync_all()).map_err(|error| format!("sync {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_adds_new_rule_and_deduplicates() {
        let rules = [".kxen/", "/.kxen/", ".agents/kxen/", "/.agents/kxen/"];
        assert_eq!(normalize_gitignore("target/\n.kxen/\n", PROJECT_GITIGNORE, &rules), "target/\n/.agents/kxen/\n");
        assert_eq!(
            normalize_gitignore("target/\n.agents/kxen/\n/.agents/kxen/\n.env\n", PROJECT_GITIGNORE, &rules),
            "target/\n/.agents/kxen/\n.env\n"
        );
    }

    #[test]
    fn prepare_project_creates_base_and_ignore() {
        let workspace = std::env::temp_dir().join(format!("kxen-ignore-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(workspace.join(".git")).unwrap();
        let paths = super::super::paths::KxenPaths::project(&workspace);

        prepare_project(&paths).unwrap();

        assert!(paths.root().is_dir());
        assert_eq!(std::fs::read_to_string(workspace.join(".gitignore")).unwrap(), "/.agents/kxen/\n");
        std::fs::remove_dir_all(workspace).ok();
    }

    #[test]
    fn prepare_nested_workspace_inside_git_tree_creates_local_ignore() {
        let repository = std::env::temp_dir().join(format!("kxen-ignore-nested-{}", uuid::Uuid::new_v4()));
        let workspace = repository.join("packages/app");
        std::fs::create_dir_all(repository.join(".git")).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        let paths = super::super::paths::KxenPaths::project(&workspace);

        prepare_project(&paths).unwrap();

        assert_eq!(std::fs::read_to_string(workspace.join(".gitignore")).unwrap(), "/.agents/kxen/\n");
        std::fs::remove_dir_all(repository).ok();
    }

    #[test]
    fn prepare_global_user_scope_ignores_only_kxen_subtree() {
        let home = std::env::temp_dir().join(format!("kxen-user-ignore-{}", uuid::Uuid::new_v4()));
        let paths = super::super::paths::KxenPaths::global_in(&home);

        prepare_user(&paths).unwrap();

        assert!(paths.root().is_dir());
        assert_eq!(std::fs::read_to_string(home.join(".agents/.gitignore")).unwrap(), "/kxen/\n");
        std::fs::remove_dir_all(home).ok();
    }
}
