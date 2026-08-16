//! rewind 撤销（B5）：按最近一次补偿点把文件层状态恢复回来；只恢复文件，不回放对话。

use std::path::Path;

use super::{clean, commit_locked, empty_dirs, git, head, repo_lock, reset_hard, rollback};

/// rewind 撤销：按最近一次 kxen-rewind-backup-* 补偿点把文件层状态恢复回来。
/// 只恢复文件——对话 JSONL 在 rewind 时已截断，撤销不回放对话（RPC 文档与前端文案同口径）。
/// 撤销前为当前状态再打一次补偿点：undo 本身也可被下一次 undo 找回（嵌套可撤销）。
pub fn undo_rewind(workdir: &Path) -> Result<String, String> {
    // 与 commit/rewind 同一把锁：补偿点与 reset/clean 期间不能有并发 add 改写 index
    let lock = repo_lock(workdir);
    let _guard = crate::core::shared::lock(&lock);
    let Some((backup_ref, target)) = latest_backup(workdir)? else {
        return Err("没有可撤销的 rewind 备份点".into());
    };
    let backup_label = format!("kxen-rewind-backup-{}-{}", std::process::id(), crate::core::shared::now_ms());
    commit_locked(workdir, &backup_label)?;
    let new_backup = head(workdir)?;
    let new_ref = format!("refs/kxen/rewind-backups/{backup_label}");
    let out = git(workdir, &["update-ref", &new_ref, &new_backup])?;
    if !out.status.success() {
        return Err(format!("git update-ref: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let directories = empty_dirs(workdir)?;
    if let Err(error) = reset_hard(workdir, &target) {
        return Err(rollback(workdir, &new_backup, &directories, error));
    }
    if let Err(error) = clean(workdir) {
        return Err(rollback(workdir, &new_backup, &directories, error));
    }
    // 撤销成功：被消费的补偿点已在 HEAD 链上，摘掉它的备份 ref（留在 refs 里会让清单失真）
    let out = git(workdir, &["update-ref", "-d", &backup_ref])?;
    if !out.status.success() {
        tracing::warn!(error = %String::from_utf8_lossy(&out.stderr), "consumed rewind-backup ref removal failed");
    }
    Ok(target)
}

/// 最近一个 rewind 补偿点：refs/kxen/rewind-backups/ 下按 label 尾部毫秒时间戳取最新。
/// 返回（ref 全名, commit hash）。
fn latest_backup(workdir: &Path) -> Result<Option<(String, String)>, String> {
    let out = git(workdir, &["for-each-ref", "--format=%(refname)%00%(objectname)", "refs/kxen/rewind-backups/"])?;
    if !out.status.success() {
        return Err(format!("git for-each-ref: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut best: Option<(u64, String, String)> = None;
    for line in text.lines() {
        let Some((refname, hash)) = line.split_once('\0') else { continue };
        let ms = refname.rsplit('-').next().and_then(|tail| tail.parse::<u64>().ok()).unwrap_or(0);
        if best.as_ref().is_none_or(|(best_ms, _, _)| ms > *best_ms) {
            best = Some((ms, refname.to_string(), hash.trim().to_string()));
        }
    }
    Ok(best.map(|(_, refname, hash)| (refname, hash)))
}
