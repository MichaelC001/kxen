use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::agent::agent_loop::{PersistTurn, ResourcePathScope};

use super::{DcpAgentOutput, DcpAgentOutputFormat, DcpRunState, DcpRunToolJournal, DcpRuntimeOptions, DcpRuntimePolicy};

pub(super) fn same_message_content(left: &crate::core::session::Message, right: &crate::core::session::Message) -> Result<bool, String> {
    let left = serde_json::to_value((&left.role, &left.parts, &left.model)).map_err(|error| error.to_string())?;
    let right = serde_json::to_value((&right.role, &right.parts, &right.model)).map_err(|error| error.to_string())?;
    Ok(left == right)
}

pub(super) fn turn_persister(
    sessions_dir: &Path,
    session_id: &str,
    run_id: &str,
    model: crate::llm::ModelRef,
    journal: Arc<DcpRunToolJournal>,
) -> PersistTurn {
    let sessions_dir = sessions_dir.to_path_buf();
    let session_id = session_id.to_string();
    let run_id = run_id.to_string();
    Arc::new(move |turn, parts| {
        let mut message = crate::core::session::new_message(&session_id, crate::core::session::Role::Assistant, parts.clone());
        message.id = format!("{run_id}_turn_{turn}");
        message.model = Some(model.clone());
        crate::core::session::append_message_idempotent_durable(&sessions_dir, &message).map_err(|error| error.to_string())?;
        journal.settle_parts(&parts)
    })
}

pub(super) fn recover_known_outcomes(sessions_dir: &Path, run: &DcpRunState, journal: &DcpRunToolJournal) -> Result<(), String> {
    let operations = journal.unrecorded_outcomes();
    if operations.is_empty() {
        return Ok(());
    }
    let parts = operations
        .iter()
        .map(|operation| crate::core::session::Part::ToolCall {
            name: operation.tool_name.clone(),
            input: serde_json::from_str(&operation.arguments_json).unwrap_or_else(|_| serde_json::json!(operation.arguments_json)),
            output: operation.output.clone().unwrap_or_default().into(),
            args: serde_json::from_str(&operation.arguments_json).ok(),
            id: operation.call_ids.last().cloned(),
            // journal 记的是 before/after 实测边界，恢复落盘如实带上
            started_at: Some(operation.started_at_ms),
            finished_at: Some(operation.updated_at_ms),
        })
        .collect();
    let mut message = crate::core::session::new_message(&run.session_id, crate::core::session::Role::Assistant, parts);
    message.id = format!("{}_recovered_tools", run.run_id);
    message.model = run.model.clone();
    crate::core::session::append_message_idempotent_durable(sessions_dir, &message)
        .map_err(|error| format!("persist recovered DCP tool outcomes: {error}"))?;
    journal.settle_operations(&operations.into_iter().map(|operation| operation.operation_id).collect::<Vec<_>>())
}

pub(super) fn validate_agent_output(output: &DcpAgentOutput, text: &str) -> Result<(), String> {
    if output.format == DcpAgentOutputFormat::Text {
        return Ok(());
    }
    let value: serde_json::Value =
        serde_json::from_str(text.trim()).map_err(|error| format!("DCPAgent JSON output is invalid: {error}"))?;
    let object = value.as_object().ok_or("DCPAgent JSON output must be one object")?;
    let missing = output.required_fields.iter().filter(|field| !object.contains_key(field.as_str())).cloned().collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!("DCPAgent JSON output is missing required fields: {}", missing.join(", ")));
    }
    Ok(())
}

pub(super) fn filtered_child_environment(
    policy: &DcpRuntimePolicy,
    tool_home: &Path,
) -> Result<crate::agent::agent_loop::ChildEnvironment, String> {
    policy.validate()?;
    let explicit = policy.pass_env.iter().map(|name| name.to_ascii_uppercase()).collect::<std::collections::BTreeSet<_>>();
    let mut environment = std::env::vars_os()
        .filter(|(name, _)| {
            let name = name.to_string_lossy();
            let upper = name.to_ascii_uppercase();
            !is_sensitive_child_env(&upper) || explicit.contains(&upper)
        })
        .collect::<BTreeMap<_, _>>();
    environment.insert(std::ffi::OsString::from("HOME"), tool_home.as_os_str().to_os_string());
    environment.insert(std::ffi::OsString::from("USERPROFILE"), tool_home.as_os_str().to_os_string());
    environment.insert(std::ffi::OsString::from("XDG_CONFIG_HOME"), tool_home.join("config").into_os_string());
    environment.insert(std::ffi::OsString::from("XDG_DATA_HOME"), tool_home.join("data").into_os_string());
    environment.insert(std::ffi::OsString::from("XDG_STATE_HOME"), tool_home.join("state").into_os_string());
    Ok(Arc::new(environment))
}

