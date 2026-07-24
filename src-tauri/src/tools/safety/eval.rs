//! 命令评估与路径守卫实现。

use regex::Regex;
use std::sync::LazyLock;

use super::rules::{
    ASK_PATTERNS, CRED_CMDS, DELETE_CMDS, DESTROY_CMDS, DISK_PATTERNS, EXEMPT_PREFIXES, GIT_DESTROY, GIT_SEGMENT, MOVE_CMDS, SYSTEM_CMDS,
    SYSTEM_PATHS, VAR_PATTERN, Verdict, deny, home_credential_dot, home_top,
};

/// 主入口：评估一条 shell 命令文本。cwd 用于相对路径解析。
pub fn evaluate_shell_command(command: &str, cwd: &str) -> Verdict {
    let inner = extract_nested(command).map(|i| evaluate_shell_command(i, cwd));
    if let Some(v @ Verdict::Deny { .. }) = inner {
        return v;
    }

    let mut recoverable_seen = false;
    let mut ask_seen: Option<Verdict> = None;
    let mut check = |cmd: &str| {
        for seg in split_segments(cmd) {
            match eval_segment(seg, cwd) {
                v @ Verdict::Deny { .. } => return Some(v),
                v @ Verdict::Ask { .. } => {
                    if ask_seen.is_none() {
                        ask_seen = Some(v);
                    }
                }
                Verdict::Recoverable => recoverable_seen = true,
                Verdict::Allow => {}
            }
        }
        None
    };
    if let Some(v) = check(command) {
        return v;
    }
    // 命令替换（反引号 / $()）内嵌命令同样评估（绕过通道）
    for sub in expand_substitutions(command) {
        if let Some(v) = check(&sub) {
            return v;
        }
    }
    if let Some(v) = ask_seen {
        return v;
    }
    if recoverable_seen { Verdict::Recoverable } else { Verdict::Allow }
}

fn extract_nested(command: &str) -> Option<&str> {
    static NESTED: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        vec![Regex::new(r#"(?:bash|zsh|sh|fish)\s+-c\s+["']([^"']+)["']"#).unwrap(), Regex::new(r#"\beval\s+["']([^"']+)["']"#).unwrap()]
    });
    NESTED.iter().find_map(|re| re.captures(command).and_then(|c| c.get(1)).map(|m| m.as_str()))
}

