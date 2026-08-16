//! [sandbox] 配置：默认值、显式覆盖、0 回落缺省、项目配置不得放宽沙箱边界。

use super::*;

#[test]
fn sandbox_defaults_preserve_builtin_limits() {
    let config = SandboxConfig::default();
    assert_eq!(config.workflow_timeout(), std::time::Duration::from_secs(600));
    assert_eq!(config.memory_limit(), 64 * 1024 * 1024);
    assert_eq!(config.dynamic_tool_timeout(), std::time::Duration::from_secs(300));
    assert_eq!(config.dynamic_tool_max_implementation_chars(), 20_000);
    // 0 与缺省同义（同 approval_timeout_seconds / checkpoint_keep 口径）：解析值相同
    let zeroed = SandboxConfig {
        workflow_timeout_seconds: Some(0),
        memory_limit_mb: Some(0),
        dynamic_tool_timeout_seconds: Some(0),
        dynamic_tool_max_implementation_chars: Some(0),
    };
    assert_eq!(zeroed.workflow_timeout(), config.workflow_timeout());
    assert_eq!(zeroed.memory_limit(), config.memory_limit());
    assert_eq!(zeroed.dynamic_tool_timeout(), config.dynamic_tool_timeout());
    assert_eq!(zeroed.dynamic_tool_max_implementation_chars(), config.dynamic_tool_max_implementation_chars());
}

#[test]
fn sandbox_explicit_values_load_and_resolve() {
    let root = std::env::temp_dir().join(format!("kxen-config-sandbox-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture root");
    let path = root.join("config.toml");
    std::fs::write(
        &path,
        "[sandbox]\nworkflow_timeout_seconds = 1200\nmemory_limit_mb = 128\ndynamic_tool_timeout_seconds = 60\ndynamic_tool_max_implementation_chars = 5000\n",
    )
    .expect("write sandbox config");
    let config = crate::core::config::Config::load(&path, None).expect("load sandbox config");
    assert_eq!(config.sandbox.workflow_timeout(), std::time::Duration::from_secs(1200));
    assert_eq!(config.sandbox.memory_limit(), 128 * 1024 * 1024);
    assert_eq!(config.sandbox.dynamic_tool_timeout(), std::time::Duration::from_secs(60));
    assert_eq!(config.sandbox.dynamic_tool_max_implementation_chars(), 5000);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn sandbox_is_user_only_not_project_overridable() {
    let root = std::env::temp_dir().join(format!("kxen-config-sandbox-scope-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture root");
    let user = root.join("user.toml");
    let project = root.join("project.toml");
    std::fs::write(&user, "[sandbox]\ndynamic_tool_timeout_seconds = 60\n").expect("write user config");
    std::fs::write(&project, "[sandbox]\ndynamic_tool_timeout_seconds = 3600\n").expect("write project config");
    let error = crate::core::config::Config::load(&user, Some(&project)).expect_err("project must not widen sandbox limits").to_string();
    assert!(error.contains("user-only"), "{error}");
    std::fs::remove_dir_all(root).ok();
}
