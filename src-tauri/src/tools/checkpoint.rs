//! checkpoint/rewind：shadow git（bare repo 存全局数据目录，--git-dir + --work-tree 不污染项目）。
//! 每用户消息一个检查点（turn 前状态）；rewind = reset 到该消息 commit + 会话截断。
//! 排除 node_modules/target（体量大且可再生）。

use std::path::{Path, PathBuf};

const EXCLUDES: &[&str] = &[":(exclude)node_modules", ":(exclude)target", ":(exclude).kxen/worktrees"];

fn repo_dir(workdir: &Path) -> PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    workdir.to_string_lossy().hash(&mut h);
    crate::core::paths::data_dir().join("shadow").join(format!("{:x}.git", h.finish()))
}

fn git(workdir: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new("git")
        .arg(format!("--git-dir={}", repo_dir(workdir).display()))
        .arg(format!("--work-tree={}", workdir.display()))
        .args(args)
        .output()
        .map_err(|e| format!("git spawn: {e}"))
}

fn ensure_repo(workdir: &Path) -> Result<(), String> {
    let dir = repo_dir(workdir);
    if dir.join("HEAD").exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // init 不带 --work-tree（init 不接受该参数）；后续操作才走 --git-dir/--work-tree
    let out = std::process::Command::new("git")
        .args(["init", "--bare"])
        .arg(&dir)
        .output()
        .map_err(|e| format!("git spawn: {e}"))?;
    if !out.status.success() {
        return Err(format!("git init: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

/// 打检查点：当前 worktree 全量提交（无变更也成功返回）。
pub fn commit(workdir: &Path, label: &str) -> Result<(), String> {
    ensure_repo(workdir)?;
    let mut add_args = vec!["add", "-A", "--", "."];
    add_args.extend(EXCLUDES);
    let out = git(workdir, &add_args)?;
    if !out.status.success() {
        return Err(format!("git add: {}", String::from_utf8_lossy(&out.stderr)));
    }
    // 无变更时 commit 失败属正常（nothing to commit），不算错误
    let out = git(workdir, &["-c", "user.name=kxen", "-c", "user.email=kxen@app", "commit", "--allow-empty-message", "--no-verify", "-q", "-m", label])?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.contains("nothing to commit") {
            return Err(format!("git commit: {stderr}"));
        }
    }
    Ok(())
}

/// 按 label 找 commit hash。
fn find(workdir: &Path, label: &str) -> Result<Option<String>, String> {
    let out = git(workdir, &["log", "--format=%H%x00%B%x00", "-z"])?;
    if !out.status.success() {
        return Ok(None);
    }
    // %x00 与 -z 各发一个 NUL：记录间是双 NUL，先滤空再两两成对
    let text = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = text.split('\0').filter(|s| !s.is_empty()).collect();
    for rec in parts.chunks(2) {
        if rec.len() == 2 && rec[1].trim() == label {
            return Ok(Some(rec[0].trim().to_string()));
        }
    }
    Ok(None)
}

/// rewind 到 label 检查点（reset --hard，调用方自行负责会话截断与提示）。
pub fn reset_to(workdir: &Path, label: &str) -> Result<String, String> {
    let Some(hash) = find(workdir, label)? else {
        return Err(format!("checkpoint not found: {label}"));
    };
    let out = git(workdir, &["reset", "--hard", &hash])?;
    if out.status.success() { Ok(hash) } else { Err(format!("git reset: {}", String::from_utf8_lossy(&out.stderr))) }
}

/// 会话是否有 rewind 历史可导（首条 checkpoint 是否存在）。
pub fn has_checkpoints(workdir: &Path) -> bool {
    repo_dir(workdir).join("HEAD").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_and_rewind() {
        let dir = std::env::temp_dir().join(format!("kxen-ckpt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "v1\n").unwrap();
        commit(&dir, "msg_1").unwrap();
        std::fs::write(dir.join("a.txt"), "v2\n").unwrap();
        commit(&dir, "msg_2").unwrap();
        // rewind 到 msg_1：a.txt 回到 v1
        reset_to(&dir, "msg_1").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "v1\n");
        // 不存在的 label 报错
        assert!(reset_to(&dir, "msg_404").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