pub(super) fn is_sensitive_child_env(upper: &str) -> bool {
    if super::runtime_policy::is_provider_credential_env(upper) {
        return true;
    }
    if matches!(
        upper,
        "AWS_CONFIG_FILE"
            | "AWS_SHARED_CREDENTIALS_FILE"
            | "AZURE_CONFIG_DIR"
            | "CLOUDSDK_CONFIG"
            | "DOCKER_CONFIG"
            | "GIT_ASKPASS"
            | "SSH_ASKPASS"
            | "SSH_AUTH_SOCK"
            | "SSH_AGENT_PID"
            | "GPG_AGENT_INFO"
            | "GNUPGHOME"
            | "GIT_CONFIG_GLOBAL"
            | "GIT_CONFIG_SYSTEM"
            | "GITHUB_ENV"
            | "GITHUB_OUTPUT"
            | "GITHUB_PATH"
            | "GITHUB_STATE"
            | "GITHUB_STEP_SUMMARY"
            | "KUBECONFIG"
            | "NETRC"
            | "NPM_CONFIG_USERCONFIG"
    ) {
        return true;
    }
    ["TOKEN", "SECRET", "PASSWORD", "PASSWD", "CREDENTIAL", "PRIVATE_KEY", "ACCESS_KEY", "API_KEY"]
        .iter()
        .any(|marker| upper.contains(marker))
}

pub(super) fn workspace_scope(workspace: &Path, capabilities: &[String]) -> ResourcePathScope {
    let read = capabilities.iter().any(|capability| matches!(capability.as_str(), "read" | "glob" | "grep" | "lsp"));
    let write = capabilities.iter().any(|capability| matches!(capability.as_str(), "edit" | "write" | "delete" | "worktree"));
    let execute = capabilities.iter().any(|capability| matches!(capability.as_str(), "exec" | "task"));
    ResourcePathScope {
        read: (read || write || execute).then(|| workspace.to_path_buf()).into_iter().collect(),
        write: (write || execute).then(|| workspace.to_path_buf()).into_iter().collect(),
        execute: execute.then(|| workspace.to_path_buf()).into_iter().collect(),
    }
}

pub(super) struct DcpAutoApprove {
    audit_path: PathBuf,
    category: &'static str,
}

impl DcpAutoApprove {
    pub(super) fn new(audit_path: PathBuf, category: &'static str) -> Self {
        Self { audit_path, category }
    }
}

impl crate::tools::auto_approve::AutoApprove for DcpAutoApprove {
    fn try_auto_allow(&self, command: &str) -> Result<(), String> {
        let entry = serde_json::json!({
            "schemaVersion": 1,
            "category": self.category,
            "commandHash": crate::core::identity::ContentHash::from_bytes(command.as_bytes()),
            "allowedAtMs": crate::core::shared::now_ms(),
        });
        let mut bytes = serde_json::to_vec(&entry).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        crate::core::durability::append_synced(&self.audit_path, &bytes)
            .map_err(|error| format!("persist DCP shell approval audit: {error}"))
    }
}

pub(super) fn load_runtime_policy(options: &DcpRuntimeOptions) -> Result<DcpRuntimePolicy, String> {
    let mut policy = match &options.policy_file {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|error| format!("read DCP runtime policy {}: {error}", path.display()))?;
            serde_json::from_str(&text).map_err(|error| format!("parse DCP runtime policy {}: {error}", path.display()))?
        }
        None => DcpRuntimePolicy::default(),
    };
    policy.allow_shell |= options.allow_shell;
    policy.allow_mcp |= options.allow_mcp;
    policy.pass_env.extend(options.pass_env.iter().cloned());
    policy.pass_env.sort();
    policy.pass_env.dedup();
    policy.validate()?;
    Ok(policy)
}

pub(super) fn load_auth(path: &Path, consume: bool) -> Result<crate::auth::credential::AuthStore, String> {
    if consume {
        return consume_auth_file(path);
    }
    load_auth_with_env(path)
}

