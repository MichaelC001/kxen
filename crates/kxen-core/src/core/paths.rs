//! Kxen 路径约定。用户级状态统一收敛到 `~/.agents/kxen`，项目级状态收敛到 `<workspace>/.agents/kxen`。

use std::path::{Path, PathBuf};

pub const APP_DIR: &str = "kxen";
pub const AGENTS_DIR: &str = ".agents";
pub const PROJECT_STATE_RELATIVE: &str = ".agents/kxen";

pub struct KxenPaths;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserPaths {
    root: PathBuf,
    ignore_root: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectPaths {
    workspace: PathBuf,
    root: PathBuf,
}

impl KxenPaths {
    pub fn user() -> UserPaths {
        if let Some(root) = std::env::var_os("KXEN_DATA_DIR") {
            return Self::custom(PathBuf::from(root));
        }
        #[cfg(test)]
        {
            return Self::custom(std::env::temp_dir().join(format!("kxen-unit-{}", std::process::id())).join(PROJECT_STATE_RELATIVE));
        }
        #[cfg(not(test))]
        Self::global()
    }

    pub fn global() -> UserPaths {
        Self::global_in(home_dir())
    }

    pub fn global_in(home: impl Into<PathBuf>) -> UserPaths {
        let ignore_root = home.into().join(AGENTS_DIR);
        UserPaths { root: ignore_root.join(APP_DIR), ignore_root: Some(ignore_root) }
    }

    pub fn custom(root: impl Into<PathBuf>) -> UserPaths {
        UserPaths { root: root.into(), ignore_root: None }
    }

    pub fn project(workspace: &Path) -> ProjectPaths {
        ProjectPaths { workspace: workspace.to_path_buf(), root: workspace.join(PROJECT_STATE_RELATIVE) }
    }

    pub fn is_runtime_namespace_entry(agents_root: &Path, candidate: &Path) -> bool {
        candidate == agents_root.join(APP_DIR)
    }

    pub fn contains_project_state(path: &str) -> bool {
        let parts = path.split(['/', '\\']).filter(|part| !part.is_empty()).collect::<Vec<_>>();
        parts.windows(2).any(|parts| parts[0].eq_ignore_ascii_case(AGENTS_DIR) && parts[1].eq_ignore_ascii_case(APP_DIR))
    }

    pub fn kanban_events_file(board_dir: &Path) -> PathBuf {
        board_dir.join("events.jsonl")
    }

    pub fn kanban_snapshot_file(board_dir: &Path) -> PathBuf {
        board_dir.join("snapshot.json")
    }

