//! OKF bundle 扫描：目录层级只形成 concept id，frontmatter `type` 决定运行 handler。
//! project 树在前 personal 树在后，同 concept id first-wins（项目覆盖个人）。

use super::{Entry, Kind, Scope, parse::parse_entry};
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 8;
const MAX_FILE_BYTES: usize = 256 * 1024;
pub(super) const MAX_TOTAL_BYTES: usize = 2 * 1024 * 1024;

pub fn scan(workdir: &Path) -> Vec<Entry> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/var/empty"));
    scan_with_home(workdir, &home)
}

/// home 抽参：测试用假 home，避免扫真实 ~/.agents。
pub(super) fn scan_with_home(workdir: &Path, home: &Path) -> Vec<Entry> {
    let mut unique = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in scan_all_with_home(workdir, home) {
        if seen.insert(entry.concept_id.clone()) {
            unique.push(entry);
        }
    }
    unique
}

/// 管理视图必须保留双 scope 的同名条目；注入视图再按 project-first 做优先级去重。
pub(super) fn scan_all(workdir: &Path) -> Vec<Entry> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/var/empty"));
    scan_all_with_home(workdir, &home)
}

pub(super) fn scan_all_with_home(workdir: &Path, home: &Path) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    let mut remaining = MAX_TOTAL_BYTES;
    let workspace_root = workdir.canonicalize().ok();
    // 根规则文件互操作：AGENTS.md 是主约定，CLAUDE.md/GEMINI.md/.cursorrules 同等注入
    if let Some(workspace_root) = workspace_root.as_deref() {
        for name in ["AGENTS.md", "CLAUDE.md", "GEMINI.md", ".cursorrules"] {
            let path = workdir.join(name);
            if let Some(text) = read_regular_utf8_within(&path, workspace_root, &mut remaining) {
                let mut e = parse_entry(Scope::Project, Kind::Rule, &path, &text);
                e.concept_id = name.to_string();
                e.always_apply = true;
                e.is_agents_md = true;
                e.description = format!("root {name}");
                out.push(e);
            }
        }
    }
    walk(&workdir.join(".agents"), Scope::Project, &mut out, &mut remaining);
    walk(&home.join(".agents"), Scope::Personal, &mut out, &mut remaining);
    out
}

fn walk(root: &Path, scope: Scope, out: &mut Vec<Entry>, remaining: &mut usize) {
    let Ok(root_metadata) = std::fs::symlink_metadata(root) else {
        return;
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return;
    }
    let Ok(canonical_root) = root.canonicalize() else {
        return;
    };
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_DEPTH || *remaining == 0 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            let Ok(canonical_path) = path.canonicalize() else {
                continue;
            };
            if !canonical_path.starts_with(&canonical_root) {
                continue;
            }
            if metadata.is_dir() {
                if depth == 0 && crate::core::paths::KxenPaths::is_runtime_namespace_entry(root, &path) {
                    continue;
                }
                // 目录型 skill 由 SKILL.md 内的 type 声明，不依赖所在目录名。
                let skill_md = path.join("SKILL.md");
                let before_probe = *remaining;
                if let Some(text) = read_regular_utf8_within(&skill_md, &canonical_root, remaining) {
                    let mut concept = parse_entry(scope, legacy_kind_hint(root, &skill_md), &skill_md, &text);
                    if concept.kind == Kind::Skill {
                        assign_identity(root, &skill_md, &mut concept);
                        if !concept.description.is_empty() {
                            out.push(concept);
                        }
                        continue;
                    }
                    // 非 skill 的 SKILL.md 是普通 OKF concept，交给目录遍历统一处理。
                    *remaining = before_probe;
                }
                stack.push((path, depth + 1));
            } else if metadata.is_file() && path.extension().is_some_and(|x| x == "md") {
                let kind = legacy_kind_hint(root, &path);
                if let Some(text) = read_regular_utf8_within(&path, &canonical_root, remaining) {
                    let mut e = parse_entry(scope, kind, &path, &text);
                    assign_identity(root, &path, &mut e);
                    if let Some(reserved) = e.reserved.as_deref() {
                        e.kind = Kind::Generic;
                        e.concept_type = reserved.to_string();
                        e.slug = e.concept_id.clone();
                    }
                    // skill 无 description 不可被清单/调用发现，按规范跳过
                    if e.reserved.is_none() && e.kind == Kind::Skill && e.description.is_empty() {
                        continue;
                    }
                    out.push(e);
                }
            }
        }
    }
}

pub(super) fn read_regular_utf8_within(path: &Path, canonical_root: &Path, remaining: &mut usize) -> Option<String> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return None;
    }
    let canonical_path = path.canonicalize().ok()?;
    if !canonical_path.starts_with(canonical_root) {
        return None;
    }
    let declared_len = usize::try_from(metadata.len()).ok()?;
    if declared_len > MAX_FILE_BYTES || declared_len > *remaining {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let actual_len = text.len();
    if actual_len > MAX_FILE_BYTES || actual_len > *remaining {
        return None;
    }
    *remaining -= actual_len;
    Some(text)
}

fn assign_identity(root: &Path, path: &Path, entry: &mut Entry) {
    let Ok(relative) = path.strip_prefix(root) else { return };
    let id_path = if entry.kind == Kind::Skill && path.file_name().is_some_and(|name| name == "SKILL.md") {
        relative.parent().unwrap_or(relative).to_path_buf()
    } else {
        relative.with_extension("")
    };
    entry.concept_id = id_path.to_string_lossy().replace('\\', "/");
}

