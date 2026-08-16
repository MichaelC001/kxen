//! 升级审批的补充判定（B2/B3）：解释器内联脚本与 git 工作区恢复形式。
//! token 级判定（regex crate 无 lookaround，「仅 --staged 安全」这类排除形态表达不了）。

use std::borrow::Cow;

/// 解释器内联脚本（python -c / node -e / perl -e / ruby -e / osascript -e）：
/// 脚本内容是一个 opaque token，脱离切段评估（osascript 可 do shell script 间接执行任意命令）。
pub(super) fn inline_script_ask(cmd: &str, tokens: &[String], cmd_idx: usize) -> Option<Cow<'static, str>> {
    const INLINE_SCRIPT_CMDS: &[&str] = &["python", "python3", "node", "perl", "ruby", "osascript"];
    if INLINE_SCRIPT_CMDS.contains(&cmd) && tokens.iter().skip(cmd_idx + 1).any(|token| matches!(token.as_str(), "-c" | "-e" | "--eval")) {
        Some(format!("{cmd} 内联脚本无法静态检查，可能执行高危操作").into())
    } else {
        None
    }
}

/// git 工作区破坏面的 token 级判定（regex crate 无 lookaround，「仅 --staged 安全」这类
/// 排除形态表达不了）：checkout --/<path>/. 与 restore 的工作区恢复形式升 Ask。
pub(super) fn git_worktree_ask(tokens: &[String], cmd_idx: usize) -> Option<&'static str> {
    // 跳过 git 全局选项（-C/-c 带值，其余 flag 直接越过）找子命令
    let mut i = cmd_idx + 1;
    while let Some(token) = tokens.get(i) {
        match token.as_str() {
            "-C" | "-c" => i += 2,
            t if t.starts_with('-') => i += 1,
            _ => break,
        }
    }
    let sub = tokens.get(i).map(String::as_str)?;
    let rest = tokens.get(i + 1..).unwrap_or_default();
    match sub {
        "checkout" => {
            // git checkout -- <path>：-- 之后是路径还原语义，丢弃文件未提交改动
            if rest.iter().any(|token| token == "--") {
                return Some("git checkout -- <path> 丢弃文件未提交改动");
            }
            // 跳过带值选项与其余 flag，首个位置参数是 . 或 ./ 即工作区恢复
            let mut j = 0;
            while let Some(token) = rest.get(j) {
                match token.as_str() {
                    "-b" | "-B" | "--orphan" | "--track" | "-t" => j += 2,
                    t if t.starts_with('-') => j += 1,
                    "." => return Some("git checkout . 丢弃当前目录全部未提交改动"),
                    t if t.starts_with("./") => return Some("git checkout <path> 丢弃文件未提交改动"),
                    _ => return None,
                }
            }
            None
        }
        "restore" => {
            // 仅 --staged 只重置索引（安全）；其余任何形式（含 --worktree/--source 或默认）
            // 都会用 HEAD/指定来源覆盖工作区文件
            let staged = rest.iter().any(|token| token == "--staged" || token == "-S");
            let worktree =
                rest.iter().any(|token| token == "--worktree" || token == "-W" || token == "--source" || token.starts_with("--source="));
            if staged && !worktree { None } else { Some("git restore 恢复工作区文件（覆盖未提交改动）") }
        }
        _ => None,
    }
}
