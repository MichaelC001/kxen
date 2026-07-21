//! .agents/ OKF 项目知识目录：rules（alwaysApply 全文注入）+ references（索引渐进披露）。
//! 约定：workdir 根的 AGENTS.md 永远注入；.agents/**/*.md 按 frontmatter 分类。
//! frontmatter 手写宽松解析：未知 type/字段不致命，无 frontmatter 视为普通 doc。

use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct OkfDoc {
    pub path: PathBuf,
    pub doc_type: String,
    pub always_apply: bool,
    pub description: String,
    pub content: String,
}

/// 扫描 workdir：根 AGENTS.md + .agents/ 下全部 .md（含多层子目录，就近原则由路径顺序体现）。
pub fn scan(workdir: &Path) -> Vec<OkfDoc> {
    let mut docs = Vec::new();
    let root_agents = workdir.join("AGENTS.md");
    if let Ok(content) = std::fs::read_to_string(&root_agents) {
        docs.push(OkfDoc {
            path: root_agents,
            doc_type: "rule".into(),
            always_apply: true,
            description: "root AGENTS.md".into(),
            content,
        });
    }
    let agents_dir = workdir.join(".agents");
    if agents_dir.is_dir() {
        let mut stack = vec![agents_dir];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|x| x == "md") {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        docs.push(parse_doc(path, text));
                    }
                }
            }
        }
    }
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    docs
}

/// 宽松 frontmatter 解析：`---` 包围的 key: value 头；正文是剩余部分。
fn parse_doc(path: PathBuf, text: String) -> OkfDoc {
    let mut doc_type = "doc".to_string();
    let mut always_apply = false;
    let mut description = String::new();
    let mut content = text.as_str();

    if let Some(rest) = text.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let header = &rest[..end];
            for line in header.lines() {
                let Some((key, value)) = line.split_once(':') else { continue };
                let value = value.trim();
                match key.trim() {
                    "type" => doc_type = value.to_string(),
                    "alwaysApply" | "always_apply" | "always" => always_apply = matches!(value, "true" | "yes" | "1"),
                    "description" => description = value.to_string(),
                    _ => {}
                }
            }
            content = rest[end + 4..].trim_start_matches('\n');
        }
    }

    if description.is_empty() {
        // 退化：取正文第一个 heading 或首行
        description = content
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim_start_matches('#').trim().chars().take(80).collect())
            .unwrap_or_default();
    }

    OkfDoc { path, doc_type, always_apply, description, content: content.to_string() }
}

/// 渲染注入段：rules + alwaysApply 全文；其余一行索引（模型按需 read）。
/// 无 .agents / AGENTS.md 时返回 None（不产生空段）。
pub fn render_context(workdir: &Path) -> Option<String> {
    let docs = scan(workdir);
    if docs.is_empty() {
        return None;
    }
    let mut rules = String::new();
    let mut index = String::new();
    for doc in &docs {
        let rel = doc.path.strip_prefix(workdir).unwrap_or(&doc.path).to_string_lossy().into_owned();
        if doc.always_apply || doc.doc_type == "rule" {
            rules.push_str(&format!("\n#### {rel}\n{}\n", doc.content.trim()));
        } else {
            index.push_str(&format!("- {rel} — {}\n", doc.description));
        }
    }
    let mut out = String::from("\n\n## Project knowledge (.agents/ + AGENTS.md)\n");
    if !rules.is_empty() {
        out.push_str("\n### Rules (always applied)\n");
        out.push_str(&rules);
    }
    if !index.is_empty() {
        out.push_str("\n### Knowledge index (read these files on demand with the read tool)\n");
        out.push_str(&index);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frontmatter() {
        let dir = std::env::temp_dir().join(format!("kxen-okf-{}", std::process::id()));
        let agents = dir.join(".agents/rules");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(
            agents.join("style.md"),
            "---\ntype: rule\nalwaysApply: true\ndescription: code style\n---\nUse tabs, single quotes.\n",
        )
        .unwrap();
        std::fs::write(agents.join("arch.md"), "---\ntype: reference\n---\n# Architecture\nDetails here.\n").unwrap();

        let docs = scan(&dir);
        assert_eq!(docs.len(), 2);
        let rendered = render_context(&dir).unwrap();
        assert!(rendered.contains("Use tabs, single quotes."));
        assert!(rendered.contains("arch.md"));
        assert!(!rendered.contains("Details here."), "reference 只进索引不进全文");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn no_agents_dir_returns_none() {
        let dir = std::env::temp_dir().join(format!("kxen-okf-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(render_context(&dir).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_fields_are_not_fatal() {
        let doc = parse_doc(PathBuf::from("x.md"), "---\ntype: rule\nweird: [a, b\n---\nbody".into());
        assert_eq!(doc.doc_type, "rule");
        assert_eq!(doc.content, "body");
    }
}
