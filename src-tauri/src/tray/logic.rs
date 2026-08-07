//! tray 的纯状态映射与 config 持久化：GUI 之外的可单测部分。

use std::path::{Path, PathBuf};

/// tray 左键默认动作（config `tray.default_open` 的解析结果）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultOpen {
    Window,
    Browser,
}

impl DefaultOpen {
    /// config 加载已校验取值（window | browser）；未知值回退 window 防御旧版本写盘。
    pub fn parse(value: &str) -> Self {
        match value {
            "browser" => Self::Browser,
            _ => Self::Window,
        }
    }

    pub fn as_config_str(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Browser => "browser",
        }
    }
}

/// 带 token 的浏览器访问 URL；服务未启动（port 0）时 None。
pub fn access_url(bind_host: &str, port: u16, token: &str) -> Option<String> {
    if port == 0 {
        return None;
    }
    // IPv6 字面量在 URL host 位置必须带方括号
    let host = if bind_host.contains(':') { format!("[{bind_host}]") } else { bind_host.to_string() };
    Some(format!("http://{host}:{port}/?token={token}"))
}

/// 「在浏览器中打开」「复制访问链接」的可用条件：浏览器访问开启且服务在跑。
pub fn browser_actions_enabled(web_enabled: bool, url_available: bool) -> bool {
    web_enabled && url_available
}

/// 浏览器访问开关项 text（带实际端口；菜单只改 text/enabled/checked，绝不 rebuild）。
pub fn web_access_label(bind_host: &str, port: u16) -> String {
    if port == 0 { "浏览器访问（服务未启动）".to_string() } else { format!("浏览器访问 ({bind_host}:{port})") }
}

pub fn user_config_path() -> PathBuf {
    kxen_gui::core::paths::config_dir().join("config.toml")
}

/// 用户 config.toml 的 read-modify-write（写前整文档校验 + tmp/rename 原子替换）。
/// 与 ws/ops_config 同型；其入口 pub(super) 于 ws 模块，bin 侧不可达，此为最小复刻。
pub fn persist_user_config(mutate: impl FnOnce(&mut toml::Table)) -> Result<(), String> {
    persist_config_at(&user_config_path(), mutate)
}