    pub fn kanban_lock_file(board_dir: &Path) -> PathBuf {
        board_dir.join("events.lock")
    }
}

impl UserPaths {
    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn ignore_root(&self) -> Option<&Path> {
        self.ignore_root.as_deref()
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn mcp_config_file(&self) -> PathBuf {
        self.root.join("mcp.json")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn auth_file(&self) -> PathBuf {
        std::env::var_os("KXEN_AUTH_FILE").map(PathBuf::from).unwrap_or_else(|| self.root.join("auth.json"))
    }

    pub fn goals_dir(&self) -> PathBuf {
        std::env::var_os("KXEN_GOALS_DIR").map(PathBuf::from).unwrap_or_else(|| self.root.join("goals"))
    }

    pub fn sessions_dir(&self) -> PathBuf {
        std::env::var_os("KXEN_SESSIONS_DIR").map(PathBuf::from).unwrap_or_else(|| self.root.join("sessions"))
    }

    pub fn bots_dir(&self) -> PathBuf {
        std::env::var_os("KXEN_BOTS_DIR").map(PathBuf::from).unwrap_or_else(|| self.root.join("bots"))
    }

    pub fn agent_state_dir(&self) -> PathBuf {
        self.root.join("agent")
    }

    pub fn teams_dir(&self) -> PathBuf {
        self.root.join("teams")
    }

    pub fn usage_attempts_dir(&self) -> PathBuf {
        self.root.join("usage-attempts")
    }

    pub fn workflow_journal(&self, run_id: &str) -> PathBuf {
        self.root.join("workflow-journals").join(format!("{run_id}.jsonl"))
    }

    pub fn composer_suggestion_cache(&self, digest: &str) -> PathBuf {
        self.root.join("composer-suggestions").join(digest).join("embedding-cache.json")
    }

    pub fn shadow_repo(&self, workspace_hash: &str) -> PathBuf {
        self.root.join("shadow").join(format!("{workspace_hash}.git"))
    }

    pub fn browser_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_dir().join(session_id).join("browser")
    }

    pub fn credential_consent_file(&self) -> PathBuf {
        self.root.join("credential-consent.json")
    }

    pub fn notifications_file(&self) -> PathBuf {
        self.root.join("notifications.json")
    }

    pub fn schedule_file(&self) -> PathBuf {
        self.root.join("schedule.json")
    }

    pub fn trusted_workspaces_file(&self) -> PathBuf {
        self.root.join("trusted.json")
    }

    pub fn usage_file(&self) -> PathBuf {
        self.root.join("usage.json")
    }

    pub fn usage_trend_file(&self) -> PathBuf {
        self.root.join("usage-trend.json")
    }

    pub fn diagnostics_dir(&self) -> PathBuf {
        self.root.join("diagnostics")
    }

    pub fn consolidation_attempts_dir(&self) -> PathBuf {
        self.root.join("consolidation-attempts")
    }

    pub fn consolidate_state_file(&self) -> PathBuf {
        self.root.join("consolidate.json")
    }

    pub fn embedding_cache_file(&self) -> PathBuf {
        self.root.join("embedding-cache.json")
    }

    pub fn knowledge_moves_dir(&self) -> PathBuf {
        self.root.join("knowledge-moves")
    }

    pub fn models_catalog_file(&self) -> PathBuf {
        self.root.join("models-catalog.json")
    }

    pub fn mcp_oauth_file(&self) -> PathBuf {
        self.root.join("mcp-oauth.json")
    }

    pub fn instance_lock_file(&self) -> PathBuf {
        self.root.join("instance.lock")
    }

    pub fn ensure_base_dirs(&self) -> Result<(), String> {
        ensure_private_dir(&self.root)?;
        ensure_private_dir(&self.cache_dir())
    }
}

impl ProjectPaths {
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn approval_rules_file(&self) -> PathBuf {
        self.root.join("approval-rules.json")
    }

    pub fn worktrees_dir(&self) -> PathBuf {
        self.root.join("worktrees")
    }

    pub fn worktree(&self, name: &str) -> PathBuf {
        self.worktrees_dir().join(name)
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.root.join("backups")
    }

    pub fn backup_path(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.backups_dir().join(relative).with_extension("kxen-bak")
    }

    pub fn kanban_dir(&self) -> PathBuf {
        self.root.join("kanban")
    }

    pub fn kanban_board(&self, board_id: &str) -> PathBuf {
        self.kanban_dir().join(board_id)
    }

    pub fn kanban_agents_dir(&self) -> PathBuf {
        self.kanban_dir().join("agents")
    }

    pub fn kanban_agent(&self, name: &str) -> PathBuf {
        self.kanban_agents_dir().join(format!("{name}.md"))
    }

    pub fn kanban_artifact_dir(&self, board_id: &str, card_id: &str) -> PathBuf {
        self.kanban_board(board_id).join("artifacts").join(card_id)
    }

    pub fn kanban_artifact_files_dir(&self, board_id: &str, card_id: &str) -> PathBuf {
        self.kanban_artifact_dir(board_id, card_id).join("files")
    }

    pub fn kanban_artifact_manifest_file(&self, board_id: &str, card_id: &str) -> PathBuf {
        self.kanban_artifact_dir(board_id, card_id).join("manifest.json")
    }

    pub fn kanban_turns_file(&self, board_id: &str, run_id: &str) -> PathBuf {
        self.kanban_board(board_id).join("runs").join(format!("{}.turns.jsonl", portable_component(run_id)))
    }

    pub fn ensure_base_dir(&self) -> Result<(), String> {
        ensure_private_dir(&self.root)
    }
}

fn portable_component(value: &str) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            encoded.push(char::from(byte));
        } else {
            write!(&mut encoded, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    encoded
}

fn ensure_private_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| format!("create {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("chmod 0700 {}: {error}", path.display()))?;
    }
    Ok(())
}

pub(crate) fn home_dir() -> PathBuf {
    dirs::home_dir()
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .or_else(|| {
            let drive = std::env::var_os("HOMEDRIVE")?;
            let path = std::env::var_os("HOMEPATH")?;
            Some(PathBuf::from(drive).join(path))
        })
        .or_else(|| std::env::current_dir().ok())
        .filter(|path| path.is_absolute())
        // 不能返回字面量 `~`：Rust 不会展开它。temp_dir 是所有支持平台都可用的最终绝对路径。
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests;