/// 仅给缺 `type` 的旧文件提供兼容 hint。未知目录没有任何默认执行语义。
fn legacy_kind_hint(root: &Path, path: &Path) -> Kind {
    let Some(first) = path.strip_prefix(root).ok().and_then(|rel| rel.components().next()).and_then(|c| c.as_os_str().to_str()) else {
        return Kind::Generic;
    };
    // 复数目录名优先按单数解析（rules->rule），失败再按原名（history 这类以 s 结尾的）
    Kind::from_str(first.trim_end_matches('s')).or_else(|| Kind::from_str(first)).unwrap_or(Kind::Generic)
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

    #[test]
    fn management_scan_retains_both_scopes_for_same_slug() {
        let dir = fixture("all-scopes");
        let home = dir.join("fake-home");
        let personal_rules = home.join(".agents/rules");
        std::fs::create_dir_all(&personal_rules).unwrap();
        std::fs::write(personal_rules.join("style.md"), "---\ndescription: 个人版\n---\n个人内容\n").unwrap();
        let entries = scan_all_with_home(&dir, &home);
        let styles: Vec<&Entry> = entries.iter().filter(|e| e.kind == Kind::Rule && e.slug == "style").collect();
        assert_eq!(styles.len(), 2, "管理视图不得隐藏被 project 覆盖的 personal 条目");
        assert!(styles.iter().any(|e| e.scope == Scope::Project));
        assert!(styles.iter().any(|e| e.scope == Scope::Personal));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn runtime_namespace_is_excluded_from_project_and_personal_knowledge() {
        let dir = fixture("runtime-excluded");
        let home = dir.join("fake-home");
        let project_runtime = crate::core::paths::KxenPaths::project(&dir).root();
        let personal_runtime = crate::core::paths::KxenPaths::global_in(&home).root();
        std::fs::create_dir_all(&project_runtime).unwrap();
        std::fs::create_dir_all(&personal_runtime).unwrap();
        std::fs::write(project_runtime.join("project-secret.md"), "must not scan").unwrap();
        std::fs::write(personal_runtime.join("personal-secret.md"), "must not scan").unwrap();

        let entries = scan_all_with_home(&dir, &home);

        assert!(!entries.iter().any(|entry| entry.path.contains("project-secret.md")));
        assert!(!entries.iter().any(|entry| entry.path.contains("personal-secret.md")));
        assert!(entries.iter().any(|entry| entry.slug == "style"), "normal .agents knowledge must remain visible");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn index_md_recognized_per_directory_layer() {
        let dir = fixture("index");
        std::fs::write(dir.join(".agents/index.md"), "---\ndescription: 总入口\n---\n先看这里。\n").unwrap();
        std::fs::write(dir.join(".agents/rules/index.md"), "---\ndescription: rules 层入口\n---\n规则地图。\n").unwrap();
        let home = dir.join("fake-home");
        let entries = scan_with_home(&dir, &home);
        let idx: Vec<&Entry> = entries.iter().filter(|e| e.path.ends_with("index.md")).collect();
        assert_eq!(idx.len(), 2, "多层 index.md 不得被同 slug first-wins 去重: {idx:?}");
        assert!(idx.iter().any(|e| e.slug == "index"));
        assert!(idx.iter().any(|e| e.slug == "rules/index"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn explicit_type_overrides_arbitrary_directory_and_unknown_stays_generic() {
        let dir = fixture("type-authority");
        let workflows = dir.join(".agents/workflows");
        std::fs::create_dir_all(&workflows).unwrap();
        std::fs::write(workflows.join("check.md"), "---\ntype: rule\ndescription: 任意目录规则\n---\n必须验证。\n").unwrap();
        std::fs::write(
            dir.join(".agents/rules/refactor.md"),
            "---\ntype: refactor\ndescription: 重构知识\ntags: [rust, code]\n---\n先保持行为。\n",
        )
        .unwrap();
        let skill = dir.join(".agents/automation/review");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "---\ntype: skill\ndescription: 审查流程\n---\n审查 $1。\n").unwrap();
        std::fs::write(skill.join("resource.md"), "---\ntype: test\n---\nresource").unwrap();

        let entries = scan(&dir);
        let rule = entries.iter().find(|entry| entry.concept_id == "workflows/check").unwrap();
        assert_eq!(rule.kind, Kind::Rule);
        let generic = entries.iter().find(|entry| entry.concept_id == "rules/refactor").unwrap();
        assert_eq!(generic.kind, Kind::Generic);
        assert_eq!(generic.concept_type, "refactor");
        assert!(generic.okf_conformant);
        assert!(entries.iter().any(|entry| entry.kind == Kind::Skill && entry.concept_id == "automation/review"));
        assert!(!entries.iter().any(|entry| entry.slug == "resource"), "Skill resources are not independent concepts");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_oversized_files_are_never_scanned() {
        use std::os::unix::fs::symlink;

        let dir = fixture("safe-files");
        let home = dir.join("fake-home");
        let outside = dir.join("outside-secret.txt");
        std::fs::write(&outside, "AWS_SECRET_ACCESS_KEY=secret").unwrap();
        symlink(&outside, dir.join(".agents/rules/cloud.md")).unwrap();
        symlink(&outside, dir.join("AGENTS.md")).unwrap();
        std::fs::write(dir.join(".agents/rules/huge.md"), vec![b'x'; MAX_FILE_BYTES + 1]).unwrap();

        let entries = scan_all_with_home(&dir, &home);
        assert!(!entries.iter().any(|entry| entry.slug == "cloud"));
        assert!(!entries.iter().any(|entry| entry.slug == "huge"));
        assert!(!entries.iter().any(|entry| entry.content.contains("AWS_SECRET_ACCESS_KEY")));
        std::fs::remove_dir_all(&dir).ok();
    }
}
