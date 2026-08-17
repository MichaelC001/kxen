use super::*;
use crate::kanban::driver::tests::{agent_board, agent_def, create_card, deps, text_stream};
use crate::kanban::driver::{LandingKind, execute};
use crate::kanban::events::Outcome;
use crate::kanban::{Board, agents};
use std::sync::Arc;

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-wt-{tag}-{}-{nanos}", std::process::id()))
}

fn git(workspace: &Path, args: &[&str]) {
    let out = std::process::Command::new("git").args(args).current_dir(workspace).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
}

/// git temp 仓库：init + 初始 commit（worktree add 需要已born的 HEAD）+ 预置 .gitignore（提交进去，worktree 才继承）。
fn git_repo(tag: &str, gitignore: &str) -> PathBuf {
    let workspace = temp(tag);
    std::fs::create_dir_all(&workspace).unwrap();
    git(&workspace, &["init"]);
    if !gitignore.is_empty() {
        std::fs::write(workspace.join(".gitignore"), gitignore).unwrap();
        git(&workspace, &["add", ".gitignore"]);
    }
    // commit.gpgsign=false：宿主全局 gitconfig 可能开签名（如 1Password op-ssh-sign），测试环境签名必挂
    git(&workspace, &["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "--allow-empty", "-m", "init"]);
    workspace
}

fn worktree_dir(workspace: &Path, card_id: &str) -> PathBuf {
    crate::core::paths::KxenPaths::project(workspace).worktree(&format!("card-{card_id}"))
}

fn artifact_dir(workspace: &Path, card_id: &str) -> PathBuf {
    crate::core::paths::KxenPaths::project(workspace).kanban_artifact_dir("board_t", card_id)
}

/// run 中往 worktree 写产物的假流：write 闭包在 LLM 请求时执行（此时 worktree 已分配）。
fn writing_stream(write: impl Fn() + Send + Sync + 'static) -> crate::llm::StreamFn {
    Arc::new(move |_, _, _, _| {
        write();
        Box::pin(futures::stream::iter(vec![crate::llm::Delta::Text("done\nVERDICT: success".into()), crate::llm::Delta::Done]))
    })
}

#[tokio::test]
async fn terminal_column_detaches_worktree_keeping_branch_and_manifest() {
    let workspace = git_repo("terminal", "");
    agents::save(&workspace, &agent_def()).unwrap();
    let mut board = agent_board(&workspace, None);
    let card_id = create_card(&mut board, "implementing");
    drop(board);
    let wt = worktree_dir(&workspace, &card_id);
    let wt_in_run = wt.clone();
    let stream = writing_stream(move || std::fs::write(wt_in_run.join("output.txt"), "artifact").unwrap());
    let landing = execute(&workspace, "board_t", &card_id, &deps(&workspace, stream), None).await.unwrap();
    assert_eq!(landing.kind, LandingKind::Finished(Outcome::Success));
    assert!(!wt.exists(), "终态 detach 必须释放 worktree 目录");
    git(&workspace, &["rev-parse", "--verify", &format!("refs/heads/kxen/card-{card_id}")]); // 分支保留
    let artifacts = artifact_dir(&workspace, &card_id);
    assert_eq!(std::fs::read_to_string(artifacts.join("files/output.txt")).unwrap(), "artifact", "产物被快照抢救");
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(artifacts.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["card_id"], serde_json::Value::String(card_id.clone()));
    assert!(manifest["collected"].as_array().unwrap().iter().any(|p| p == "output.txt"), "manifest collected 含产物: {manifest}");
    let board = Board::open(&workspace, "board_t").unwrap();
    assert!(board.state().cards[&card_id].comments.iter().any(|c| c.body.contains("worktree 已释放")), "detach 必须落审计评论");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn detach_snapshots_untracked_and_ignored_files() {
    let workspace = git_repo("snapshot", "*.log\n");
    agents::save(&workspace, &agent_def()).unwrap();
    let mut board = agent_board(&workspace, None);
    let card_id = create_card(&mut board, "implementing");
    drop(board);
    let wt = worktree_dir(&workspace, &card_id);
    let wt_in_run = wt.clone();
    let stream = writing_stream(move || {
        std::fs::write(wt_in_run.join("new.txt"), "untracked").unwrap();
        std::fs::write(wt_in_run.join("debug.log"), "ignored").unwrap();
    });
    let landing = execute(&workspace, "board_t", &card_id, &deps(&workspace, stream), None).await.unwrap();
    assert_eq!(landing.kind, LandingKind::Finished(Outcome::Success));
    let files = artifact_dir(&workspace, &card_id).join("files");
    assert_eq!(std::fs::read_to_string(files.join("new.txt")).unwrap(), "untracked");
    assert_eq!(std::fs::read_to_string(files.join("debug.log")).unwrap(), "ignored", "被 gitignore 的产物也要抢救");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(files.parent().unwrap().join("manifest.json")).unwrap()).unwrap();
    let collected: Vec<&str> = manifest["collected"].as_array().unwrap().iter().filter_map(|p| p.as_str()).collect();
    assert!(collected.contains(&"new.txt") && collected.contains(&"debug.log"), "两类产物都在 collected: {collected:?}");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn failure_to_nonterminal_column_keeps_worktree_for_retry() {
    let workspace = git_repo("keep", "");
    agents::save(&workspace, &agent_def()).unwrap();
    let mut board = agent_board(&workspace, None); // on_failure 回流 implementing（非终态）
    let card_id = create_card(&mut board, "implementing");
    drop(board);
    let landing = execute(&workspace, "board_t", &card_id, &deps(&workspace, text_stream("nope\nVERDICT: failure")), None).await.unwrap();
    assert_eq!(landing.kind, LandingKind::Finished(Outcome::Failure));
    assert!(worktree_dir(&workspace, &card_id).exists(), "非终态保留 worktree 供显式重试复跑");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn non_git_workspace_runs_at_root_with_degrade_comment() {
    let workspace = temp("nogit");
    std::fs::create_dir_all(&workspace).unwrap();
    agents::save(&workspace, &agent_def()).unwrap();
    let mut board = agent_board(&workspace, None);
    let card_id = create_card(&mut board, "implementing");
    drop(board);
    let landing = execute(&workspace, "board_t", &card_id, &deps(&workspace, text_stream("done\nVERDICT: success")), None).await.unwrap();
    assert_eq!(landing.kind, LandingKind::Finished(Outcome::Success));
    assert!(!crate::core::paths::KxenPaths::project(&workspace).worktrees_dir().exists(), "非 git workspace 不得创建 worktree");
    let board = Board::open(&workspace, "board_t").unwrap();
    let card = &board.state().cards[&card_id];
    assert_eq!(card.column_id, "done", "降级 run 正常流转");
    assert!(card.comments.iter().any(|c| c.body.contains("无 worktree 隔离")), "降级必须落审计评论");
    assert!(!card.comments.iter().any(|c| c.body.contains("worktree 已释放")), "无 worktree 可拆不得落 detach 评论");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn ignored_directory_is_recorded_skipped_not_copied() {
    let workspace = git_repo("ignored-dir", "node_modules/\n");
    agents::save(&workspace, &agent_def()).unwrap();
    let mut board = agent_board(&workspace, None);
    let card_id = create_card(&mut board, "implementing");
    drop(board);
    let wt = worktree_dir(&workspace, &card_id);
    let wt_in_run = wt.clone();
    let stream = writing_stream(move || {
        let dir = wt_in_run.join("node_modules").join("pkg");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.js"), "module").unwrap();
    });
    let landing = execute(&workspace, "board_t", &card_id, &deps(&workspace, stream), None).await.unwrap();
    assert_eq!(landing.kind, LandingKind::Finished(Outcome::Success));
    let artifacts = artifact_dir(&workspace, &card_id);
    assert!(!artifacts.join("files").join("node_modules").exists(), "ignored 目录不递归拷贝");
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(artifacts.join("manifest.json")).unwrap()).unwrap();
    let skipped = manifest["skipped"].as_array().unwrap();
    assert!(skipped.iter().any(|s| s["path"] == "node_modules/"), "ignored 目录必须记 manifest skipped: {manifest}");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn parse_porcelain_z_classifies_three_entry_kinds() {
    let entries = parse_porcelain_z("?? new.txt\0 M src/a.rs\0!! debug.log\0R  new name.txt\0old name.txt\0");
    assert_eq!(
        entries,
        vec![("??", "new.txt"), (" M", "src/a.rs"), ("!!", "debug.log"), ("R ", "new name.txt"),],
        "?? / 已跟踪改动 / !! 三类行 + 重命名取新路径跳过源段"
    );
    assert!(parse_porcelain_z("").is_empty());
}

#[tokio::test]
async fn ignored_file_over_size_limit_is_skipped() {
    let workspace = git_repo("limit", "*.bin\n");
    let CardWorkdir::Worktree(wt) = ensure_card_worktree(&workspace, "c1").await.unwrap() else {
        panic!("git 仓库必须分配 worktree")
    };
    std::fs::write(wt.join("big.bin"), vec![0u8; 32]).unwrap();
    std::fs::write(wt.join("small.bin"), vec![0u8; 4]).unwrap();
    // 上限做成参数：用小上限测边界语义（生产为 MAX_IGNORED_FILE_BYTES = 10MB / MAX_SNAPSHOT_TOTAL_BYTES = 64MB）
    let report = snapshot_artifacts(&workspace, "board_t", "c1", &wt, 10, 1024).await.unwrap();
    assert!(report.collected.iter().any(|p| p == "small.bin"), "限额内的 ignored 文件照收");
    assert!(report.skipped.iter().any(|s| s.path == "big.bin"), "超限 ignored 文件记 skipped");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn untracked_file_over_size_limit_is_skipped() {
    let workspace = git_repo("ulimit", "");
    let CardWorkdir::Worktree(wt) = ensure_card_worktree(&workspace, "c1").await.unwrap() else {
        panic!("git 仓库必须分配 worktree")
    };
    std::fs::write(wt.join("big.txt"), vec![0u8; 32]).unwrap();
    std::fs::write(wt.join("small.txt"), vec![0u8; 4]).unwrap();
    let report = snapshot_artifacts(&workspace, "board_t", "c1", &wt, 10, 1024).await.unwrap();
    assert!(report.skipped.iter().any(|s| s.path == "big.txt"), "超限未跟踪文件记 skipped: {:?}", report.skipped);
    assert!(report.collected.iter().any(|p| p == "small.txt"), "限额内的未跟踪文件照收");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn files_over_total_limit_are_skipped_without_losing_collected() {
    let workspace = git_repo("tlimit", "");
    let CardWorkdir::Worktree(wt) = ensure_card_worktree(&workspace, "c1").await.unwrap() else {
        panic!("git 仓库必须分配 worktree")
    };
    // porcelain 按路径排序：a.txt 先收（8B），b.txt 累计 16B 超总量上限记 skipped
    std::fs::write(wt.join("a.txt"), vec![0u8; 8]).unwrap();
    std::fs::write(wt.join("b.txt"), vec![0u8; 8]).unwrap();
    let report = snapshot_artifacts(&workspace, "board_t", "c1", &wt, 10, 12).await.unwrap();
    assert_eq!(report.collected, vec!["a.txt".to_string()], "总量上限前已收的不丢: {:?}", report.collected);
    assert!(report.skipped.iter().any(|s| s.path == "b.txt" && s.reason.contains("总量")), "超出总量的记 skipped: {:?}", report.skipped);
    std::fs::remove_dir_all(workspace).ok();
}

// symlink 创建是 unix 语义；CI 的 Windows 任务不跑测试但跑 check/clippy，cfg(unix) 圈住整段
#[cfg(unix)]
#[tokio::test]
async fn symlinks_are_skipped_not_followed() {
    let workspace = git_repo("symlink", "");
    let CardWorkdir::Worktree(wt) = ensure_card_worktree(&workspace, "c1").await.unwrap() else {
        panic!("git 仓库必须分配 worktree")
    };
    std::os::unix::fs::symlink("/tmp", wt.join("link_out")).unwrap();
    std::fs::write(wt.join("real.txt"), "real").unwrap();
    let report = snapshot_artifacts(&workspace, "board_t", "c1", &wt, 1024, 4096).await.unwrap();
    assert!(report.skipped.iter().any(|s| s.path == "link_out"), "symlink 必须记 skipped: {:?}", report.skipped);
    assert!(!report.files_dir.join("link_out").exists(), "symlink 不得有快照内容");
    assert!(report.collected.iter().any(|p| p == "real.txt"), "普通文件照收");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn ensure_degrades_when_workspace_is_subdir_of_parent_repo() {
    let parent = git_repo("parent", "");
    let sub = parent.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    assert!(matches!(ensure_card_worktree(&sub, "c1").await.unwrap(), CardWorkdir::WorkspaceRoot), "父仓库子目录不得建 worktree");
    assert!(!crate::core::paths::KxenPaths::project(&parent).worktrees_dir().exists(), "父仓库不得被建 worktree");
    std::fs::remove_dir_all(parent).ok();
}

#[tokio::test]
async fn ensure_allocates_worktree_at_repo_root() {
    let workspace = git_repo("root", "");
    assert!(matches!(ensure_card_worktree(&workspace, "c1").await.unwrap(), CardWorkdir::Worktree(_)), "仓库根本身必须分配 worktree");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn ensure_degrades_on_non_git_workspace() {
    let workspace = temp("ensure-nogit");
    std::fs::create_dir_all(&workspace).unwrap();
    assert!(matches!(ensure_card_worktree(&workspace, "c1").await.unwrap(), CardWorkdir::WorkspaceRoot));
    assert!(!workspace.join(".gitignore").exists(), "非 git workspace 不得被写 .gitignore");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn worktree_name_enforces_whitelist() {
    assert_eq!(worktree_name("card_ok-1").unwrap(), "card-card_ok-1");
    assert!(worktree_name("../escape").is_err(), "路径穿越必须被白名单拒绝");
}
