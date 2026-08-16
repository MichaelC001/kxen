//! worktree 删除前抢救备份与合回（B4）：dirty worktree 的未提交改动打成 patch 存
//! `<repo>/.kxen/backups/`，删除成为可恢复操作；apply 把指定 worktree 的全部差异合回主树。

use std::path::{Path, PathBuf};

use super::{canon, git, validate_name};

/// 删除前抢救备份的上限（与 kanban 产物快照同口径）：symlink 拒收、单文件 10MB、总量 64MB。
const BACKUP_MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
const BACKUP_MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

/// 删除前备份结果：patch 路径 + 因上限/符号链接未入 patch 的条目。
#[derive(Debug)]
pub struct RemoveBackup {
    pub patch: PathBuf,
    pub skipped: Vec<SkippedBackup>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SkippedBackup {
    pub path: String,
    pub reason: String,
}

/// 把 worktree 未提交改动（含 untracked，不含 ignored）打成 git apply 可用的 patch，
/// 存 `<repo>/.kxen/backups/worktree-<name>-<ms>.patch`；skipped 清单落同名 .skipped.txt。
/// 恢复方式：`git apply <patch>`（在目标仓库根执行）。
pub(super) async fn backup_uncommitted(repo: &Path, name: &str, worktree: &Path) -> Result<RemoveBackup, String> {
    let out = git(worktree, &["status", "--porcelain", "-z"]).await?;
    let mut untracked: Vec<String> = Vec::new();
    let mut skipped: Vec<SkippedBackup> = Vec::new();
    let mut total_bytes: u64 = 0;
    // 已跟踪改动天然进 git diff HEAD；只需为 untracked 做 intent-to-add 与上限过滤
    for (code, path) in parse_porcelain_z(&out) {
        if code != "??" {
            continue;
        }
        let source = worktree.join(path);
        if path.ends_with('/') {
            collect_untracked_dir(&source, worktree, &mut untracked, &mut skipped, &mut total_bytes)?;
        } else {
            collect_untracked_file(&source, path, &mut untracked, &mut skipped, &mut total_bytes)?;
        }
    }
    // 超限/symlink 之外的内容入 patch；intent-to-add 让 untracked 出现在 diff 里
    for chunk in untracked.chunks(100) {
        let mut args = vec!["add", "-N", "--"];
        args.extend(chunk.iter().map(String::as_str));
        git(worktree, &args).await?;
    }
    let patch = git(worktree, &["diff", "--binary", "--full-index", "HEAD"]).await?;
    let backups = repo.join(".kxen").join("backups");
    std::fs::create_dir_all(&backups).map_err(|e| format!("create {}: {e}", backups.display()))?;
    let patch_path = backups.join(format!("worktree-{name}-{}.patch", crate::core::shared::now_ms()));
    std::fs::write(&patch_path, &patch).map_err(|e| format!("write {}: {e}", patch_path.display()))?;
    if !skipped.is_empty() {
        let note = skipped.iter().map(|s| format!("{}\t{}", s.path, s.reason)).collect::<Vec<_>>().join("\n");
        std::fs::write(patch_path.with_extension("skipped.txt"), format!("{note}\n")).map_err(|e| e.to_string())?;
    }
    prune_backups(repo);
    Ok(RemoveBackup { patch: patch_path, skipped })
}

/// 单文件上限/符号链接过滤（口径与 kanban/worktree.rs collect_file 一致）。
fn collect_untracked_file(
    source: &Path,
    rel: &str,
    untracked: &mut Vec<String>,
    skipped: &mut Vec<SkippedBackup>,
    total_bytes: &mut u64,
) -> Result<(), String> {
    let meta = match std::fs::symlink_metadata(source) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("stat {}: {error}", source.display())),
    };
    if meta.file_type().is_symlink() {
        skipped.push(SkippedBackup { path: rel.into(), reason: "符号链接不备份，防逃逸与深递归".into() });
        return Ok(());
    }
    if meta.len() > BACKUP_MAX_FILE_BYTES {
        skipped.push(SkippedBackup { path: rel.into(), reason: format!("文件超过单文件上限 {BACKUP_MAX_FILE_BYTES} 字节") });
        return Ok(());
    }
    if total_bytes.saturating_add(meta.len()) > BACKUP_MAX_TOTAL_BYTES {
        skipped.push(SkippedBackup { path: rel.into(), reason: format!("备份总量超过上限 {BACKUP_MAX_TOTAL_BYTES} 字节") });
        return Ok(());
    }
    untracked.push(rel.to_string());
    *total_bytes += meta.len();
    Ok(())
}

