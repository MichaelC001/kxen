use std::path::Path;

use crate::core::identity::ContentHash;

use super::{GitWorkspaceBinding, WorkspaceBinding};

impl WorkspaceBinding {
    pub fn capture(root: &Path) -> Result<Self, String> {
        let root = std::fs::canonicalize(root).map_err(|error| format!("canonicalize Workspace {}: {error}", root.display()))?;
        if !root.is_dir() {
            return Err(format!("Workspace is not a directory: {}", root.display()));
        }
        let git = capture_git(&root)?;
        let identity_material = git
            .as_ref()
            .and_then(|binding| binding.remote_hash.as_ref().map(|hash| hash.as_str().to_string()))
            .unwrap_or_else(|| root.to_string_lossy().into_owned());
        Ok(Self { root: root.to_string_lossy().into_owned(), identity: ContentHash::from_bytes(identity_material.as_bytes()), git })
    }

    pub fn verify(&self, candidate: &Path, allow_rebind: bool) -> Result<Self, String> {
        let captured = Self::capture(candidate)?;
        if captured.identity != self.identity {
            return Err(format!("Workspace identity mismatch for {}", candidate.display()));
        }
        if !allow_rebind && captured.root != self.root {
            return Err(format!("Session is bound to {}; pass --rebind-workspace to use {}", self.root, captured.root));
        }
        if let (Some(expected), Some(actual)) = (&self.git, &captured.git)
            && expected.branch.is_some()
            && actual.branch != expected.branch
        {
            return Err(format!(
                "Git branch mismatch: expected {}, got {}",
                expected.branch.as_deref().unwrap_or("detached"),
                actual.branch.as_deref().unwrap_or("detached")
            ));
        }
        Ok(captured)
    }
}

fn capture_git(root: &Path) -> Result<Option<GitWorkspaceBinding>, String> {
    let run = |args: &[&str]| -> Result<Option<String>, String> {
        let output =
            std::process::Command::new("git").arg("-C").arg(root).args(args).output().map_err(|error| format!("run git: {error}"))?;
        if !output.status.success() {
            return Ok(None);
        }
        Ok(Some(String::from_utf8_lossy(&output.stdout).trim().to_string()))
    };
    let Some(repository_root) = run(&["rev-parse", "--show-toplevel"])? else { return Ok(None) };
    let head = run(&["rev-parse", "HEAD"])?.ok_or("Git repository has no HEAD")?;
    let branch = run(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let remote_hash = run(&["config", "--get", "remote.origin.url"])?
        .filter(|remote| !remote.is_empty())
        .map(|remote| ContentHash::from_bytes(sanitize_remote(&remote).as_bytes()));
    Ok(Some(GitWorkspaceBinding { repository_root, remote_hash, branch, head }))
}

pub(super) fn sanitize_remote(remote: &str) -> String {
    if let Some((scheme, rest)) = remote.split_once("://")
        && let Some((_, host_path)) = rest.rsplit_once('@')
    {
        return format!("{scheme}://{host_path}");
    }
    remote.to_string()
}
