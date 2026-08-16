//! checkpoint 保留窗口（B6）：shadow 历史裁剪到最近 keep 个，提交成功后 best-effort 执行。
//! 窗口最旧保留项重建为无父 root（rewind 按 label 查找，hash 变更无妨）。

use std::path::Path;

use super::{git, has_head};

/// checkpoint 保留窗口：config.toml checkpoint_keep 覆盖，缺省 50。
pub(super) fn keep_count() -> usize {
    crate::core::config_cache::cached_user_config()
        .and_then(|config| config.checkpoint_keep)
        .filter(|keep| *keep > 0)
        .map(|keep| keep as usize)
        .unwrap_or(50)
}

/// 全部 checkpoint（newest first）：(hash, label)。backup 补偿点在 reset 后不可达，不在此列。
pub(super) fn history(workdir: &Path) -> Result<Vec<(String, String)>, String> {
    let out = git(workdir, &["log", "--format=%H%x00%B%x00", "-z"])?;
    if !out.status.success() {
        return Err(format!("git log: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut parts = text.split('\0').filter(|part| !part.is_empty());
    let mut commits = Vec::new();
    while let Some(hash) = parts.next() {
        let Some(message) = parts.next() else { break };
        commits.push((hash.trim().to_string(), message.trim().to_string()));
    }
    Ok(commits)
}

/// 裁剪 shadow 历史到最近 keep 个 checkpoint：窗口最旧保留项重建为无父 root
/// （commit-tree 同 tree 同 label，hash 变更无妨——rewind 按 label 查找），
/// update-ref 后 gc 立即回收窗口外快照对象。
pub(super) fn prune_locked(workdir: &Path, keep: usize) -> Result<(), String> {
    let commits = history(workdir)?;
    if commits.len() <= keep || !has_head(workdir)? {
        return Ok(());
    }
    let mut parent: Option<String> = None;
    for (hash, label) in commits.iter().take(keep).rev() {
        let tree_out = git(workdir, &["show", "-s", "--format=%T", hash])?;
        if !tree_out.status.success() {
            return Err(format!("git show: {}", String::from_utf8_lossy(&tree_out.stderr)));
        }
        let tree = String::from_utf8_lossy(&tree_out.stdout).trim().to_string();
        let mut args =
            vec!["-c", "user.name=kxen", "-c", "user.email=kxen@app", "-c", "commit.gpgsign=false", "commit-tree", tree.as_str()];
        if let Some(p) = parent.as_deref() {
            args.push("-p");
            args.push(p);
        }
        args.push("-m");
        args.push(label.as_str());
        let out = git(workdir, &args)?;
        if !out.status.success() {
            return Err(format!("git commit-tree: {}", String::from_utf8_lossy(&out.stderr)));
        }
        parent = Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }
    let tip = parent.expect("commits.len() > keep >= 1");
    let out = git(workdir, &["update-ref", "HEAD", &tip])?;
    if !out.status.success() {
        return Err(format!("git update-ref: {}", String::from_utf8_lossy(&out.stderr)));
    }
    // gc 失败不推翻已完成的裁剪：对象留待下次 gc 回收
    let out = git(workdir, &["gc", "--prune=now", "--quiet"])?;
    if !out.status.success() {
        tracing::warn!(error = %String::from_utf8_lossy(&out.stderr), "shadow repo gc failed after prune");
    }
    Ok(())
}
