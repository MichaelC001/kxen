//! 每卡 worktree 隔离（P4）：列执行在每卡专属 git worktree 内跑，卡片间并发执行互不踩踏。
//!
//! 惰性分配：worktree 在 driver claim/adopt 之后、执行之前分配，不在 card_create 分配——card_create
//! 是纯事件命令（同步、无 git 依赖），纯任务跟踪的非 git workspace 不能因此坏掉；隔离只在执行时有意义。
//! worktree 名/路径由 card_id 确定性派生（card-<card_id>），无需事件记录分配状态。
//!
//! 终态 detach：卡片落入无出边列后，先快照抢救未提交/被 gitignore 的产物到看板存储，再释放 worktree
//! 目录、保留分支（merge/rebase 回主树是人的事，在看板 review 列完成）。
//! 隔离是纵深防御不是安全边界（安全边界在 safety_gate）：非 git workspace 优雅降级为 workspace 根。

use std::path::{Path, PathBuf};

use super::command::Board;
use super::land;

/// ignored 单文件快照上限：构建产物可巨大，抢救不无界；超限记 manifest skipped 由人处理。
const MAX_IGNORED_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// 单卡执行工作目录：git workspace = 卡专属 worktree；非 git = workspace 根（降级）。
pub enum CardWorkdir {
    Worktree(PathBuf),
    WorkspaceRoot,
}

/// worktree 名 = card-<card_id>。card_id 来自事件流（ids::new_id 生成，是可信源），
/// 但路径拼接仍走白名单校验（与 tools/worktree.rs validate_name 同规则，杜绝路径穿越）。
fn worktree_name(card_id: &str) -> Result<String, String> {
    let name = format!("card-{card_id}");
    crate::tools::worktree::validate_name(&name)?;
    Ok(name)
}

/// 惰性分配单卡 worktree：非 git workspace 返回 WorkspaceRoot（优雅降级），git 错误如实上抛。
pub async fn ensure_card_worktree(workspace: &Path, card_id: &str) -> Result<CardWorkdir, String> {
    // 先判 git 仓库：tools/worktree create 会先写 .gitignore，非 git workspace 不能被它污染
    let is_git = tokio::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(workspace)
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false); // git 不可用按非 git 降级
    if !is_git {
        return Ok(CardWorkdir::WorkspaceRoot);
    }
    Ok(CardWorkdir::Worktree(crate::tools::worktree::create(workspace, &worktree_name(card_id)?).await?.path))
}

/// driver 分配编排：非 git 降级为 workspace 根并落审计评论；git 错误由调用方按 Config 裁定。
pub async fn allocate(workspace: &Path, board_id: &str, card_id: &str) -> Result<PathBuf, String> {
    match ensure_card_worktree(workspace, card_id).await? {
        CardWorkdir::Worktree(path) => Ok(path),
        CardWorkdir::WorkspaceRoot => {
            land::comment(workspace, board_id, card_id, "workspace 不是 git 仓库，本次 run 无 worktree 隔离".into(), "kanban-driver");
            Ok(workspace.to_path_buf())
        }
    }
}