fn persist_config_at(path: &Path, mutate: impl FnOnce(&mut toml::Table)) -> Result<(), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("config read {}: {error}", path.display())),
    };
    let mut doc: toml::Table = if text.trim().is_empty() {
        toml::Table::new()
    } else {
        toml::from_str(&text).map_err(|error| format!("config parse {}: {error}", path.display()))?
    };
    mutate(&mut doc);
    kxen_gui::core::config::validate_user_document(&doc, &path.display().to_string()).map_err(|error| error.to_string())?;
    let parent = path.parent().ok_or_else(|| format!("config path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent).map_err(|error| format!("config mkdir {}: {error}", parent.display()))?;
    let tmp = path.with_extension("toml.tmp");
    let serialized = toml::to_string(&doc).map_err(|error| format!("config serialize {}: {error}", path.display()))?;
    std::fs::write(&tmp, serialized).map_err(|error| format!("config write {}: {error}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|error| {
        std::fs::remove_file(&tmp).ok();
        format!("config replace {}: {error}", path.display())
    })?;
    Ok(())
}

/// 写 `[section] key = bool`（表不存在或类型被污染时重建）。
pub fn set_bool(doc: &mut toml::Table, section: &str, key: &str, value: bool) {
    section_table(doc, section).insert(key.into(), toml::Value::Boolean(value));
}

/// 写 `[section] key = string`（表不存在或类型被污染时重建）。
pub fn set_str(doc: &mut toml::Table, section: &str, key: &str, value: &str) {
    section_table(doc, section).insert(key.into(), toml::Value::String(value.into()));
}

fn section_table<'a>(doc: &'a mut toml::Table, section: &str) -> &'a mut toml::Table {
    let entry = doc.entry(section).or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !entry.is_table() {
        *entry = toml::Value::Table(toml::Table::new());
    }
    entry.as_table_mut().expect("刚重建为 table")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_open_parses_known_values_and_falls_back_to_window() {
        assert_eq!(DefaultOpen::parse("window"), DefaultOpen::Window);
        assert_eq!(DefaultOpen::parse("browser"), DefaultOpen::Browser);
        assert_eq!(DefaultOpen::parse(""), DefaultOpen::Window);
        assert_eq!(DefaultOpen::parse("gui"), DefaultOpen::Window);
        assert_eq!(DefaultOpen::Window.as_config_str(), "window");
        assert_eq!(DefaultOpen::Browser.as_config_str(), "browser");
    }

    #[test]
    fn access_url_requires_a_running_port() {
        assert_eq!(access_url("127.0.0.1", 0, "tok"), None);
        assert_eq!(access_url("127.0.0.1", 7824, "tok").as_deref(), Some("http://127.0.0.1:7824/?token=tok"));
        assert_eq!(access_url("::1", 7824, "tok").as_deref(), Some("http://[::1]:7824/?token=tok"));
    }

    #[test]
    fn browser_actions_require_enabled_service_and_available_url() {
        assert!(browser_actions_enabled(true, true));
        assert!(!browser_actions_enabled(false, true));
        assert!(!browser_actions_enabled(true, false));
        assert!(!browser_actions_enabled(false, false));
    }

    #[test]
    fn web_access_label_carries_actual_port_or_unavailable_hint() {
        assert_eq!(web_access_label("127.0.0.1", 7824), "浏览器访问 (127.0.0.1:7824)");
        assert_eq!(web_access_label("127.0.0.1", 0), "浏览器访问（服务未启动）");
    }

    #[test]
    fn set_bool_and_set_str_write_nested_tables() {
        let mut doc = toml::Table::new();
        set_bool(&mut doc, "web", "enabled", false);
        set_str(&mut doc, "tray", "default_open", "browser");
        assert_eq!(doc["web"]["enabled"].as_bool(), Some(false));
        assert_eq!(doc["tray"]["default_open"].as_str(), Some("browser"));
        // 被污染的标量 section 重建而不是 panic
        let mut dirty: toml::Table = toml::from_str("[web]\n").unwrap();
        dirty.insert("tray".into(), toml::Value::Integer(1));
        set_bool(&mut dirty, "tray", "close_to_tray", false);
        assert_eq!(dirty["tray"]["close_to_tray"].as_bool(), Some(false));
    }

    #[test]
    fn persist_config_at_roundtrips_and_keeps_other_keys() {
        let root = std::env::temp_dir().join(format!("kxen-tray-persist-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("mkdir");
        let path = root.join("config.toml");
        std::fs::write(&path, "[coding_rules]\nenabled = false\n[web]\nport = 9000\n").expect("write fixture");
        persist_config_at(&path, |doc| {
            set_bool(doc, "web", "enabled", false);
            set_str(doc, "tray", "default_open", "browser");
        })
        .expect("persist");
        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(text.contains("enabled = false"), "{text}");
        assert!(text.contains("port = 9000"), "其他键必须保留: {text}");
        let config = kxen_gui::core::config::Config::load(&path, None).expect("reload persisted config");
        assert!(!config.web.enabled);
        assert_eq!(config.web.port, 9000);
        assert_eq!(config.tray.default_open, "browser");
        assert!(!config.coding_rules.enabled);
        assert!(!path.with_extension("toml.tmp").exists(), "tmp 不得残留");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn persist_config_at_rejects_invalid_mutation_without_touching_disk() {
        let root = std::env::temp_dir().join(format!("kxen-tray-persist-invalid-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("mkdir");
        let path = root.join("config.toml");
        std::fs::write(&path, "[tray]\nclose_to_tray = true\n").expect("write fixture");
        let error = persist_config_at(&path, |doc| set_str(doc, "tray", "default_open", "gui")).expect_err("非法值必须被校验拦截");
        assert!(error.contains("tray.default_open"), "{error}");
        let text = std::fs::read_to_string(&path).expect("read back");
        assert!(!text.contains("default_open"), "校验失败不得落盘: {text}");
        std::fs::remove_dir_all(root).ok();
    }
}
