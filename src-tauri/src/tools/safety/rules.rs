//! 规则模式与常量（F1-F5 规则族的匹配表）。

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
pub(super) const SYSTEM_PATHS: &[&str] = &[
    "/", "/System", "/usr", "/bin", "/sbin", "/etc", "/var", "/Library", "/private/etc", "/private/var/db",
    "/private/var/root", "/private/bin", "/private/sbin", "/private/System", "/boot", "/proc", "/sys", "/dev",
];

pub(super) const EXEMPT_PREFIXES: &[&str] = &["/private/var/folders", "/private/tmp", "/dev/null", "/dev/stdout", "/dev/stderr", "/dev/tty"];

pub(super) fn home_top() -> &'static [&'static str] {
    &["Documents", "Desktop", "Downloads", "Library", "Pictures", "Movies"]
}

pub(super) fn home_credential_dot() -> &'static [&'static str] {
    &[".ssh", ".gnupg", ".aws", ".kube", ".docker"]
}

pub(super) static DISK_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
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

pub(super) static SYSTEM_CMDS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [r"\b(shutdown|reboot|halt)\b", r"\b(nvram|csrutil)\b"].iter().map(|p| Regex::new(p).expect("static pattern")).collect()
});

pub(super) static CRED_CMDS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [r"\bsecurity\s+delete-", r"\bgpg\s+--delete-secret-key"].iter().map(|p| Regex::new(p).expect("static pattern")).collect()
});

pub(super) static DESTROY_CMDS: LazyLock<Vec<(Regex, &'static str, &'static str)>> = LazyLock::new(|| {
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

pub(super) static GIT_DESTROY: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"\bgit\s+update-ref\s+-d\b", "git update-ref -d 删除 refs"),
        (r"\bgit\s+branch\s+-D\s+\*", "git branch -D 批量删除分支"),
    ]
    .iter()
    .map(|(p, why)| (Regex::new(p).expect("static pattern"), *why))
    .collect()
});

pub(super) static VAR_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\{?[A-Za-z_][A-Za-z0-9_]*\}?").unwrap());

pub(super) static GIT_SEGMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|/)\.git(/|$)").unwrap());

pub(super) const DELETE_CMDS: &[&str] = &["rm", "rmdir", "trash", "unlink", "shred"];
pub(super) const MOVE_CMDS: &[&str] = &["mv", "move"];

pub(super) fn deny(rule_id: &'static str, reason: impl Into<Cow<'static, str>>, suggestion: Option<&'static str>) -> Verdict {
    Verdict::Deny { rule_id, reason: reason.into(), suggestion }
}
