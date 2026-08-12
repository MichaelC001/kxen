use std::path::{Path, PathBuf};

use crate::agent::agent_loop::ResourcePathScope;
use crate::bot::{ResourceAccess, ResourcePolicy};
use crate::core::identity::{ContentHash, ResourceId};

pub fn workspace_id(path: &Path) -> Result<ResourceId, String> {
    let canonical = std::fs::canonicalize(path).map_err(|error| format!("workspace unavailable: {error}"))?;
    let hash = ContentHash::from_bytes(canonical.to_string_lossy().as_bytes());
    ResourceId::parse(format!("workspace_{}", &hash.as_str()["sha256:".len()..]))
}

pub(super) fn resolve_paths(policy: &ResourcePolicy, workspace: &Path, sandbox: &Path) -> Result<ResourcePathScope, String> {
    if policy.workspaces.is_empty() {
        std::fs::create_dir_all(sandbox).map_err(|error| format!("create local Bot sandbox: {error}"))?;
        return Ok(ResourcePathScope { read: vec![sandbox.to_path_buf()], write: vec![sandbox.to_path_buf()], execute: Vec::new() });
    }
    if policy.workspaces.len() != 1 {
        return Err("a BotRun must resolve exactly one Workspace binding".into());
    }
    let binding = &policy.workspaces[0];
    if binding.workspace_id != workspace_id(workspace)? {
        return Err(format!("Bot Workspace binding does not match active Workspace: {}", binding.workspace_id));
    }
    let mut scope = ResourcePathScope::default();
    for grant in &binding.paths {
        let root = checked_root(workspace, &grant.relative_path, grant.access)?;
        match grant.access {
            ResourceAccess::Read => scope.read.push(root),
            ResourceAccess::Write => {
                scope.read.push(root.clone());
                scope.write.push(root);
            }
            ResourceAccess::Execute => {
                scope.read.push(root.clone());
                scope.write.push(root.clone());
                scope.execute.push(root);
            }
        }
    }
    dedupe(&mut scope.read);
    dedupe(&mut scope.write);
    dedupe(&mut scope.execute);
    Ok(scope)
}

fn checked_root(workspace: &Path, relative: &str, access: ResourceAccess) -> Result<PathBuf, String> {
    let workspace = std::fs::canonicalize(workspace).map_err(|error| error.to_string())?;
    let root = crate::tools::path_policy::canonicalize_lenient(&workspace.join(relative))?;
    if !root.starts_with(&workspace) {
        return Err(format!("Bot resource escapes Workspace: {relative}"));
    }
    if !root.exists() {
        if access == ResourceAccess::Read {
            return Err(format!("read-only Bot resource root does not exist: {}", root.display()));
        }
        std::fs::create_dir_all(&root).map_err(|error| format!("create Bot resource root {}: {error}", root.display()))?;
    }
    Ok(root)
}

fn dedupe(paths: &mut Vec<PathBuf>) {
    paths.sort();
    paths.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::{PathGrantSpec, WorkspaceGrantSpec};

    fn policy(workspace: &Path, relative_path: &str, access: ResourceAccess) -> ResourcePolicy {
        ResourcePolicy {
            workspaces: vec![WorkspaceGrantSpec {
                workspace_id: workspace_id(workspace).unwrap(),
                paths: vec![PathGrantSpec { relative_path: relative_path.into(), access }],
            }],
            connectors: Default::default(),
        }
    }

    #[test]
    fn read_grant_never_creates_a_missing_workspace_path() {
        let workspace = std::env::temp_dir().join(format!("kxen-bot-policy-read-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&workspace).unwrap();
        let missing = workspace.join("missing");
        let result = resolve_paths(&policy(&workspace, "missing", ResourceAccess::Read), &workspace, &workspace.join("sandbox"));
        assert!(result.is_err());
        assert!(!missing.exists());
        std::fs::remove_dir_all(workspace).ok();
    }

    #[test]
    fn write_grant_may_create_its_declared_workspace_path() {
        let workspace = std::env::temp_dir().join(format!("kxen-bot-policy-write-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&workspace).unwrap();
        let created = workspace.join("generated");
        let scope = resolve_paths(&policy(&workspace, "generated", ResourceAccess::Write), &workspace, &workspace.join("sandbox")).unwrap();
        assert!(created.is_dir());
        assert_eq!(scope.write, vec![std::fs::canonicalize(created).unwrap()]);
        std::fs::remove_dir_all(workspace).ok();
    }
}
