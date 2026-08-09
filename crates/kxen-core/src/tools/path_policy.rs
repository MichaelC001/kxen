//! Agent tool path boundary.
//!
//! Every model-controlled filesystem path is resolved here before it reaches a
//! file, search, LSP, shell, or background-task implementation. The boundary is
//! the canonical Workspace root plus explicit paths selected through the native
//! picker for the current Session. Credential locations are never grantable.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct ResolvedPath {
    absolute: PathBuf,
    authority_root: PathBuf,
    relative: PathBuf,
    authority: Arc<cap_std::fs::Dir>,
}

impl std::fmt::Debug for ResolvedPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedPath")
            .field("absolute", &self.absolute)
            .field("authority_root", &self.authority_root)
            .field("relative", &self.relative)
            .finish_non_exhaustive()
    }
}

impl PartialEq for ResolvedPath {
    fn eq(&self, other: &Self) -> bool {
        self.absolute == other.absolute && self.authority_root == other.authority_root && self.relative == other.relative
    }
}

impl Eq for ResolvedPath {}

impl ResolvedPath {
    pub fn as_path(&self) -> &Path {
        &self.absolute
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.absolute
    }

    pub fn metadata(&self) -> std::io::Result<cap_std::fs::Metadata> {
        self.authority.metadata(&self.relative)
    }

    pub fn open(&self) -> std::io::Result<cap_std::fs::File> {
        self.authority.open(&self.relative)
    }

    pub fn read_dir(&self) -> std::io::Result<cap_std::fs::ReadDir> {
        self.authority.read_dir(&self.relative)
    }

    pub fn read_optional_capped(&self, max_bytes: usize) -> std::io::Result<Option<String>> {
        let mut file = match self.authority.open(&self.relative) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let size = file.metadata()?.len();
        if size > max_bytes as u64 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("file exceeds {max_bytes} byte snapshot cap")));
        }
        let mut bytes = Vec::with_capacity(size as usize);
        std::io::Read::by_ref(&mut file).take((max_bytes + 1) as u64).read_to_end(&mut bytes)?;
        if bytes.len() > max_bytes {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("file exceeds {max_bytes} byte snapshot cap")));
        }
        String::from_utf8(bytes).map(Some).map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    /// 同一 capability 内写临时文件再 rename，目录句柄把所有路径解析限制在已授权 root。
    pub fn write_atomic(&self, bytes: &[u8]) -> std::io::Result<()> {
        let parent = self.relative.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
        self.authority.create_dir_all(parent)?;
        let parent_dir = self.authority.open_dir(parent)?;
        let file_name =
            self.relative.file_name().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "target has no file name"))?;
        let existing_permissions = parent_dir.metadata(file_name).ok().map(|metadata| metadata.permissions());
        let temporary = format!(".kxen-write-{}.tmp", uuid::Uuid::new_v4());
        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = parent_dir.open_with(&temporary, &options)?;
        if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
            parent_dir.remove_file(&temporary).ok();
            return Err(error);
        }
        if let Some(permissions) = existing_permissions
            && let Err(error) = file.set_permissions(permissions)
        {
            parent_dir.remove_file(&temporary).ok();
            return Err(error);
        }
        drop(file);
        if let Err(error) = parent_dir.rename(&temporary, &parent_dir, file_name) {
            parent_dir.remove_file(&temporary).ok();
            return Err(error);
        }
        sync_parent_dir(&parent_dir, &self.authority_root.join(parent))
    }

    /// 先用 anchored rename 把目标原子移到随机同根暂存名，再交给系统 Trash。
    /// 最终 path API 只看到已隔离的随机名，攻击者无法把原始 leaf 换成 Workspace 外目标。
    pub fn move_to_trash(&self) -> Result<(), String> {
        let file_name = self.relative.file_name().ok_or_else(|| "cannot trash authority root".to_string())?;
        let staged_name = format!(".{}.kxen-trash-{}", file_name.to_string_lossy(), uuid::Uuid::new_v4());
        let staged_relative = self.relative.with_file_name(staged_name);
        self.authority
            .rename(&self.relative, &self.authority, &staged_relative)
            .map_err(|error| format!("stage {}: {error}", self.absolute.display()))?;
        if let Err(error) = verify_open_root(&self.authority_root, &self.authority) {
            self.authority.rename(&staged_relative, &self.authority, &self.relative).ok();
            return Err(format!("authority root changed before trash: {error}"));
        }
        let staged_absolute = self.authority_root.join(&staged_relative);
        if let Err(error) = trash::delete(&staged_absolute) {
            let rollback = self.authority.rename(&staged_relative, &self.authority, &self.relative);
            return match rollback {
                Ok(()) => Err(format!("trash {}: {error}", self.absolute.display())),
                Err(rollback_error) => {
                    Err(format!("trash {}: {error}; staged item recovery failed: {rollback_error}", self.absolute.display()))
                }
            };
        }
        Ok(())
    }
}

