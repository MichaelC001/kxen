//! 灾难操作防护（F1-F5 规则族 + 命令解析 + 路径守卫 + trash 降档）。
//! 热路径零分配：&str 切片 + OnceLock 预编译 Regex。
//! 规则文档：docs/rules/safety-rules.md

use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny {
        rule_id: &'static str,
        reason: Cow<'static, str>,
        suggestion: Option<&'static str>,
    },
    /// trash 的可恢复删除（approval 档，safety 不硬拦但记录）
    Recoverable,
}

// F1 系统路径（macOS 细化：/private/var/folders 与 /private/tmp 是临时区，豁免）
const SYSTEM_PATHS: &[&str] = &[
    "/", "/System", "/usr", "/bin", "/sbin", "/etc", "/var", "/Library", "/private/etc", "/private/var/db",
    "/private/var/root", "/private/bin", "/private/sbin", "/private/System", "/boot", "/proc", "/sys", "/dev",
];

const EXEMPT_PREFIXES: &[&str] = &["/private/var/folders", "/private/tmp", "/dev/null", "/dev/stdout", "/dev/stderr", "/dev/tty"];

fn home_top() -> &'static [&'static str] {
    &["Documents", "Desktop", "Downloads", "Library", "Pictures", "Movies"]
}

fn home_credential_dot() -> &'static [&'static str] {
    &[".ssh", ".gnupg", ".aws", ".kube", ".docker"]
}

static DISK_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"^\s*/?dd\b.*\bof=/dev/",
        r"\bmkfs(\.|\b)",
        r"\bdiskutil\s+erase",
        r"\bhdiutil\s+erase",
        r"\bfdisk\b",
        r"\bparted\b",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("static pattern"))
    .collect()
});

static SYSTEM_CMDS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [r"\b(shutdown|reboot|halt)\b", r"\b(nvram|csrutil)\b"].iter().map(|p| Regex::new(p).expect("static pattern")).collect()
});

static CRED_CMDS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [r"\bsecurity\s+delete-", r"\bgpg\s+--delete-secret-key"].iter().map(|p| Regex::new(p).expect("static pattern")).collect()
});

static DESTROY_CMDS: LazyLock<Vec<(Regex, &'static str, &'static str)>> = LazyLock::new(|| {
    [
        (r"\bterraform\s+destroy\b", "F4", "terraform destroy 销毁基础设施"),
        (r"\bdropdb\b", "F4", "dropdb 删除整个数据库"),
        (r"\b(psql|mysql|mongosh?|mongo|redis-cli)\b.*\b(drop\s+database|dropDatabase|flushall)", "F4", "数据库毁灭操作"),
        (r"\bkubectl\s+delete\s+(ns|namespace|--all)\b", "F4", "kubectl 命名空间/全量删除"),
        (r"\baws\s+s3\s+rb\s+.*--force\b", "F4", "aws s3 rb --force 删除整个 bucket"),
        (r"\bgcloud\s+projects\s+delete\b", "F4", "gcloud 项目删除"),
        (r"\bdocker\s+system\s+prune\b.*(--volumes|-a\b)", "F4", "docker system prune 卷/全量清理"),
    ]
    .iter()
    .map(|(p, id, why)| (Regex::new(p).expect("static pattern"), *id, *why))
    .collect()
});

static GIT_DESTROY: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"\bgit\s+update-ref\s+-d\b", "git update-ref -d 删除 refs"),
        (r"\bgit\s+branch\s+-D\s+\*", "git branch -D 批量删除分支"),
    ]
    .iter()
    .map(|(p, why)| (Regex::new(p).expect("static pattern"), *why))
    .collect()
});

static VAR_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\{?[A-Za-z_][A-Za-z0-9_]*\}?").unwrap());

static GIT_SEGMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|/)\.git(/|$)").unwrap());

const DELETE_CMDS: &[&str] = &["rm", "rmdir", "trash", "unlink", "shred"];
const MOVE_CMDS: &[&str] = &["mv", "move"];

fn deny(rule_id: &'static str, reason: impl Into<Cow<'static, str>>, suggestion: Option<&'static str>) -> Verdict {
    Verdict::Deny { rule_id, reason: reason.into(), suggestion }
}

