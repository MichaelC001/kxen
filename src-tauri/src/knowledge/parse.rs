//! 统一 frontmatter 超集解析：一份规范吃全 kind，未知字段不致命。
//! 历史兼容：`type` 按值分流——值是 kind 名则当 kind，值是 note 子类型（correction 等）则 kind=Note。

use super::{Entry, Kind, NOTE_TYPES, Scope};
use std::path::Path;

pub(super) fn parse_entry(scope: Scope, kind_hint: Kind, path: &Path, text: &str) -> Entry {
    let mut kind = kind_hint;
    let mut slug = String::new();
    let mut description = String::new();
    let mut always_apply = false;
    let mut globs: Vec<String> = Vec::new();
    let mut enabled = true;
    let mut needs: Vec<String> = Vec::new();
    let mut when_to_use = None;
    let mut arguments: Vec<String> = Vec::new();
    let mut disable_model_invocation = false;
    let mut user_invocable = true;
    let mut argument_hint = None;
    let mut note_type = None;
    let mut date = String::new();
    let mut content = text;

    if let Some(rest) = text.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                let Some((key, value)) = line.split_once(':') else { continue };
                let key = key.trim();
                let value = value.trim().trim_matches('"');
                match key {
                    "kind" | "type" => {
                        if let Some(k) = Kind::from_str(value) {
                            kind = k;
                        } else if NOTE_TYPES.contains(&value) {
                            kind = if kind_hint == Kind::Memory { Kind::Memory } else { Kind::Note };
                            note_type = Some(value.to_string());
                        }
                    }
                    "name" => slug = value.chars().take(64).collect(),
                    "description" => description = value.chars().take(1024).collect(),
                    "alwaysApply" | "always_apply" | "always" => always_apply = matches!(value, "true" | "yes" | "1"),
                    "globs" | "glob" => globs = list_value(value),
                    "enabled" => enabled = value != "false",
                    "needs" => needs = list_value(value),
                    "when_to_use" | "when-to-use" => when_to_use = Some(value.to_string()),
                    "arguments" => arguments = list_value(value),
                    "disable-model-invocation" | "disable_model_invocation" => {
                        disable_model_invocation = matches!(value, "true" | "yes" | "1")
                    }
                    "user-invocable" | "user_invocable" => user_invocable = !matches!(value, "false" | "no" | "0"),
                    "argument-hint" | "argument_hint" => argument_hint = Some(value.to_string()),
                    "note-type" | "note_type" => note_type = Some(value.to_string()),
                    "date" => date = value.to_string(),
                    _ => {}
                }
            }
            content = rest[end + 4..].trim_start_matches('\n');
        }
    }

    // slug 兜底：文件名；SKILL.md 用父目录名
    if slug.is_empty() {
        slug = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if slug == "SKILL" {
            slug = path.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        }
    }
    // description 兜底：正文首个非空行（skill 除外——无显式 description 的 skill 按规范不可见）
    if description.is_empty() && kind != Kind::Skill {
        description = content
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim_start_matches('#').trim().chars().take(80).collect())
            .unwrap_or_default();
    }
    let dir = if path.file_name().is_some_and(|n| n == "SKILL.md") {
        path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
    } else {
        String::new()
    };

    Entry {
        scope,
        kind,
        slug,
        description,
        content: content.trim().to_string(),
        path: path.to_string_lossy().into_owned(),
        enabled,
        always_apply,
        globs,
        needs,
        when_to_use,
        arguments,
        disable_model_invocation,
        user_invocable,
        argument_hint,
        note_type,
        date,
        dir,
        is_agents_md: false,
    }
}

/// 逗号分隔或行内数组（`["a", "b"]`）两种写法都收。
fn list_value(value: &str) -> Vec<String> {
    value
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn legacy_type_splits_by_value() {
        let rule = parse_entry(
            Scope::Project,
            Kind::Reference,
            Path::new("/w/.agents/rules/a.md"),
            "---\ntype: rule\nalwaysApply: true\n---\nbody",
        );
        assert_eq!(rule.kind, Kind::Rule);
        assert!(rule.always_apply);

        let note = parse_entry(
            Scope::Personal,
            Kind::Note,
            Path::new("/h/.agents/notes/b.md"),
            "---\ntype: correction\ndescription: use trash\n---\nnever rm",
        );
        assert_eq!(note.kind, Kind::Note);
        assert_eq!(note.note_type.as_deref(), Some("correction"));
    }

    #[test]
    fn needs_and_lists_parse() {
        let e = parse_entry(
            Scope::Project,
            Kind::Skill,
            Path::new("/w/.agents/skills/review/SKILL.md"),
            "---\nname: review\ndescription: 审查\nneeds: [style-guide, \"rust-rules\"]\nglobs: *.rs, src/**\n---\n审查 $1",
        );
        assert_eq!(e.slug, "review");
        assert_eq!(e.needs, vec!["style-guide", "rust-rules"]);
        assert_eq!(e.globs, vec!["*.rs", "src/**"]);
        assert!(!e.dir.is_empty());
    }

    #[test]
    fn unknown_fields_not_fatal() {
        let e = parse_entry(Scope::Project, Kind::Reference, PathBuf::from("x.md").as_path(), "---\nweird: [a, b\n---\nbody");
        assert_eq!(e.content, "body");
    }
}