/// rename 落盘后 fsync 父目录，保证目录项持久。
/// Linux 上 cap-std 的 Dir 是 O_PATH fd（cap-primitives target_o_path），对其 fsync 直接
/// EBADF；按绝对路径重开普通只读 fd 再 sync。重开只用于 fsync，授权边界已由 capability
/// 内的 open/rename 保证，不扩大可写范围。
#[cfg(target_os = "linux")]
fn sync_parent_dir(_dir: &cap_std::fs::Dir, absolute: &Path) -> std::io::Result<()> {
    std::fs::File::open(absolute)?.sync_all()
}

#[cfg(not(target_os = "linux"))]
fn sync_parent_dir(dir: &cap_std::fs::Dir, _absolute: &Path) -> std::io::Result<()> {
    dir.try_clone()?.into_std_file().sync_all()
}

/// Resolve a model-provided path against a Workspace and enforce the host
/// boundary. Nonexistent write targets are resolved through their nearest
/// existing ancestor so `..` and symlink escapes cannot hide in new paths.
pub fn resolve(input: &str, workspace: &Path, grants: &HashSet<PathBuf>) -> Result<ResolvedPath, String> {
    let workspace = canonicalize_existing(workspace).map_err(|e| format!("workspace path unavailable: {e}"))?;
    let expanded = expand_home(input)?;
    let candidate = if expanded.is_absolute() { expanded } else { workspace.join(expanded) };
    let candidate = canonicalize_lenient(&candidate)?;

    if let Some(reason) = sensitive_reason(&candidate) {
        return Err(format!("path denied: {reason}"));
    }
    if let crate::tools::safety::Verdict::Deny { rule_id, reason, .. } =
        crate::tools::safety::guard_path(&candidate.to_string_lossy(), &workspace.to_string_lossy())
    {
        return Err(format!("path denied by {rule_id}: {reason}"));
    }
    let authority_root = if candidate.starts_with(&workspace) {
        workspace
    } else {
        grant_root(&candidate, grants)
            .ok_or_else(|| format!("path escapes workspace: {} (workspace: {})", candidate.display(), workspace.display()))?
    };
    let relative = candidate
        .strip_prefix(&authority_root)
        .map_err(|_| format!("path is outside authority root: {}", candidate.display()))?
        .to_path_buf();
    let authority = open_verified_root(&authority_root)?;
    Ok(ResolvedPath { absolute: candidate, authority_root, relative, authority: Arc::new(authority) })
}

/// Canonicalize a path that may not exist yet. Existing ancestors are resolved
/// through the filesystem, then missing normal components are appended.
pub fn canonicalize_lenient(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().map_err(|e| e.to_string())?.join(path) };
    let normalized = lexical_normalize(&absolute)?;
    let mut cursor = normalized.as_path();
    let mut missing = Vec::new();
    while !cursor.exists() {
        let name = cursor.file_name().ok_or_else(|| format!("path has no existing ancestor: {}", path.display()))?;
        missing.push(name.to_os_string());
        cursor = cursor.parent().ok_or_else(|| format!("path has no existing ancestor: {}", path.display()))?;
    }
    let mut resolved = canonicalize_existing(cursor).map_err(|e| format!("canonicalize {}: {e}", cursor.display()))?;
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn canonicalize_existing(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

fn lexical_normalize(path: &Path) -> Result<PathBuf, String> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err(format!("path escapes filesystem root: {}", path.display()));
                }
            }
            Component::Normal(part) => out.push(part),
        }
    }
    Ok(out)
}

