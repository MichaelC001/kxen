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
    assert_eq!(super::KxenPaths::user().root(), std::path::PathBuf::from(expected));
}

/// 不设覆盖时，用户数据统一落在 ~/.agents/kxen。
#[test]
fn data_dir_default_uses_agents_namespace() {
    if std::env::var_os("KXEN_DATA_DIR").is_some() {
        return;
    }
    let paths = super::KxenPaths::user();
    let dir = paths.root();
    assert!(dir.is_absolute(), "默认 data_dir 必须是绝对路径: {}", dir.display());
    assert!(dir.ends_with(std::path::Path::new(".agents").join(super::APP_DIR)), "unexpected data_dir: {}", dir.display());
    assert_eq!(paths.config_file(), dir.join("config.toml"));
    assert_eq!(paths.cache_dir(), dir.join("cache"));
}

#[test]
fn global_and_custom_scopes_are_explicit() {
    let home = std::path::Path::new("/home/example");
    assert_eq!(super::KxenPaths::global_in(home).root(), home.join(".agents/kxen"));
    assert_eq!(super::KxenPaths::custom("/var/lib/example").root(), std::path::Path::new("/var/lib/example"));
}

#[test]
fn project_dir_uses_isolated_agents_namespace() {
    let paths = super::KxenPaths::project(std::path::Path::new("/workspace"));
    assert_eq!(paths.root(), std::path::Path::new("/workspace").join(".agents").join("kxen"));
    assert_eq!(paths.worktree("fix-1"), std::path::Path::new("/workspace/.agents/kxen/worktrees/fix-1"));
    assert_eq!(paths.kanban_board("board_1"), std::path::Path::new("/workspace/.agents/kxen/kanban/board_1"));
    assert_eq!(
        paths.kanban_turns_file("board_1", "board_1:card_1:implementing:1"),
        std::path::Path::new("/workspace/.agents/kxen/kanban/board_1/runs/board_1%3Acard_1%3Aimplementing%3A1.turns.jsonl")
    );
}

#[test]
fn project_runtime_detection_supports_windows_and_posix_separators() {
    assert!(super::KxenPaths::contains_project_state("/repo/.agents/kxen/worktrees/fix"));
    assert!(super::KxenPaths::contains_project_state(r"C:\repo\.agents\kxen\worktrees\fix"));
    assert!(!super::KxenPaths::contains_project_state("/repo/.agents/rules/kxen.md"));
}
