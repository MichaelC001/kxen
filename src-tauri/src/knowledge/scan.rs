//! 统一扫描：project 树在前 personal 树在后，同 (kind, slug) first-wins（项目覆盖个人）。
//! skills/ 特殊：目录型只认 SKILL.md（目录内其余 .md 是资源），扁平 .md 直接收。

use super::{parse::parse_entry, Entry, Kind, Scope};
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 8;

pub fn scan(workdir: &Path) -> Vec<Entry> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    scan_with_home(workdir, &home)
}

/// home 抽参：测试用假 home，避免扫真实 ~/.agents。
pub(super) fn scan_with_home(workdir: &Path, home: &Path) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    // 根规则文件互操作：AGENTS.md 是主约定，CLAUDE.md/GEMINI.md/.cursorrules 同等注入
    for name in ["AGENTS.md", "CLAUDE.md", "GEMINI.md", ".cursorrules"] {
        let path = workdir.join(name);
        if let Ok(text) = std::fs::read_to_string(&path) {
            let mut e = parse_entry(Scope::Project, Kind::Rule, &path, &text);
            e.always_apply = true;
            e.is_agents_md = true;
            e.description = format!("root {name}");
            out.push(e);
        }
    }
    walk(&workdir.join(".agents"), Scope::Project, &mut out);
    walk(&home.join(".agents"), Scope::Personal, &mut out);
    out
}

fn walk(root: &Path, scope: Scope, out: &mut Vec<Entry>) {
    if !root.is_dir() {
        return;
    }
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // 目录型 skill：收 SKILL.md 即停，不深入资源文件
                let skill_md = path.join("SKILL.md");
                if kind_of(root, &path) == Kind::Skill && skill_md.exists() {
                    if let Ok(text) = std::fs::read_to_string(&skill_md) {
                        push_unique(out, parse_entry(scope, Kind::Skill, &skill_md, &text));
                    }
                } else {
                    stack.push((path, depth + 1));
                }
            } else if path.extension().is_some_and(|x| x == "md") {
                if path.file_name().is_some_and(|n| n == "SKILL.md") {
                    continue; // 已在目录分支处理
                }
                let kind = kind_of(root, &path);
                if let Ok(text) = std::fs::read_to_string(&path) {
                    let e = parse_entry(scope, kind, &path, &text);
                    // skill 无 description 不可被清单/调用发现，按规范跳过
                    if e.kind == Kind::Skill && e.description.is_empty() {
                        continue;
                    }
                    push_unique(out, e);
                }
            }
        }
    }
}

/// kind 由 scope 根下第一级子目录推断；根散文件与未知子目录按 Reference（可被 frontmatter 覆盖）。
fn kind_of(root: &Path, path: &Path) -> Kind {
    let Some(first) = path
        .strip_prefix(root)
        .ok()
        .and_then(|rel| rel.components().next())
        .and_then(|c| c.as_os_str().to_str())
    else {
        return Kind::Reference;
    };
    // 复数目录名优先按单数解析（rules->rule），失败再按原名（history 这类以 s 结尾的）
    Kind::from_str(first.trim_end_matches('s'))
        .or_else(|| Kind::from_str(first))
        .unwrap_or(Kind::Reference)
}

fn push_unique(out: &mut Vec<Entry>, e: Entry) {
    if !out.iter().any(|x| x.kind == e.kind && x.slug == e.slug) {
        out.push(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kxen-kn-scan-{tag}-{}", std::process::id()));
        let rules = dir.join(".agents/rules");
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(rules.join("style.md"), "---\nalwaysApply: true\ndescription: 风格\n---\n用 trash。\n").unwrap();
        let skills = dir.join(".agents/skills/review");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::write(skills.join("SKILL.md"), "---\ndescription: 对抗审查\n---\n审查 $1\n").unwrap();
        std::fs::write(skills.join("checklist.md"), "资源文件不应成为条目").unwrap();
        dir
    }

    #[test]
    fn kinds_from_subdirs_and_skill_dir_resource_skipped() {
        let dir = fixture("kinds");
        let entries = scan(&dir);
        assert!(entries.iter().any(|e| e.kind == Kind::Rule && e.slug == "style"));
        assert!(entries.iter().any(|e| e.kind == Kind::Skill && e.slug == "review"));
        assert!(!entries.iter().any(|e| e.slug == "checklist"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn interop_root_rule_files() {
        let dir = std::env::temp_dir().join(format!("kxen-kn-interop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("CLAUDE.md"), "claude 专属规则").unwrap();
        std::fs::write(dir.join(".cursorrules"), "cursor 专属规则").unwrap();
        let home = dir.join("fake-home");
        let entries = scan_with_home(&dir, &home);
        assert!(entries.iter().any(|e| e.is_agents_md && e.description.contains("CLAUDE.md")));
        assert!(entries.iter().any(|e| e.is_agents_md && e.description.contains(".cursorrules")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn project_wins_over_personal_same_slug() {
        let dir = fixture("wins");
        let home = dir.join("fake-home");
        let personal_rules = home.join(".agents/rules");
        std::fs::create_dir_all(&personal_rules).unwrap();
        std::fs::write(personal_rules.join("style.md"), "---\ndescription: 个人版\n---\n个人内容\n").unwrap();
        let entries = scan_with_home(&dir, &home);
        let styles: Vec<&Entry> = entries.iter().filter(|e| e.slug == "style").collect();
        assert_eq!(styles.len(), 1, "同 (kind, slug) first-wins 去重");
        assert_eq!(styles[0].scope, Scope::Project);
        assert!(styles[0].content.contains("用 trash。"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
