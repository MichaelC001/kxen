//! OKF frontmatter 解析。YAML 必须完整解析，`type` 决定 concept 语义；未知 type 宽松消费。
//! 旧文件缺少 `type` 时只用目录 hint 保持可用，并明确标记为 non-conformant。

use super::{Entry, Kind, Scope};
use serde_yaml_ng::{Mapping, Value};
use std::path::Path;
use std::sync::LazyLock;

pub(super) fn parse_entry(scope: Scope, kind_hint: Kind, path: &Path, text: &str) -> Entry {
    let (metadata, content, has_frontmatter) = split_frontmatter(text);
    let parsed = metadata.and_then(|yaml| serde_yaml_ng::from_str::<Mapping>(yaml).ok());
    let explicit_type = parsed.as_ref().and_then(|map| strict_string(map, &["type"])).filter(|value| !value.trim().is_empty());
    let legacy_kind = parsed.as_ref().and_then(|map| string(map, &["kind"])).and_then(|value| Kind::from_str(&value));

    let note_type = parsed.as_ref().and_then(|map| string(map, &["note-type", "note_type"]));
    let (kind, concept_type) = match explicit_type.as_deref() {
        Some(value) => (Kind::from_str(value).unwrap_or(Kind::Generic), value.trim().to_string()),
        None if has_frontmatter && parsed.is_none() => (Kind::Generic, "invalid".to_string()),
        None => {
            let fallback = legacy_kind.unwrap_or(kind_hint);
            (fallback, fallback_type(fallback).to_string())
        }
    };

    let map = parsed.as_ref();
    let mut slug = map.and_then(|m| string(m, &["name"])).unwrap_or_default();
    if slug.is_empty() {
        slug = path.file_stem().map(|value| value.to_string_lossy().into_owned()).unwrap_or_default();
        if slug == "SKILL" {
            slug =
                path.parent().and_then(|parent| parent.file_name()).map(|value| value.to_string_lossy().into_owned()).unwrap_or_default();
        }
    }
    slug = slug.chars().take(64).collect();

    let title = map.and_then(|m| string(m, &["title"])).unwrap_or_default().chars().take(256).collect::<String>();
    let mut description = map.and_then(|m| string(m, &["description"])).unwrap_or_default().chars().take(1024).collect::<String>();
    if description.is_empty() && has_frontmatter && parsed.is_none() {
        description = "invalid YAML frontmatter".to_string();
    } else if description.is_empty() && kind != Kind::Skill {
        description = content
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(|line| line.trim_start_matches('#').trim().chars().take(80).collect())
            .unwrap_or_default();
    }
    let reserved = match path.file_name().and_then(|name| name.to_str()) {
        Some("index.md") => Some("index".to_string()),
        Some("log.md") => Some("log".to_string()),
        _ => None,
    };
    let is_reserved = reserved.is_some();

    Entry {
        scope,
        concept_type,
        kind,
        concept_id: slug.clone(),
        slug,
        title,
        description,
        content: content.trim().to_string(),
        path: path.to_string_lossy().into_owned(),
        resource: map.and_then(|m| string(m, &["resource"])),
        tags: map.map(|m| list(m, &["tags"])).unwrap_or_default(),
        status: map.and_then(|m| string(m, &["status"])),
        stale_after: map.and_then(|m| string(m, &["stale_after", "stale-after"])),
        links: markdown_links(content),
        okf_conformant: is_reserved || (has_frontmatter && parsed.is_some() && explicit_type.is_some()),
        reserved,
        okf_version: map.and_then(|m| string(m, &["okf_version", "okf-version"])),
        enabled: map.and_then(|m| boolean(m, &["enabled"])).unwrap_or(true),
        always_apply: map.and_then(|m| boolean(m, &["alwaysApply", "always_apply", "always"])).unwrap_or(false),
        globs: map.map(|m| list(m, &["globs", "glob"])).unwrap_or_default(),
        needs: map.map(|m| list(m, &["needs"])).unwrap_or_default(),
        when_to_use: map.and_then(|m| string(m, &["when_to_use", "when-to-use"])),
        arguments: map.map(|m| list(m, &["arguments"])).unwrap_or_default(),
        disable_model_invocation: map.and_then(|m| boolean(m, &["disable-model-invocation", "disable_model_invocation"])).unwrap_or(false),
        user_invocable: map.and_then(|m| boolean(m, &["user-invocable", "user_invocable"])).unwrap_or(true),
        argument_hint: map.and_then(|m| string(m, &["argument-hint", "argument_hint"])),
        note_type,
        date: map.and_then(|m| string(m, &["date"])).unwrap_or_default(),
        dir: if path.file_name().is_some_and(|name| name == "SKILL.md") {
            path.parent().map(|parent| parent.to_string_lossy().into_owned()).unwrap_or_default()
        } else {
            String::new()
        },
        is_agents_md: false,
    }
}

