//! worktree 隔离：git worktree 并行安全（批量迁移 / 并行修改）。
//! worktree 放 `<repo>/.kxen/worktrees/<name>`（自动把 .kxen/ 写进 .gitignore），分支 `kxen/<name>`。

use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: PathBuf,
    pub branch: String,
}

/// macOS 临时目录 /var 是 /private/var 的软链：git 输出全是不等价的真实路径，统一 canonicalize。
fn canon(repo: &Path) -> PathBuf {
    repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf())
}

/// 创建 worktree（已存在则直接复用）。
pub async fn create(repo: &Path, name: &str) -> Result<WorktreeInfo, String> {
    let repo = &canon(repo);
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("worktree name must be alphanumeric or dash".into());
    }
    ensure_gitignore(repo)?;
    let path = repo.join(".kxen").join("worktrees").join(name);
    let branch = format!("kxen/{name}");
    if path.exists() {
        return Ok(WorktreeInfo { name: name.into(), path, branch });
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    git(repo, &["worktree", "add", &path.to_string_lossy(), "-b", &branch]).await?;
    Ok(WorktreeInfo { name: name.into(), path, branch })
}

/// 移除 worktree（分支默认保留，用户自行 merge/diff 后处理）。
pub async fn remove(repo: &Path, name: &str, delete_branch: bool) -> Result<(), String> {
    let repo = &canon(repo);
    let path = repo.join(".kxen").join("worktrees").join(name);
    if path.exists() {
        git(repo, &["worktree", "remove", "--force", &path.to_string_lossy()]).await?;
    }
    if delete_branch {
        git(repo, &["branch", "-D", &format!("kxen/{name}")]).await?;
    }
    Ok(())
}

pub async fn list(repo: &Path) -> Result<Vec<WorktreeInfo>, String> {
    let repo = &canon(repo);
    let out = git(repo, &["worktree", "list", "--porcelain"]).await?;
    let mut infos = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch = String::new();
    for line in out.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            if let Some(p) = path.take() {
                infos.push((p, std::mem::take(&mut branch)));
            }
            path = Some(PathBuf::from(p));
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            branch = b.to_string();
        }
    }
    if let Some(p) = path {
        infos.push((p, branch));
    }
    let prefix = repo.join(".kxen").join("worktrees");
    Ok(infos
        .into_iter()
        .filter_map(|(p, branch)| {
            let name = p.strip_prefix(&prefix).ok()?.to_string_lossy().into_owned();
            Some(WorktreeInfo { name, path: p, branch })
        })
        .collect())
}

/// 当前树相对 worktree 分支的 diff --stat（完成回主树的预览）。
pub async fn diff_stat(repo: &Path, name: &str) -> Result<String, String> {
    git(repo, &["diff", "--stat", &format!("kxen/{name}")]).await
}

// ---------------- 通用 git 状态/diff（dock 改动面板数据源） ----------------

#[derive(Debug, serde::Serialize)]
pub struct StatusEntry {
    pub path: String,
    /// M / A / D / ??（取 porcelain 首列，重命名取 R）
    pub status: String,
}

/// git status --porcelain（未暂存 + 未跟踪，dock 的改动清单）。
pub async fn status(repo: &Path) -> Result<Vec<StatusEntry>, String> {
    let repo = &canon(repo);
    let out = git(repo, &["status", "--porcelain"]).await?;
    Ok(out
        .lines()
        .filter(|l| l.len() > 3)
        .map(|l| {
            let code = l[..2].trim().to_string();
            // 重命名 "R  old -> new" 取新路径
            let path = l[3..].rsplit(" -> ").next().unwrap_or(&l[3..]).to_string();
            StatusEntry { path, status: code }
        })
        .collect())
}

/// 单文件 diff（未暂存）；未跟踪文件走 --no-index 合成 new-file diff。
pub async fn diff_file(repo: &Path, path: &str) -> Result<String, String> {
    let repo = &canon(repo);
    let diff = git(repo, &["diff", "--", path]).await.unwrap_or_default();
    if !diff.trim().is_empty() {
        return Ok(diff);
    }
    // --no-index 命中差异时退出码为 1：走容忍路径
    let out = tokio::process::Command::new("git")
        .args(["diff", "--no-index", "--", "/dev/null", path])
        .current_dir(repo)
        .output()
        .await
        .map_err(|e| format!("git spawn: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if text.trim().is_empty() {
        Err("no diff (unchanged or not a file)".into())
    } else {
        Ok(text)
    }
}

/// .kxen/ 进 .gitignore（幂等）。
fn ensure_gitignore(repo: &Path) -> Result<(), String> {
    let path = repo.join(".gitignore");
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    if content.lines().any(|l| l.trim() == ".kxen/") {
        return Ok(());
    }
    let mut new = content;
    if !new.is_empty() && !new.ends_with('\n') {
        new.push('\n');
    }
    new.push_str(".kxen/\n");
    std::fs::write(&path, new).map_err(|e| e.to_string())
}

async fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = tokio::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .await
        .map_err(|e| format!("git spawn: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!("git {} failed: {}", args.first().unwrap_or(&""), String::from_utf8_lossy(&out.stderr).chars().take(300).collect::<String>()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用临时 git 仓库真实跑 create/list/remove。
    #[tokio::test]
    async fn lifecycle() {
        let repo = std::env::temp_dir().join(format!("kxen-wt-{}", std::process::id()));
        std::fs::create_dir_all(&repo).unwrap();
        let init = std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(init.status.success());
        std::fs::write(repo.join("a.txt"), "hello").unwrap();
        let add = std::process::Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        assert!(add.status.success());
        let commit = std::process::Command::new("git")
            .args(["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "commit", "-m", "init"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(commit.status.success());

        let info = create(&repo, "wt1").await.unwrap();
        assert!(info.path.join("a.txt").exists());
        assert_eq!(info.branch, "kxen/wt1");
        // .gitignore 幂等
        let gi = std::fs::read_to_string(repo.join(".gitignore")).unwrap();
        assert_eq!(gi.matches(".kxen/").count(), 1);

        let trees = list(&repo).await.unwrap();
        assert_eq!(trees.len(), 1);
        assert_eq!(trees[0].name, "wt1");

        remove(&repo, "wt1", true).await.unwrap();
        assert!(list(&repo).await.unwrap().is_empty());

        std::fs::remove_dir_all(&repo).ok();
    }
}
