//! 统一知识系统：OKF 单规范 + project/personal 双 scope。
//! 一棵树两个镜像：project = <workdir>/.agents/（入 git 共享），personal = ~/.agents/（跟人走）。
//! rules / references / skills / commands / notes / memory / history 都是 Entry，区别只在 kind 与激活方式。

mod parse;
mod scan;
mod render;
mod store;
pub mod distill;

pub use render::render;
pub use scan::scan;
pub use store::{add, list, move_entry, remove, set_enabled};

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Project,
    Personal,
}

impl Scope {
    pub fn parse(s: &str) -> Result<Scope, String> {
        match s {
            "project" => Ok(Scope::Project),
            // global 是 personal 的旧名，外部输入一律归一
            "personal" | "global" => Ok(Scope::Personal),
            other => Err(format!("unknown scope: {other} (project|personal)")),
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::Personal => "personal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Rule,
    Reference,
    Skill,
    Command,
    Note,
    Memory,
    History,
}

impl Kind {
    pub fn from_str(s: &str) -> Option<Kind> {
        Some(match s {
            "rule" => Kind::Rule,
            "reference" | "doc" => Kind::Reference,
            "skill" => Kind::Skill,
            "command" => Kind::Command,
            "note" => Kind::Note,
            "memory" => Kind::Memory,
            "history" => Kind::History,
            _ => return None,
        })
    }
    pub fn dir_name(&self) -> &'static str {
        match self {
            Kind::Rule => "rules",
            Kind::Reference => "references",
            Kind::Skill => "skills",
            Kind::Command => "commands",
            Kind::Note => "notes",
            Kind::Memory => "memory",
            Kind::History => "history",
        }
    }
}

/// note/memory 的子类型（蒸馏与人工写入共用）。
pub const NOTE_TYPES: &[&str] = &["correction", "convention", "pitfall", "preference", "note"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub scope: Scope,
    pub kind: Kind,
    pub slug: String,
    pub description: String,
    pub content: String,
    pub path: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub always_apply: bool,
    #[serde(default)]
    pub globs: Vec<String>,
    /// skill/command 懒加载依赖：加载或展开时随正文注入的条目 slug。
    #[serde(default)]
    pub needs: Vec<String>,
    // skill 字段
    #[serde(default)]
    pub when_to_use: Option<String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub disable_model_invocation: bool,
    #[serde(default = "default_true")]
    pub user_invocable: bool,
    // command 字段
    #[serde(default)]
    pub argument_hint: Option<String>,
    // note/memory 字段
    #[serde(default)]
    pub note_type: Option<String>,
    #[serde(default)]
    pub date: String,
    /// 目录型 skill 的资源目录（SKILL.md 的父目录）。
    #[serde(default)]
    pub dir: String,
    /// 根/就近 AGENTS.md 合成的条目（不在 kind 子目录内）。
    #[serde(default)]
    pub is_agents_md: bool,
}

fn default_true() -> bool {
    true
}

pub fn scope_root(scope: Scope, workdir: &Path) -> PathBuf {
    match scope {
        Scope::Project => workdir.join(".agents"),
        Scope::Personal => dirs::home_dir().unwrap_or_else(|| PathBuf::from("~")).join(".agents"),
    }
}

pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut dash = true;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    let capped: String = trimmed.chars().take(48).collect();
    let capped = capped.trim_end_matches('-');
    if capped.is_empty() { "note".to_string() } else { capped.to_string() }
}

pub fn today() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!("{:04}-{:02}-{:02}", now.year(), now.month() as u8, now.day())
}

/// needs 解析：跨双 scope 按 slug 找条目并渲染成注入块（project 优先，与 scan 去重同序）。
pub fn resolve_needs(workdir: &Path, needs: &[String]) -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    resolve_needs_inner(workdir, &home, needs)
}

fn resolve_needs_inner(workdir: &Path, home: &Path, needs: &[String]) -> String {
    if needs.is_empty() {
        return String::new();
    }
    let entries = scan::scan_with_home(workdir, home);
    let mut out = String::from("\n<knowledge-deps>\n");
    let mut hit = 0;
    for need in needs {
        let slug = slugify(need);
        if let Some(e) = entries.iter().find(|e| e.enabled && e.slug == slug) {
            hit += 1;
            out.push_str(&format!("## [{}] {}\n{}\n\n", e.kind.dir_name(), e.description, e.content.trim()));
        }
    }
    if hit == 0 {
        return String::new();
    }
    out.push_str("</knowledge-deps>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_resolve_injects_dep_bodies() {
        let dir = std::env::temp_dir().join(format!("kxen-kn-needs-{}", std::process::id()));
        let home = dir.join("fake-home");
        let rules = dir.join(".agents/rules");
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(rules.join("style-guide.md"), "---\ndescription: 风格\n---\n用 trash 不用 rm。\n").unwrap();
        let block = resolve_needs_inner(&dir, &home, &["style-guide".into(), "missing".into()]);
        assert!(block.contains("<knowledge-deps>"));
        assert!(block.contains("用 trash 不用 rm。"));
        assert!(resolve_needs_inner(&dir, &home, &["missing".into()]).is_empty());
        assert!(resolve_needs_inner(&dir, &home, &[]).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
