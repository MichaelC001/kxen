use std::sync::Arc;

use super::runner_support::{ensure_private_dir, is_sensitive_child_env};
use super::*;

#[test]
fn sensitive_environment_names_are_filtered_by_default() {
    assert!(is_sensitive_child_env("GH_TOKEN"));
    assert!(is_sensitive_child_env("AWS_SHARED_CREDENTIALS_FILE"));
    assert!(is_sensitive_child_env("AWS_CONFIG_FILE"));
    assert!(is_sensitive_child_env("CLOUDSDK_CONFIG"));
    assert!(is_sensitive_child_env("GNUPGHOME"));
    assert!(is_sensitive_child_env("SSH_AUTH_SOCK"));
    assert!(is_sensitive_child_env("GITHUB_ENV"));
    assert!(is_sensitive_child_env("GITHUB_OUTPUT"));
    assert!(is_sensitive_child_env("GITHUB_PATH"));
    assert!(!is_sensitive_child_env("CI"));
    assert!(!is_sensitive_child_env("GPG_TTY"));
}

#[test]
fn tool_subprocess_credentials_require_a_consumed_private_auth_file() {
    let root = std::env::temp_dir().join(format!("kxen-dcp-one-shot-auth-{}", uuid::Uuid::new_v4()));
    let config = root.join("config.toml");
    let auth = root.join("one-shot-auth.json");
    let policy = root.join("policy.json");
    ensure_private_dir(&root).unwrap();
    std::fs::write(&config, "[roles.execution]\nprovider = \"xai\"\nmodel = \"grok-test\"\n").unwrap();
    std::fs::write(&policy, r#"{"allowShell":true,"allowedCapabilities":["read","exec"]}"#).unwrap();
    std::fs::write(&auth, r#"{"xai":{"type":"api","key":"one-shot-secret"}}"#).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&auth, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    let options = |consume_auth_file| DcpRuntimeOptions {
        data_dir: root.join(if consume_auth_file { "consumed-state" } else { "persistent-state" }),
        config_file: config.clone(),
        auth_file: auth.clone(),
        consume_auth_file,
        policy_file: Some(policy.clone()),
        event_format: DcpEventFormat::Jsonl,
        allow_shell: false,
        allow_mcp: false,
        pass_env: Vec::new(),
    };
    let error = DcpRuntime::new(options(false), Arc::new(|_| {})).err().expect("persistent credentials must fail closed");
    assert!(error.contains("requires an explicit --auth-file and --consume-auth-file"));
    assert!(auth.exists());

    std::fs::write(&policy, r#"{"allowMcp":true,"allowedCapabilities":["mcp__github__issue_read"]}"#).unwrap();
    let error = DcpRuntime::new(options(false), Arc::new(|_| {})).err().expect("MCP subprocess credentials must fail closed");
    assert!(error.contains("requires an explicit --auth-file and --consume-auth-file"));
    assert!(auth.exists());

    std::fs::write(&policy, r#"{"allowShell":true,"allowedCapabilities":["read","exec"]}"#).unwrap();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let runtime = DcpRuntime::new(options(true), Arc::new(|_| {})).expect("private one-shot credential");
        assert!(!auth.exists(), "credential file must be unlinked before the runtime is returned");
        assert!(matches!(
            runtime.auth_store.get("xai"),
            Some(crate::auth::credential::CredentialKind::Api { key, .. }) if key == "one-shot-secret"
        ));
        drop(runtime);
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let error = DcpRuntime::new(options(true), Arc::new(|_| {})).err().expect("unsupported process isolation must fail closed");
        assert!(error.contains("require Linux or macOS process isolation"));
        assert!(auth.exists(), "unsupported execution must not consume the credential file");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(unix)]
fn one_shot_auth_rejects_group_readable_files_without_consuming_them() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!("kxen-dcp-public-auth-{}", uuid::Uuid::new_v4()));
    let config = root.join("config.toml");
    let auth = root.join("public-auth.json");
    ensure_private_dir(&root).unwrap();
    std::fs::write(&config, "[roles.execution]\nprovider = \"xai\"\nmodel = \"grok-test\"\n").unwrap();
    std::fs::write(&auth, r#"{"xai":{"type":"api","key":"must-remain-private"}}"#).unwrap();
    std::fs::set_permissions(&auth, std::fs::Permissions::from_mode(0o640)).unwrap();
    let error = DcpRuntime::new(
        DcpRuntimeOptions {
            data_dir: root.join("state"),
            config_file: config,
            auth_file: auth.clone(),
            consume_auth_file: true,
            policy_file: None,
            event_format: DcpEventFormat::Jsonl,
            allow_shell: false,
            allow_mcp: false,
            pass_env: Vec::new(),
        },
        Arc::new(|_| {}),
    )
    .err()
    .expect("public auth file must fail closed");
    assert!(error.contains("must not be accessible by group or other users"));
    assert!(auth.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(unix)]
fn one_shot_auth_rejects_symlinks_and_multiple_hard_links() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let root = std::env::temp_dir().join(format!("kxen-dcp-linked-auth-{}", uuid::Uuid::new_v4()));
    let config = root.join("config.toml");
    let auth = root.join("auth.json");
    let linked = root.join("linked.json");
    ensure_private_dir(&root).unwrap();
    std::fs::write(&config, "").unwrap();
    std::fs::write(&auth, "{}").unwrap();
    std::fs::set_permissions(&auth, std::fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&auth, &linked).unwrap();

    let options = |auth_file| DcpRuntimeOptions {
        data_dir: root.join("state"),
        config_file: config.clone(),
        auth_file,
        consume_auth_file: true,
        policy_file: None,
        event_format: DcpEventFormat::Jsonl,
        allow_shell: false,
        allow_mcp: false,
        pass_env: Vec::new(),
    };
    let error = DcpRuntime::new(options(linked.clone()), Arc::new(|_| {})).err().expect("symlink must fail closed");
    assert!(error.contains("open private one-shot auth file"));
    assert!(auth.exists());

    std::fs::remove_file(&linked).unwrap();
    std::fs::hard_link(&auth, &linked).unwrap();
    let error = DcpRuntime::new(options(auth.clone()), Arc::new(|_| {})).err().expect("multiple hard links must fail closed");
    assert!(error.contains("must have exactly one hard link"));
    assert!(auth.exists());
    assert!(linked.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn linux_tool_subprocess_cannot_read_orchestrator_environment() {
    let root = std::env::temp_dir().join(format!("kxen-dcp-proc-isolation-{}", uuid::Uuid::new_v4()));
    let config = root.join("config.toml");
    ensure_private_dir(&root).unwrap();
    std::fs::write(&config, "").unwrap();

    let runtime = DcpRuntime::new(
        DcpRuntimeOptions {
            data_dir: root.join("state"),
            config_file: config,
            auth_file: root.join("auth.json"),
            consume_auth_file: false,
            policy_file: None,
            event_format: DcpEventFormat::Jsonl,
            allow_shell: true,
            allow_mcp: false,
            pass_env: Vec::new(),
        },
        Arc::new(|_| {}),
    )
    .expect("Linux process isolation");

    let output = std::process::Command::new("sh").args(["-c", "cat /proc/$PPID/environ"]).output().expect("run inspection probe");
    assert!(!output.status.success(), "tool subprocess unexpectedly read the orchestrator environment");
    assert!(output.stdout.is_empty(), "inspection probe returned environment bytes");

    drop(runtime);
    std::fs::remove_dir_all(root).unwrap();
}
