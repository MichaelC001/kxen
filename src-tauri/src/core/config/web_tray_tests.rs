//! [web]/[tray] section 的默认值、解析与边界测试（tests.rs 350 行门禁外溢）。

use super::*;

#[test]
fn web_and_tray_sections_default_when_absent() {
    let root = std::env::temp_dir().join(format!("kxen-config-web-tray-default-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture root");
    let path = root.join("config.toml");
    std::fs::write(&path, "[coding_rules]\nenabled = true\n").expect("write minimal config");
    let config = Config::load(&path, None).expect("load config without web/tray sections");
    assert!(config.web.enabled);
    assert_eq!(config.web.bind, "127.0.0.1");
    assert_eq!(config.web.port, 7824);
    assert_eq!(config.tray.default_open, "window");
    assert!(config.tray.close_to_tray);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn web_and_tray_sections_parse_explicit_values() {
    let root = std::env::temp_dir().join(format!("kxen-config-web-tray-parse-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture root");
    let path = root.join("config.toml");
    std::fs::write(
        &path,
        "[web]\nenabled = false\nbind = \"127.0.0.1\"\nport = 9000\n[tray]\ndefault_open = \"browser\"\nclose_to_tray = false\n",
    )
    .expect("write explicit config");
    let config = Config::load(&path, None).expect("load explicit web/tray config");
    assert!(!config.web.enabled);
    assert_eq!(config.web.port, 9000);
    assert_eq!(config.tray.default_open, "browser");
    assert!(!config.tray.close_to_tray);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn tray_default_open_rejects_unknown_value() {
    let root = std::env::temp_dir().join(format!("kxen-config-tray-invalid-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture root");
    let path = root.join("config.toml");
    for value in ["", "gui", "web", "Window"] {
        std::fs::write(&path, format!("[tray]\ndefault_open = {value:?}\n")).expect("write invalid default_open");
        let error = Config::load(&path, None).expect_err("invalid tray.default_open must fail").to_string();
        assert!(error.contains("tray.default_open"), "error must identify field: {error}");
    }
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn web_bind_must_be_an_ip_address() {
    let root = std::env::temp_dir().join(format!("kxen-config-web-bind-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture root");
    let path = root.join("config.toml");
    std::fs::write(&path, "[web]\nbind = \"localhost\"\n").expect("write invalid bind");
    let error = Config::load(&path, None).expect_err("hostname bind must fail").to_string();
    assert!(error.contains("web.bind"), "error must identify field: {error}");
    for bind in ["0.0.0.0", "::1"] {
        std::fs::write(&path, format!("[web]\nbind = {bind:?}\n")).expect("write valid bind");
        Config::load(&path, None).unwrap_or_else(|error| panic!("valid bind {bind} must load: {error}"));
    }
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn web_and_tray_are_user_only_keys() {
    let root = std::env::temp_dir().join(format!("kxen-config-web-tray-scope-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create fixture root");
    let user = root.join("user.toml");
    let project = root.join("project.toml");
    std::fs::write(&user, "").expect("write user config");
    for body in ["[web]\nenabled = false\n", "[tray]\nclose_to_tray = false\n"] {
        std::fs::write(&project, body).expect("write project config");
        let error = Config::load(&user, Some(&project)).expect_err("project must not set web/tray").to_string();
        assert!(error.contains("user-only"), "web/tray must stay user-owned: {error}");
    }
    std::fs::remove_dir_all(root).ok();
}