fn fallback_type(kind: Kind) -> &'static str {
    match kind {
        Kind::Rule => "rule",
        Kind::Reference => "reference",
        Kind::Skill => "skill",
        Kind::Command => "command",
        Kind::Note => "note",
        Kind::Memory => "memory",
        Kind::History => "history",
        Kind::Generic => "concept",
    }
}

fn split_frontmatter(text: &str) -> (Option<&str>, &str, bool) {
    let mut lines = text.split_inclusive('\n');
    let Some(first) = lines.next() else { return (None, text, false) };
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return (None, text, false);
    }
    let start = first.len();
    let mut offset = start;
    for line in lines {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return (Some(&text[start..offset]), &text[offset + line.len()..], true);
        }
        offset += line.len();
    }
    (None, text, false)
}

fn value<'a>(map: &'a Mapping, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| map.get(&Value::String((*key).to_string())))
}

fn string(map: &Mapping, keys: &[&str]) -> Option<String> {
    match value(map, keys)? {
        Value::String(value) => Some(value.trim().to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn strict_string(map: &Mapping, keys: &[&str]) -> Option<String> {
    match value(map, keys)? {
        Value::String(value) => Some(value.trim().to_string()),
        _ => None,
    }
}

fn boolean(map: &Mapping, keys: &[&str]) -> Option<bool> {
    match value(map, keys)? {
        Value::Bool(value) => Some(*value),
        Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn list(map: &Mapping, keys: &[&str]) -> Vec<String> {
    match value(map, keys) {
        Some(Value::Sequence(values)) => values
            .iter()
            .filter_map(|item| match item {
                Value::String(value) => Some(value.trim().to_string()),
                Value::Number(value) => Some(value.to_string()),
                _ => None,
            })
            .filter(|item| !item.is_empty())
            .collect(),
        Some(Value::String(value)) => value.split(',').map(str::trim).filter(|item| !item.is_empty()).map(String::from).collect(),
        _ => Vec::new(),
    }
}

fn markdown_links(content: &str) -> Vec<String> {
    static LINKS: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\[[^\]]*\]\(([^)\s]+)\)").expect("valid link regex"));
    let mut links: Vec<String> = LINKS
        .captures_iter(content)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str()))
        .map(|value| value.split('#').next().unwrap_or(value).trim())
        .filter(|value| {
            !value.is_empty()
                && !value.starts_with("https://")
                && !value.starts_with("http://")
                && !value.starts_with("mailto:")
                && !value.starts_with("data:")
        })
        .map(String::from)
        .collect();
    links.sort();
    links.dedup();
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_is_authoritative_and_unknown_is_generic() {
        let entry = parse_entry(
            Scope::Project,
            Kind::Rule,
            Path::new("/w/.agents/workflows/refactor.md"),
            "---\ntype: refactor\ntitle: Safe refactor\ndescription: |\n  Refactor with\n  verification\ntags:\n  - rust\n  - code\nenabled: true\n---\nSee [tests](../tests/unit.md).",
        );
        assert_eq!(entry.kind, Kind::Generic);
        assert_eq!(entry.concept_type, "refactor");
        assert_eq!(entry.tags, ["rust", "code"]);
        assert_eq!(entry.links, ["../tests/unit.md"]);
        assert!(entry.okf_conformant);
    }

    #[test]
    fn legacy_missing_type_uses_hint_but_is_non_conformant() {
        let entry = parse_entry(Scope::Project, Kind::Rule, Path::new("rules/a.md"), "---\nalwaysApply: true\n---\nbody");
        assert_eq!(entry.kind, Kind::Rule);
        assert_eq!(entry.concept_type, "rule");
        assert!(!entry.okf_conformant);
        assert!(entry.always_apply);
    }

    #[test]
    fn note_subtype_is_separate_and_unknown_type_stays_generic() {
        let current = parse_entry(
            Scope::Personal,
            Kind::Generic,
            Path::new("notes/a.md"),
            "---\ntype: note\nnote-type: correction\ndescription: use trash\n---\nnever rm",
        );
        assert_eq!(current.kind, Kind::Note);
        assert_eq!(current.note_type.as_deref(), Some("correction"));
        assert!(current.okf_conformant);

        let generic = parse_entry(Scope::Personal, Kind::Note, Path::new("notes/b.md"), "---\ntype: pitfall\n---\nbody");
        assert_eq!(generic.kind, Kind::Generic);
        assert_eq!(generic.concept_type, "pitfall");
        assert_eq!(generic.note_type, None);
    }

    #[test]
    fn malformed_yaml_keeps_body_and_does_not_claim_conformance() {
        let entry = parse_entry(Scope::Project, Kind::Reference, Path::new("x.md"), "---\ntype: [bad\n---\nbody");
        assert_eq!(entry.content, "body");
        assert!(!entry.okf_conformant);
    }
}