fn consume_auth_file(path: &Path) -> Result<crate::auth::credential::AuthStore, String> {
    for variable in provider_credential_variables() {
        if std::env::var_os(variable).is_some_and(|value| !value.is_empty()) {
            return Err(format!(
                "{variable} must be absent when --consume-auth-file is used; launch the agent in a separate credential-free step"
            ));
        }
    }
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(path)
            .map_err(|error| format!("open private one-shot auth file {}: {error}", path.display()))?;
        let metadata = file.metadata().map_err(|error| format!("inspect one-shot auth file {}: {error}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!("one-shot auth file must be a regular file: {}", path.display()));
        }
        if metadata.mode() & 0o077 != 0 {
            return Err(format!("one-shot auth file must not be accessible by group or other users: {}", path.display()));
        }
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(format!("one-shot auth file must be owned by the current user: {}", path.display()));
        }
        if metadata.nlink() != 1 {
            return Err(format!("one-shot auth file must have exactly one hard link: {}", path.display()));
        }
        file
    };
    #[cfg(not(unix))]
    let mut file = {
        let metadata =
            std::fs::symlink_metadata(path).map_err(|error| format!("inspect one-shot auth file {}: {error}", path.display()))?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(format!("one-shot auth file must be a regular file, not a symlink: {}", path.display()));
        }
        std::fs::File::open(path).map_err(|error| format!("open one-shot auth file {}: {error}", path.display()))?
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|error| format!("read one-shot auth file {}: {error}", path.display()))?;
    let store = serde_json::from_slice(&bytes).map_err(|error| format!("parse one-shot auth file {}: {error}", path.display()))?;
    std::fs::remove_file(path).map_err(|error| format!("unlink one-shot auth file {} before execution: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let metadata = file.metadata().map_err(|error| format!("verify consumed auth file {}: {error}", path.display()))?;
        if metadata.nlink() != 0 {
            return Err(format!("one-shot auth file changed while it was being consumed: {}", path.display()));
        }
    }
    Ok(store)
}

fn load_auth_with_env(path: &Path) -> Result<crate::auth::credential::AuthStore, String> {
    let mut store = crate::auth::credential::read_auth_file(path).map_err(|error| format!("load auth store: {error}"))?;
    let mappings = BTreeMap::from([
        ("ANTHROPIC_API_KEY", "anthropic"),
        ("OPENAI_API_KEY", "openai"),
        ("XAI_API_KEY", "xai"),
        ("GOOGLE_API_KEY", "google"),
        ("OPENROUTER_API_KEY", "openrouter"),
        ("GROQ_API_KEY", "groq"),
        ("MISTRAL_API_KEY", "mistral"),
        ("DEEPSEEK_API_KEY", "deepseek"),
    ]);
    for (variable, provider) in mappings {
        if let Ok(key) = std::env::var(variable)
            && !key.trim().is_empty()
        {
            store.insert(provider.into(), crate::auth::credential::CredentialKind::Api { key, region: None });
        }
    }
    Ok(store)
}

fn provider_credential_variables() -> [&'static str; 8] {
    [
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "XAI_API_KEY",
        "GOOGLE_API_KEY",
        "OPENROUTER_API_KEY",
        "GROQ_API_KEY",
        "MISTRAL_API_KEY",
        "DEEPSEEK_API_KEY",
    ]
}

pub(super) fn harden_tool_process_isolation() -> Result<bool, String> {
    static HARDENED: std::sync::OnceLock<Result<bool, String>> = std::sync::OnceLock::new();
    HARDENED
        .get_or_init(|| {
            #[cfg(target_os = "linux")]
            {
                // Tool subprocesses share the runner UID. Marking the orchestrator non-dumpable
                // prevents those children from reading its environment or memory through /proc/ptrace.
                let result = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
                if result != 0 {
                    return Err(format!("protect DCP orchestrator from tool subprocess inspection: {}", std::io::Error::last_os_error()));
                }
                Ok(true)
            }
            #[cfg(target_os = "macos")]
            {
                // PT_DENY_ATTACH is irreversible for this process and fails closed before tools start.
                let result = unsafe { libc::ptrace(libc::PT_DENY_ATTACH, 0, std::ptr::null_mut(), 0) };
                if result != 0 {
                    return Err(format!("protect DCP orchestrator from tool subprocess inspection: {}", std::io::Error::last_os_error()));
                }
                Ok(true)
            }
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            Ok(false)
        })
        .clone()
}

pub(super) fn ensure_private_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| format!("create {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("secure {}: {error}", path.display()))?;
    }
    Ok(())
}
