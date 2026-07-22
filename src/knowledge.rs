//! 知识沉淀写回路：三 scope 存储（project 克制 / global 跨项目 / memory 本机）+ frontmatter + 同 slug 覆盖更新。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub scope: String,
    pub slug: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
    pub date: String,
    pub content: String,
    pub path: String,
}

const KINDS: &[&str] = &["correction", "convention", "pitfall", "preference", "note"];

pub fn scope_dir(scope: &str, workdir: &Path) -> Result<PathBuf, String> {
    match scope {
        "project" => Ok(workdir.join(".agents/rules")),
        "global" => Ok(dirs::home_dir().ok_or("no home dir")?.join(".agents/rules")),
        "memory" => Ok(workdir.join(".kxen/memory")),
        other => Err(format!("unknown scope: {other} (project|global|memory)")),
    }
}

pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut dash = true; // 开头不补 dash
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

fn today() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!("{:04}-{:02}-{:02}", now.year(), now.month() as u8, now.day())
}

/// 写入或更新一条知识（同 slug = 同题，整体覆盖不追加）。
pub fn add(scope: &str, workdir: &Path, slug: Option<&str>, kind: &str, description: &str, content: &str) -> Result<String, String> {
    let kind = if KINDS.contains(&kind) { kind } else { "note" };
    let description = description.trim();
    if description.is_empty() {
        return Err("missing description".into());
    }
    let slug = slugify(slug.unwrap_or(description));
    let dir = scope_dir(scope, workdir)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let body = format!("---\ntype: {kind}\ndescription: {description}\ndate: {}\n---\n\n{}\n", today(), content.trim());
    let path = dir.join(format!("{slug}.md"));
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

fn parse_file(scope: &str, path: &Path) -> Option<KnowledgeEntry> {
    let text = std::fs::read_to_string(path).ok()?;
    let slug = path.file_stem()?.to_string_lossy().into_owned();
    let mut kind = "note".to_string();
    let mut description = String::new();
    let mut date = String::new();
    let mut content = text.as_str();
    if let Some(rest) = text.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            for line in rest[..end].lines() {
                if let Some((k, v)) = line.split_once(':') {
                    match k.trim() {
                        "type" => kind = v.trim().to_string(),
                        "description" => description = v.trim().to_string(),
                        "date" => date = v.trim().to_string(),
                        _ => {}
                    }
                }
            }
            content = rest[end + 5..].trim();
        }
    }
    if description.is_empty() {
        description = content.lines().next().unwrap_or("").chars().take(60).collect();
    }
    Some(KnowledgeEntry { scope: scope.into(), slug, kind, description, date, content: content.into(), path: path.to_string_lossy().into_owned() })
}

/// 全 scope 列出（设置页审计 + 注入渲染共用）。
pub fn list(workdir: &Path) -> Vec<KnowledgeEntry> {
    let mut out = Vec::new();
    for scope in ["project", "global", "memory"] {
        let Ok(dir) = scope_dir(scope, workdir) else { continue };
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                if let Some(e) = parse_file(scope, &path) {
                    out.push(e);
                }
            }
        }
    }
    out.sort_by(|a, b| b.date.cmp(&a.date));
    out
}

/// 删除一条（走回收站，与 delete 工具同纪律）。
pub fn remove(scope: &str, workdir: &Path, slug: &str) -> Result<(), String> {
    let path = scope_dir(scope, workdir)?.join(format!("{}.md", slugify(slug)));
    if !path.exists() {
        return Err(format!("not found: {scope}/{slug}"));
    }
    let status = std::process::Command::new("/usr/bin/trash").arg(&path).status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("trash failed: {status}"));
    }
    Ok(())
}

/// 注入渲染：global + memory 全文（project 已由 OKF 扫描覆盖）。单篇截 500 字符。
pub fn render_extra(workdir: &Path) -> Option<String> {
    let all = list(workdir);
    let entries: Vec<&KnowledgeEntry> = all.iter().filter(|e| e.scope != "project").collect();
    if entries.is_empty() {
        return None;
    }
    let mut out = String::from("\n\n## Global & local knowledge (~/.agents/rules + .kxen/memory)\n");
    for e in entries {
        let body: String = e.content.chars().take(500).collect();
        out.push_str(&format!("\n### [{}] {} ({})\n{}\n", e.kind, e.description, e.scope, body));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kxen-kn-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn same_slug_updates_not_duplicates() {
        let dir = ws("dedup");
        add("project", &dir, None, "correction", "use trash not rm", "v1").unwrap();
        add("project", &dir, None, "correction", "use trash not rm", "v2").unwrap();
        let entries = list(&dir);
        assert_eq!(entries.iter().filter(|e| e.scope == "project").count(), 1);
        assert!(entries[0].content.contains("v2"));
        assert_eq!(entries[0].kind, "correction");
        assert!(!entries[0].date.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slugify_rules() {
        assert_eq!(slugify("Use Trash, Not RM!"), "use-trash-not-rm");
        assert_eq!(slugify("---"), "note");
    }

    #[test]
    fn remove_goes_to_trash() {
        let dir = ws("remove");
        let path = add("memory", &dir, None, "note", "temp note", "x").unwrap();
        remove("memory", &dir, "temp-note").unwrap();
        assert!(!Path::new(&path).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
