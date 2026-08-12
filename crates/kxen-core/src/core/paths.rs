//! 路径约定（macOS / Linux / Windows 规范目录，由 dirs crate 按平台解析）。

use std::path::PathBuf;

pub const APP_DIR: &str = "kxen";

fn home_dir() -> PathBuf {
    dirs::home_dir()
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .filter(|path| path.is_absolute())
        // 不能返回字面量 `~`：Rust 不会展开它，数据会相对当前工作目录落盘。
        .unwrap_or_else(|| PathBuf::from("/var/empty"))
}

/// ~/.config/kxen（XDG 风格，跨平台一致，与官方 CLI 的 ~/.codex ~/.grok 同风格）
pub fn config_dir() -> PathBuf {
    home_dir().join(".config").join(APP_DIR)
}

/// ~/Library/Application Support/kxen（数据：goals、sessions、auth.json）
pub fn data_dir() -> PathBuf {
    // kxen 无头部署与测试隔离：环境变量覆盖（与 auth_file 同规约，勿删）
    if let Ok(p) = std::env::var("KXEN_DATA_DIR") {
        return PathBuf::from(p);
    }
    dirs::data_dir().unwrap_or_else(|| home_dir().join("Library/Application Support")).join(APP_DIR)
}

/// ~/Library/Caches/kxen
pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| home_dir().join("Library/Caches")).join(APP_DIR)
}

/// auth.json 路径（0600）
pub fn auth_file() -> PathBuf {
    // 测试隔离：环境变量覆盖（与 trust.rs 同规约，Once 写序防并行 env 竞态，勿删）
    if let Ok(p) = std::env::var("KXEN_AUTH_FILE") {
        return PathBuf::from(p);
    }
    data_dir().join("auth.json")
}

/// goals 目录
pub fn goals_dir() -> PathBuf {
    // 测试隔离：环境变量覆盖（与 auth_file 同规约，Once 写序防并行 env 竞态，勿删）
    if let Ok(p) = std::env::var("KXEN_GOALS_DIR") {
        return PathBuf::from(p);
    }
    data_dir().join("goals")
}

/// sessions 目录
pub fn sessions_dir() -> PathBuf {
    // 测试隔离：环境变量覆盖（与 auth_file 同规约，Once 写序防并行 env 竞态，勿删）
    if let Ok(p) = std::env::var("KXEN_SESSIONS_DIR") {
        return PathBuf::from(p);
    }
    data_dir().join("sessions")
}

/// Application-level Bot definitions, Runs, Conversations, Routines and artifacts.
pub fn bots_dir() -> PathBuf {
    if let Ok(path) = std::env::var("KXEN_BOTS_DIR") {
        return PathBuf::from(path);
    }
    data_dir().join("bots")
}

#[cfg(test)]
mod tests {
    const CHILD_ENV: &str = "KXEN_PATHS_DATA_DIR_CHILD";

    /// KXEN_DATA_DIR 优先于平台默认路径（fork 子进程隔离：env 是进程全局，勿在父进程并行写）。
    #[test]
    fn data_dir_env_override_wins_in_isolated_child() {
        if std::env::var_os(CHILD_ENV).is_none() {
            let dir = std::env::temp_dir().join(format!("kxen-data-dir-{}", uuid::Uuid::new_v4()));
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "core::paths::tests::data_dir_env_override_wins_in_isolated_child"])
                .env(CHILD_ENV, "1")
                .env("KXEN_DATA_DIR", &dir)
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let expected = std::env::var("KXEN_DATA_DIR").unwrap();
        assert_eq!(super::data_dir(), std::path::PathBuf::from(expected));
    }

    /// 不设覆盖时默认路径不变：绝对路径 + APP_DIR 收尾。
    #[test]
    fn data_dir_default_keeps_platform_layout() {
        if std::env::var_os("KXEN_DATA_DIR").is_some() {
            return;
        }
        let dir = super::data_dir();
        assert!(dir.is_absolute(), "默认 data_dir 必须是绝对路径: {}", dir.display());
        assert_eq!(dir.file_name().unwrap(), super::APP_DIR);
    }
}
