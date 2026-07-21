//! 路径约定（macOS 规范目录，仅 Apple Silicon 平台）。

use std::path::PathBuf;

pub const APP_DIR: &str = "kxen";

/// ~/.config/kxen（XDG 风格，跨平台一致，与官方 CLI 的 ~/.codex ~/.grok 同风格）
pub fn config_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("~")).join(".config").join(APP_DIR)
}

/// ~/Library/Application Support/kxen（数据：goals、sessions、auth.json）
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/Library/Application Support"))
        .join(APP_DIR)
}

/// ~/Library/Caches/kxen
pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| PathBuf::from("~/Library/Caches")).join(APP_DIR)
}

/// auth.json 路径（0600）
pub fn auth_file() -> PathBuf {
    data_dir().join("auth.json")
}

/// goals 目录
pub fn goals_dir() -> PathBuf {
    data_dir().join("goals")
}