fn split_segments(command: &str) -> Vec<&str> {
    command
        .split(|c| matches!(c, ';' | '|' | '\n'))
        .flat_map(|part| part.split("&&"))
        .flat_map(|part| part.split("||"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// 命令替换展开：反引号与 $() 内嵌的命令同样要进评估（`rm -rf $(cat f)` 类绕过）。
fn expand_substitutions(command: &str) -> Vec<String> {
    static RE: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![Regex::new(r"`([^`]+)`").unwrap(), Regex::new(r"\$\(([^)]+)\)").unwrap()]);
    RE.iter()
        .flat_map(|re| re.captures_iter(command).filter_map(|c| c.get(1).map(|m| m.as_str().to_string())).collect::<Vec<_>>())
        .collect()
}

fn eval_segment(seg: &str, cwd: &str) -> Verdict {
    if DISK_PATTERNS.iter().any(|re| re.is_match(seg)) {
        return deny("F1", "磁盘级操作（dd/mkfs/erase/fdisk/parted）", None);
    }
    if SYSTEM_CMDS.iter().any(|re| re.is_match(seg)) {
        return deny("F1", "系统属性或系统级进程操作", None);
    }
    if CRED_CMDS.iter().any(|re| re.is_match(seg)) {
        return deny("F2", "凭证存储销毁（Keychain / GPG 私钥）", None);
    }
    if let Some((_, id, why)) = DESTROY_CMDS.iter().find(|(re, _, _)| re.is_match(seg)) {
        return deny(id, *why, None);
    }
    if let Some((_, why)) = GIT_DESTROY.iter().find(|(re, _)| re.is_match(seg)) {
        return deny("F3", *why, Some("删除单个分支用 git branch -d"));
    }
    let delete_verdict = eval_delete_segment(seg, cwd);
    if !matches!(delete_verdict, Verdict::Allow) {
        return delete_verdict;
    }
    // Ask 档最后判定：具体危险（Deny/Recoverable）优先于审批
    if let Some((_, why)) = ASK_PATTERNS.iter().find(|(re, _)| re.is_match(seg)) {
        return Verdict::Ask { reason: (*why).into() };
    }
    delete_verdict
}

fn tokens_of(seg: &str) -> Vec<&str> {
    seg.split_whitespace().map(|t| t.trim_matches(|c| c == '"' || c == '\'')).filter(|t| !t.is_empty()).collect()
}

fn eval_delete_segment(seg: &str, cwd: &str) -> Verdict {
    let tokens = tokens_of(seg);
    let cmd = match tokens.first().copied() {
        Some("sudo") | Some("doas") => tokens.get(1).copied().unwrap_or(""),
        Some(c) => c,
        None => "",
    };

    let is_delete = DELETE_CMDS.contains(&cmd)
        || (seg.starts_with("find ") && (seg.contains(" -delete") || seg.contains(" -exec rm") || seg.contains(" -exec trash")))
        || (seg.starts_with("rsync ") && seg.contains("--delete"));
    let is_move = MOVE_CMDS.contains(&cmd);
    if !is_delete && !is_move {
        return Verdict::Allow;
    }

    // trash 命令按可恢复降档（删除进回收站）：只拦 .git 与系统路径
    let recoverable = cmd == "trash";

    let targets: Vec<&str> = tokens.iter().skip(1).filter(|t| !t.starts_with('-')).copied().collect();

    if targets.is_empty() && is_delete && (seg.contains("-r") || seg.contains("-f")) {
        return deny("F5", "递归/强制删除缺少可静态确定的目标路径", Some("明确写出完整目标路径后再执行"));
    }

    for target in targets {
        if VAR_PATTERN.is_match(target) {
            return deny("F5", format!("删除/移动目标含未求值变量 {target}，无法静态判定"), Some("先 echo 展开确认实际路径"));
        }
        if let Some(hit) = classify_path(target, cwd) {
            if recoverable && hit.family == Family::Home {
                continue; // trash 的用户目录删除可恢复，放行
            }
            let rule = match hit.family {
                Family::Git => "F3",
                Family::Home | Family::Credential => "F2",
                Family::System => "F1",
            };
            return deny(
                rule,
                format!("{cmd} 的目标 {target} 命中保护路径 {}", hit.guard),
                Some("工作区内的具体子路径操作不受限，请缩小范围"),
            );
        }
    }

    if recoverable {
        return Verdict::Recoverable;
    }
    Verdict::Allow
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    System,
    Home,
    Credential,
    Git,
}

struct PathHit {
    family: Family,
    guard: std::borrow::Cow<'static, str>,
}

fn classify_path(target: &str, cwd: &str) -> Option<PathHit> {
    let norm = normalize_path(target, cwd);

    if EXEMPT_PREFIXES.iter().any(|p| norm == *p || norm.starts_with(&format!("{p}/"))) {
        return None;
    }
    if GIT_SEGMENT.is_match(&norm) {
        return Some(PathHit { family: Family::Git, guard: ".git".into() });
    }
    for guard in SYSTEM_PATHS {
        if *guard == "/" {
            if norm == "/" {
                return Some(PathHit { family: Family::System, guard: "/".into() });
            }
            continue;
        }
        if norm == *guard || norm.starts_with(&format!("{guard}/")) || guard.starts_with(&format!("{norm}/")) {
            return Some(PathHit { family: Family::System, guard: (*guard).into() });
        }
    }
    let home = dirs::home_dir()?;
    let home_str = home.to_string_lossy();
    if norm == home_str {
        return Some(PathHit { family: Family::Home, guard: home_str.to_string().into() });
    }
    for dot in home_credential_dot() {
        let guard = format!("{home_str}/{dot}");
        if norm == guard || norm.starts_with(&format!("{guard}/")) {
            return Some(PathHit { family: Family::Credential, guard: guard.into() });
        }
    }
    for top in home_top() {
        let guard = format!("{home_str}/{top}");
        if norm == guard {
            return Some(PathHit { family: Family::Home, guard: guard.into() });
        }
    }
    for rc in [".zshrc", ".bashrc", ".bash_profile", ".zprofile", ".profile"] {
        let guard = format!("{home_str}/{rc}");
        if norm == guard {
            return Some(PathHit { family: Family::Home, guard: guard.into() });
        }
    }
    // .config：拦整体删除，内容放行
    let guard = format!("{home_str}/.config");
    if norm == guard {
        return Some(PathHit { family: Family::Home, guard: guard.into() });
    }
    None
}

fn normalize_path(target: &str, cwd: &str) -> String {
    let home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default();
    // macOS /var、/tmp 是 /private/* 软链：cwd 先 canonicalize，否则临时区被误判为系统区
    let cwd_canon = std::fs::canonicalize(cwd).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| cwd.to_string());
    let mut s = if target == "~" {
        home.clone()
    } else if let Some(rest) = target.strip_prefix("~/") {
        format!("{home}/{rest}")
    } else if target.starts_with('/') {
        std::fs::canonicalize(target).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| target.to_string())
    } else {
        format!("{cwd_canon}/{target}")
    };
    // 解析 . 与 .. 与多余斜杠
    let mut parts: Vec<&str> = Vec::new();
    for seg in s.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    s = format!("/{}", parts.join("/"));
    s
}

/// 路径守卫（write/edit/delete 的最终防线）。
pub fn guard_path(target: &str, cwd: &str) -> Verdict {
    match classify_path(target, cwd) {
        None => Verdict::Allow,
        Some(hit) => {
            let rule = match hit.family {
                Family::Git => "F3",
                Family::Home | Family::Credential => "F2",
                Family::System => "F1",
            };
            deny(rule, format!("路径 {target} 命中保护路径 {}", hit.guard), Some("工作区内的具体子路径操作不受限，请缩小范围"))
        }
    }
}
