//! 会话/workspace 级审批规则：ApprovalCard 的「本会话放行」「总是放行」落点。
//! 匹配语义与 kanban PolicySpec 同口径：trim 后前缀 + 词边界 + 禁 shell 元字符 + max_uses + expires_at。
//! session 规则纯内存（随进程生命周期）；workspace 规则持久化到 `<workspace>/.agents/kxen/approval-rules.json`。
//! fail-closed 顺序不变：Deny 判定永远先于规则表（safety_gate 内先评估后查表）；
//! 命中规则先写 durable 审计（Part::Approval decision=rule_allow）再放行，审计失败即不自动放行。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleScope {
    Session,
    Workspace,
}

impl RuleScope {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "session" => Some(Self::Session),
            "workspace" => Some(Self::Workspace),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRule {
    pub id: String,
    /// trim 后的命令前缀（存储的即 trim 后的：装饰空白不该改变授权面）
    pub prefix: String,
    pub scope: RuleScope,
    /// session 规则的归属会话；workspace 规则按文件归属 workspace，此字段为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub created_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u32>,
    #[serde(default)]
    pub used: u32,
    /// 触发建规的审批理由（设置页规则列表展示用）
    #[serde(default)]
    pub reason: String,
}

/// shell 元字符：命中即不可自动放行（与 kanban AutoApproved 守卫同一清单）——
/// 元字符意味着前缀命中之外还藏着第二段动作。
fn has_metacharacters(text: &str) -> bool {
    text.bytes().any(|b| matches!(b, b';' | b'&' | b'|' | b'\n' | b'\r' | b'`' | b'$' | b'(' | b')' | b'<' | b'>' | b'\\'))
}

/// 建规校验：空前缀拒绝；含元字符的前缀永远命中不了（命中检查会拒），只能是误配。
pub fn validate_prefix(prefix: &str) -> Result<String, String> {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Err("规则前缀不能为空".into());
    }
    if has_metacharacters(prefix) {
        return Err("命令含 shell 元字符（; & | 换行 反引号 $ 括号 重定向 反斜杠），不能建自动放行规则".into());
    }
    Ok(prefix.to_string())
}

/// 匹配判定：词边界（前缀后必须是串结束或 ASCII 空白，否则 "git" 会放行 "gitx upload"）
/// + 禁元字符（复合命令永不自动放行）。
pub fn matches(rule: &ApprovalRule, command: &str) -> bool {
    let head = command.trim_start();
    let hit = head.strip_prefix(rule.prefix.as_str()).is_some_and(|rest| rest.is_empty() || rest.as_bytes()[0].is_ascii_whitespace());
    hit && !has_metacharacters(head)
}

/// 活性判定：过期/耗尽的规则不再命中（惰性清理由调用方做）。
pub fn alive(rule: &ApprovalRule, now_ms: u64) -> bool {
    if rule.expires_at_ms.is_some_and(|expires| now_ms > expires) {
        return false;
    }
    if let Some(max) = rule.max_uses
        && rule.used >= max
    {
        return false;
    }
    true
}

pub fn rules_file(workspace: &Path) -> PathBuf {
    crate::core::paths::KxenPaths::project(workspace).approval_rules_file()
}

pub fn load_workspace_rules(workspace: &Path) -> Result<Vec<ApprovalRule>, String> {
    crate::core::ignore_manager::prepare_project(&crate::core::paths::KxenPaths::project(workspace))?;
    let path = rules_file(workspace);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("read {}: {error}", path.display())),
    };
    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

/// 原子写：tmp + rename，写一半的文件绝不成为真源。
pub fn save_workspace_rules(workspace: &Path, rules: &[ApprovalRule]) -> Result<(), String> {
    crate::core::ignore_manager::prepare_project(&crate::core::paths::KxenPaths::project(workspace))?;
    let path = rules_file(workspace);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let text = serde_json::to_string_pretty(rules).map_err(|error| error.to_string())?;
    let result = (|| {
        std::fs::write(&tmp, text).map_err(|error| format!("write {}: {error}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|error| format!("replace {}: {error}", path.display()))
    })();
    if result.is_err() {
        std::fs::remove_file(&tmp).ok();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(prefix: &str) -> ApprovalRule {
        ApprovalRule {
            id: "r1".into(),
            prefix: prefix.into(),
            scope: RuleScope::Session,
            session_id: Some("s1".into()),
            created_at_ms: 0,
            expires_at_ms: None,
            max_uses: None,
            used: 0,
            reason: String::new(),
        }
    }

    #[test]
    fn prefix_word_boundary() {
        assert!(matches(&rule("git push --force"), "git push --force origin main"));
        assert!(matches(&rule("git status"), "git status"));
        assert!(matches(&rule("git status"), "  git status"));
        assert!(!matches(&rule("git"), "gitx upload"), "词边界：git 不得放行 gitx");
        assert!(!matches(&rule("git push"), "git pull"));
    }

    #[test]
    fn metacharacters_never_match() {
        assert!(!matches(&rule("git status"), "git status; rm -rf x"));
        assert!(!matches(&rule("git status"), "git status && echo hi"));
        assert!(!matches(&rule("git status"), "git status | head"));
        assert!(!matches(&rule("git status"), "git status $(whoami)"));
    }

    #[test]
    fn prefix_validation() {
        assert!(validate_prefix("  git status  ").is_ok());
        assert!(validate_prefix("").is_err());
        assert!(validate_prefix("git status; ls").is_err());
    }

    #[test]
    fn aliveness() {
        let mut r = rule("git status");
        assert!(alive(&r, 100));
        r.expires_at_ms = Some(100);
        assert!(!alive(&r, 101));
        assert!(alive(&r, 100));
        r.expires_at_ms = None;
        r.max_uses = Some(2);
        r.used = 2;
        assert!(!alive(&r, 50));
    }

    #[test]
    fn workspace_rules_roundtrip() {
        let dir = std::env::temp_dir().join(format!("kxen-rules-{}-{}", std::process::id(), crate::core::shared::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load_workspace_rules(&dir).unwrap().is_empty());
        save_workspace_rules(&dir, &[rule("git status")]).unwrap();
        let loaded = load_workspace_rules(&dir).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].prefix, "git status");
        std::fs::remove_dir_all(&dir).ok();
    }
}