/// 未跟踪目录递归全收：porcelain 把整目录折叠成一条 `dir/` 条目。
/// read_dir 的 file_type 不跟随 symlink：链接目录由 collect_untracked_file 拒收，递归不逃逸。
fn collect_untracked_dir(
    dir: &Path,
    root: &Path,
    untracked: &mut Vec<String>,
    skipped: &mut Vec<SkippedBackup>,
    total_bytes: &mut u64,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let rel = path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().into_owned();
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            collect_untracked_dir(&path, root, untracked, skipped, total_bytes)?;
        } else {
            collect_untracked_file(&path, &rel, untracked, skipped, total_bytes)?;
        }
    }
    Ok(())
}

/// porcelain -z 输出解析：NUL 分隔、路径不加 C 引号；R/C 条目后跟一个源路径段，跳过。
fn parse_porcelain_z(output: &str) -> Vec<(&str, &str)> {
    let mut entries = Vec::new();
    let mut segments = output.split('\0').filter(|s| !s.is_empty());
    while let Some(segment) = segments.next() {
        if segment.len() <= 3 {
            continue;
        }
        let code = &segment[..2];
        entries.push((code, &segment[3..]));
        if code.contains('R') || code.contains('C') {
            segments.next();
        }
    }
    entries
}

/// worktree.apply 结果：applied=false 即冲突，diff 返回给前端展示/另存，主树不落盘。
#[derive(Debug, serde::Serialize)]
pub struct ApplyOutcome {
    pub applied: bool,
    pub diff: String,
}

/// 把指定 worktree 的全部差异（相对主树 HEAD：分支已提交 + 未提交 + untracked）合回主树。
/// 先 `git apply --check` 验证，冲突则返回 diff 不落盘（fail-closed）。
/// 注意：会在 worktree 索引留下 intent-to-add 条目（diff 口径需要，不影响内容）。
pub async fn apply(repo: &Path, name: &str) -> Result<ApplyOutcome, String> {
    let repo = &canon(repo);
    validate_name(name)?;
    let path = repo.join(".kxen").join("worktrees").join(name);
    if !path.join(".git").exists() {
        return Err(format!("worktree {name} 不存在"));
    }
    let head = git(repo, &["rev-parse", "HEAD"]).await?;
    let head = head.trim();
    // intent-to-add 让 untracked 进 diff（与备份同口径；ignored 不收）
    git(&path, &["add", "-N", "--", "."]).await?;
    let patch = git(&path, &["diff", "--binary", "--full-index", head]).await?;
    if patch.trim().is_empty() {
        return Ok(ApplyOutcome { applied: true, diff: String::new() });
    }
    if git_stdin(repo, &["apply", "--check", "-"], &patch).await.is_err() {
        return Ok(ApplyOutcome { applied: false, diff: patch });
    }
    git_stdin(repo, &["apply", "-"], &patch).await?;
    Ok(ApplyOutcome { applied: true, diff: patch })
}

/// 从 stdin 喂 patch 的 git 调用（apply --check / apply）。
async fn git_stdin(repo: &Path, args: &[&str], input: &str) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;
    let mut child = tokio::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("git spawn: {e}"))?;
    let mut stdin = child.stdin.take().expect("stdin piped");
    stdin.write_all(input.as_bytes()).await.map_err(|e| format!("git stdin: {e}"))?;
    drop(stdin);
    let out = child.wait_with_output().await.map_err(|e| format!("git wait: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&out.stderr).chars().take(300).collect::<String>()
        ))
    }
}

/// .kxen/backups/ 数量上限：超出清最旧（mtime 升序），覆盖备份不无界增长。
const BACKUP_KEEP: usize = 50;
/// .kxen/backups/ 时间上限：默认 30 天，与数量上限并存，先到先清。
const BACKUP_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(30 * 24 * 3600);

pub fn prune_backups(root: &Path) {
    let mut dirs = vec![root.join(".kxen").join("backups")];
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    while let Some(d) = dirs.pop() {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    dirs.push(e.path());
                } else if let Ok(mtime) = e.metadata().and_then(|m| m.modified()) {
                    files.push((mtime, e.path()));
                }
            }
        }
    }
    let now = std::time::SystemTime::now();
    let mut fresh: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for (mtime, path) in files {
        let expired = now.duration_since(mtime).map(|age| age > BACKUP_MAX_AGE).unwrap_or(false);
        if expired {
            std::fs::remove_file(&path).ok();
        } else {
            fresh.push((mtime, path));
        }
    }
    fresh.sort_unstable();
    for (_, p) in fresh.iter().take(fresh.len().saturating_sub(BACKUP_KEEP)) {
        std::fs::remove_file(p).ok();
    }
}