/// detach 报告：产物清单 + 快照目录（审计评论与 manifest.json 共用同一份数据）。
pub struct DetachReport {
    pub collected: Vec<String>,
    pub skipped: Vec<SkippedEntry>,
    pub files_dir: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SkippedEntry {
    pub path: String,
    pub reason: String,
}

/// 终态 detach：先快照抢救产物，再释放 worktree 目录（分支永不删）。
pub async fn detach(workspace: &Path, board_id: &str, card_id: &str) -> Result<DetachReport, String> {
    let name = worktree_name(card_id)?;
    let worktree = workspace.join(".kxen").join("worktrees").join(&name);
    let report = snapshot_artifacts(workspace, board_id, card_id, &worktree, MAX_IGNORED_FILE_BYTES).await?;
    // confirmed=true 的 WHY：快照已先抢救、分支保留（delete_branch=false 永不删），终态 detach 的 dirty
    // 是预期内（agent 未提交产物）；人工裁断发生在看板 human_gate 列，不在 git 层重复确认
    crate::tools::worktree::remove_with_approval(workspace, &name, false, None, true).await?;
    Ok(report)
}

/// land_finished 成功后的终态收口：卡片当前列无出边（terminal）则 detach。
/// detach 失败只落评论不翻盘：outcome 已 durable，清理失败留残骸由人处理，fail-closed 不适用于已落地结果。
/// timeout/blocked/landing 失败不进这里：worktree 保留，显式重试继续用。
pub async fn detach_if_terminal(workspace: &Path, board_id: &str, card_id: &str) {
    let terminal = Board::open(workspace, board_id).ok().and_then(|board| {
        let card = board.state().cards.get(card_id)?;
        let column = board.state().column(&card.column_id)?;
        Some(column.transitions.on_success.is_none() && column.transitions.on_failure.is_none())
    });
    if terminal != Some(true) {
        return;
    }
    // 非 git workspace 的降级 run 没有 worktree 可拆；重复调用幂等（已拆过直接返回）
    if !workspace.join(".kxen").join("worktrees").join(format!("card-{card_id}")).exists() {
        return;
    }
    let note = match detach(workspace, board_id, card_id).await {
        Ok(report) if report.collected.is_empty() => "worktree 已释放（无待抢救产物，分支保留）".to_string(),
        Ok(report) => {
            let mut note = format!("worktree 已释放（分支保留），{} 个产物快照到 {}", report.collected.len(), report.files_dir.display());
            if !report.skipped.is_empty() {
                note.push_str(&format!("；{} 项未抢救（原因见 manifest.json）", report.skipped.len()));
            }
            note
        }
        Err(error) => format!("worktree detach 失败，目录与分支保留待人工处理: {error}"),
    };
    land::comment(workspace, board_id, card_id, note, "kanban-driver");
}

/// 快照抢救：worktree 内未提交/被 gitignore 的产物镜像到
/// `<workspace>/.kxen/kanban/<board_id>/artifacts/<card_id>/files/`，并写 manifest.json。
/// porcelain -z 三类行：?? 未跟踪、!! 被忽略、其余 = 已跟踪改动。
/// ?? 与改动文件全收（?? 目录递归）；!! 只收 <= max_ignored_bytes 的文件，!! 目录不递归（node_modules 防爆）记 skipped。
/// 拷贝失败 = Err：抢救失败不能静默丢。card_id 拼路径前已过 worktree_name 白名单（detach 入口）。
async fn snapshot_artifacts(
    workspace: &Path,
    board_id: &str,
    card_id: &str,
    worktree: &Path,
    max_ignored_bytes: u64,
) -> Result<DetachReport, String> {
    let out = git(worktree, &["status", "--porcelain", "-z", "--ignored=traditional"]).await?;
    let artifact_dir = super::store::board_dir(workspace, board_id).map_err(|e| e.to_string())?.join("artifacts").join(card_id);
    let files_dir = artifact_dir.join("files");
    let mut collected: Vec<String> = Vec::new();
    let mut skipped: Vec<SkippedEntry> = Vec::new();
    for (code, path) in parse_porcelain_z(&out) {
        let source = worktree.join(&path);
        match code.as_str() {
            "!!" if path.ends_with('/') => {
                skipped.push(SkippedEntry { path, reason: "ignored 目录不递归快照".into() });
            }
            "!!" => match std::fs::metadata(&source) {
                Ok(meta) if meta.len() <= max_ignored_bytes => copy_file(&source, &files_dir, &path, &mut collected)?,
                Ok(_) => {
                    skipped.push(SkippedEntry { path, reason: format!("ignored 文件超过单文件上限 {max_ignored_bytes} 字节") });
                }
                Err(error) => return Err(format!("stat {}: {error}", source.display())),
            },
            _ if path.ends_with('/') => collect_untracked_dir(&source, worktree, &files_dir, &mut collected)?,
            _ if source.is_file() => copy_file(&source, &files_dir, &path, &mut collected)?,
            _ => {
                skipped.push(SkippedEntry { path, reason: "源文件不存在（已删除），无可快照内容".into() });
            }
        }
    }
    std::fs::create_dir_all(&artifact_dir).map_err(|e| format!("create {}: {e}", artifact_dir.display()))?;
    let manifest = serde_json::json!({ "card_id": card_id, "collected": collected, "skipped": skipped });
    let manifest = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(artifact_dir.join("manifest.json"), manifest).map_err(|e| format!("write manifest: {e}"))?;
    Ok(DetachReport { collected, skipped, files_dir })
}

/// porcelain -z 输出解析：NUL 分隔、路径不加 C 引号（无反转义问题）；R/C 条目后跟一个源路径段，跳过。
/// 返回（状态两列, 路径）列表，快照只需要重命名后的新路径。
fn parse_porcelain_z(output: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let mut segments = output.split('\0').filter(|s| !s.is_empty());
    while let Some(segment) = segments.next() {
        if segment.len() <= 3 {
            continue;
        }
        let code = &segment[..2];
        entries.push((code.to_string(), segment[3..].to_string()));
        if code.contains('R') || code.contains('C') {
            segments.next();
        }
    }
    entries
}

/// 镜像拷贝单文件到 files/<相对路径>：父目录按需创建，IO 失败上抛（抢救失败不静默）。
fn copy_file(source: &Path, files_dir: &Path, rel: &str, collected: &mut Vec<String>) -> Result<(), String> {
    let dest = files_dir.join(rel);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::copy(source, &dest).map_err(|e| format!("copy {}: {e}", source.display()))?;
    collected.push(rel.to_string());
    Ok(())
}

/// 未跟踪目录递归全收：porcelain 把整目录折叠成一条 `dir/` 条目，内容文件不逐条列出。
fn collect_untracked_dir(dir: &Path, root: &Path, files_dir: &Path, collected: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            collect_untracked_dir(&path, root, files_dir, collected)?;
        } else {
            let rel = path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().into_owned();
            copy_file(&path, files_dir, &rel, collected)?;
        }
    }
    Ok(())
}

async fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = tokio::process::Command::new("git").args(args).current_dir(dir).output().await.map_err(|e| format!("git spawn: {e}"))?;
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

#[cfg(test)]
#[path = "worktree/tests.rs"]
mod tests;