fn expand_home(input: &str) -> Result<PathBuf, String> {
    if input == "~" {
        return dirs::home_dir().ok_or("home directory unavailable".into());
    }
    if let Some(rest) = input.strip_prefix("~/") {
        return dirs::home_dir().map(|home| home.join(rest)).ok_or("home directory unavailable".into());
    }
    Ok(PathBuf::from(input))
}

fn grant_root(candidate: &Path, grants: &HashSet<PathBuf>) -> Option<PathBuf> {
    grants.iter().find_map(|grant| {
        let Ok(grant) = canonicalize_lenient(grant) else {
            return None;
        };
        if grant.is_dir() && candidate.starts_with(&grant) {
            Some(grant)
        } else if candidate == grant {
            grant.parent().map(Path::to_path_buf)
        } else {
            None
        }
    })
}

fn open_verified_root(root: &Path) -> Result<cap_std::fs::Dir, String> {
    let authority = cap_std::fs::Dir::open_ambient_dir(root, cap_std::ambient_authority())
        .map_err(|error| format!("open authority root {}: {error}", root.display()))?;
    verify_open_root(root, &authority)?;
    Ok(authority)
}

fn verify_open_root(root: &Path, authority: &cap_std::fs::Dir) -> Result<(), String> {
    // 身份核对依赖 dev/ino,只有 unix 提供;Windows 由 open_ambient_dir 的成功打开保证存在性
    #[cfg(unix)]
    {
        let path_metadata = std::fs::metadata(root).map_err(|error| format!("stat {}: {error}", root.display()))?;
        let handle_metadata = authority.metadata(".").map_err(|error| format!("stat open root {}: {error}", root.display()))?;
        use cap_std::fs::MetadataExt as CapMetadataExt;
        use std::os::unix::fs::MetadataExt as StdMetadataExt;
        if StdMetadataExt::dev(&path_metadata) != CapMetadataExt::dev(&handle_metadata)
            || StdMetadataExt::ino(&path_metadata) != CapMetadataExt::ino(&handle_metadata)
        {
            return Err(format!("authority root identity changed: {}", root.display()));
        }
    }
    #[cfg(not(unix))]
    let _ = (root, authority);
    Ok(())
}

fn sensitive_reason(candidate: &Path) -> Option<String> {
    if sensitive_root_matches(candidate) {
        return Some(format!("credential or application data is protected: {}", candidate.display()));
    }

    let file_name = candidate.file_name().and_then(|name| name.to_str()).unwrap_or_default();
    if [".netrc", ".npmrc", ".pypirc", ".git-credentials"].contains(&file_name) {
        return Some(format!("credential file is protected: {}", candidate.display()));
    }
    let extension = candidate.extension().and_then(|ext| ext.to_str()).unwrap_or_default();
    if ["p8", "p12", "pfx", "keychain", "keychain-db"].iter().any(|sensitive| extension.eq_ignore_ascii_case(sensitive)) {
        return Some(format!("private key or keychain file is protected: {}", candidate.display()));
    }
    None
}

fn build_sensitive_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        crate::core::paths::config_dir(),
        crate::core::paths::data_dir(),
        crate::core::paths::cache_dir(),
        crate::core::paths::auth_file(),
        crate::mcp::oauth_store::store_path(),
    ];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Library/Keychains"));
        for name in [".ssh", ".gnupg", ".aws", ".kube", ".docker", ".codex", ".claude", ".grok", ".kimi-code"] {
            roots.push(home.join(name));
        }
    }
    roots
}

fn sensitive_root_matches(candidate: &Path) -> bool {
    sensitive_root_matches_in(candidate, &build_sensitive_roots())
}

fn sensitive_root_matches_in(candidate: &Path, roots: &[PathBuf]) -> bool {
    // 敏感目录可能在进程运行中创建或被替换为 symlink。每次按当前文件系统解析，
    // 不能缓存 canonical root，否则旧 target 会让新 target 逃过保护。
    roots.iter().any(|root| {
        let root = canonicalize_lenient(root).unwrap_or_else(|_| root.clone());
        candidate == root || candidate.starts_with(root)
    })
}

#[cfg(test)]
#[path = "path_policy/tests.rs"]
mod tests;