/// 主入口：评估一条 shell 命令文本。cwd 用于相对路径解析。
pub fn evaluate_shell_command(command: &str, cwd: &str) -> Verdict {
    let inner = extract_nested(command).map(|i| evaluate_shell_command(i, cwd));
    if let Some(v @ Verdict::Deny { .. }) = inner {
        return v;
    }

    let mut recoverable_seen = false;
    for seg in split_segments(command) {
        match eval_segment(seg, cwd) {
            v @ Verdict::Deny { .. } => return v,
            Verdict::Recoverable => recoverable_seen = true,
            Verdict::Allow => {}
        }
    }
    if recoverable_seen { Verdict::Recoverable } else { Verdict::Allow }
}

fn extract_nested(command: &str) -> Option<&str> {
    static NESTED: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        vec![
            Regex::new(r#"(?:bash|zsh|sh|fish)\s+-c\s+["']([^"']+)["']"#).unwrap(),
            Regex::new(r#"\beval\s+["']([^"']+)["']"#).unwrap(),
        ]
    });
    NESTED.iter().find_map(|re| re.captures(command).and_then(|c| c.get(1)).map(|m| m.as_str()))
}

fn split_segments(command: &str) -> Vec<&str> {
    command
        .split(|c| matches!(c, ';' | '|'))
        .flat_map(|part| part.split("&&"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
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
    eval_delete_segment(seg, cwd)
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

    let targets: Vec<&str> = tokens
        .iter()
        .skip(1)
        .filter(|t| !t.starts_with('-'))
        .copied()
        .collect();

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

#[cfg(test)]
mod tests {
    use super::*;

    const CWD: &str = "/Users/test/project";

    fn denied(cmd: &str) -> bool {
        matches!(evaluate_shell_command(cmd, CWD), Verdict::Deny { .. })
    }

    fn allowed(cmd: &str) -> bool {
        matches!(evaluate_shell_command(cmd, CWD), Verdict::Allow | Verdict::Recoverable)
    }

    #[test]
    fn f1_system() {
        for cmd in ["rm -rf /", "rm -rf /usr", "sudo rm -rf /etc", "dd if=/dev/zero of=/dev/disk0", "mkfs.ext4 /dev/sda1", "diskutil eraseDisk JHFS+ New disk0", "find / -name x -delete"] {
            assert!(denied(cmd), "should deny: {cmd}");
        }
    }

    #[test]
    fn macos_temp_exempt() {
        assert!(allowed("rm -rf /private/var/folders/qb/xxx/T/test"));
        assert!(allowed("rm -rf /private/tmp/foo"));
        assert!(allowed("rm -rf /tmp/foo"));
        assert!(denied("rm -rf /private/etc"));
        assert!(denied("rm -rf /private/var/db"));
    }

    #[test]
    fn f2_home() {
        assert!(denied("rm -rf ~"));
        assert!(denied("rm -rf ~/Documents"));
        assert!(denied("trash ~/.ssh"));
        assert!(allowed("rm ~/Documents/draft.txt"));
    }

    #[test]
    fn f3_git() {
        assert!(denied("rm -rf .git"));
        assert!(denied("mv .git /tmp/trash"));
        assert!(denied("git update-ref -d refs/heads/main"));
        assert!(allowed("git reset --hard HEAD"));
        assert!(allowed("git branch -d feature-x"));
    }

    #[test]
    fn f4_destroy() {
        for cmd in ["terraform destroy", "dropdb production", "kubectl delete ns prod", "aws s3 rb s3://b --force", "docker system prune --volumes"] {
            assert!(denied(cmd), "should deny: {cmd}");
        }
    }

    #[test]
    fn f5_bypass() {
        assert!(denied("bash -c \"rm -rf /usr\""));
        assert!(denied("rm -rf $DIR/"));
    }

    #[test]
    fn trash_recoverable() {
        assert!(matches!(evaluate_shell_command("trash ./dist", CWD), Verdict::Recoverable));
        assert!(denied("trash .git"));
        assert!(denied("trash ~/.ssh"));
    }

    #[test]
    fn guard() {
        assert!(matches!(guard_path("~/.ssh/id_rsa", CWD), Verdict::Deny { .. }));
        assert!(matches!(guard_path(".git/config", CWD), Verdict::Deny { .. }));
        assert!(matches!(guard_path("src/index.ts", CWD), Verdict::Allow));
    }
}
