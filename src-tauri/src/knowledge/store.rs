//! 写回路：notes/ 写入（同 slug 覆盖）、启停、双 scope 晋升、回收站删除、.kxen 私址存量迁移。

use super::{scan, scope_root, slugify, today, Entry, Kind, Scope, NOTE_TYPES};
use std::path::{Path, PathBuf};

/// 写入或更新一条 note（同 slug = 同题，整体覆盖不追加）。返回文件路径。
pub fn add(
    scope: Scope,
    workdir: &Path,
    slug: Option<&str>,
    note_type: &str,
    description: &str,
    content: &str,
) -> Result<String, String> {
    let note_type = if NOTE_TYPES.contains(&note_type) { note_type } else { "note" };
    let description = description.trim();
    if description.is_empty() {
        return Err("missing description".into());
    }
    let slug = slugify(slug.unwrap_or(description));
    let dir = scope_root(scope, workdir).join(Kind::Note.dir_name());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let body = format!(
        "---\nnote-type: {note_type}\ndescription: {description}\ndate: {}\n---\n\n{}\n",
        today(),
        content.trim()
    );
    let path = dir.join(format!("{slug}.md"));
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// 设置页与 knowledge 工具共用的全量列表（双 scope，scan 序 = 项目在前）。
pub fn list(workdir: &Path) -> Vec<Entry> {
    scan(workdir)
}

fn find_entry(scope: Scope, workdir: &Path, slug: &str) -> Result<Entry, String> {
    let slug = slugify(slug);
    scan(workdir)
        .into_iter()
        .find(|e| e.scope == scope && e.slug == slug)
        .ok_or_else(|| format!("not found: {}/{slug}", scope.as_str()))
}

/// 删除一条（进系统废纸篓可恢复；目录型 skill 整目录移走）。
pub fn remove(scope: Scope, workdir: &Path, slug: &str) -> Result<(), String> {
    let e = find_entry(scope, workdir, slug)?;
    let target = if !e.dir.is_empty() { PathBuf::from(&e.dir) } else { PathBuf::from(&e.path) };
    trash::delete(&target).map_err(|e| e.to_string())
}

/// 启停开关：frontmatter 加/去 enabled:false（注入跳过但不删除）。
pub fn set_enabled(scope: Scope, workdir: &Path, slug: &str, enabled: bool) -> Result<(), String> {
    let e = find_entry(scope, workdir, slug)?;
    let text = std::fs::read_to_string(&e.path).map_err(|err| err.to_string())?;
    let mut out = String::new();
    let mut seen = false;
    let mut in_fm = false;
    for (i, line) in text.lines().enumerate() {
        if i == 0 && line == "---" {
            in_fm = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_fm && line == "---" {
            if !seen && !enabled {
                out.push_str("enabled: false\n");
            }
            in_fm = false;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_fm && line.starts_with("enabled:") {
            seen = true;
            if !enabled {
                out.push_str("enabled: false\n");
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    std::fs::write(&e.path, out).map_err(|err| err.to_string())
}

/// 跨 scope 晋升（personal -> project 唯一方向有意义，反向也允许）：保 kind 目录落位。
pub fn move_entry(scope: Scope, workdir: &Path, slug: &str, to: Scope) -> Result<String, String> {
    if scope == to {
        return Err("scope 相同".into());
    }
    let e = find_entry(scope, workdir, slug)?;
    let dir = scope_root(to, workdir).join(e.kind.dir_name());
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let dest = dir.join(format!("{}.md", e.slug));
    std::fs::rename(&e.path, &dest).map_err(|err| err.to_string())?;
    Ok(dest.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kxen-kn-store-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn same_slug_updates_not_duplicates() {
        let dir = ws("dedup");
        add(Scope::Project, &dir, None, "correction", "use trash not rm", "v1").unwrap();
        add(Scope::Project, &dir, None, "correction", "use trash not rm", "v2").unwrap();
        let entries: Vec<Entry> = list(&dir)
            .into_iter()
            .filter(|e| e.scope == Scope::Project && e.kind == Kind::Note)
            .collect();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].content.contains("v2"));
        assert_eq!(entries[0].note_type.as_deref(), Some("correction"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remove_goes_to_trash() {
        let dir = ws("remove");
        let path = add(Scope::Project, &dir, None, "note", "temp note", "x").unwrap();
        remove(Scope::Project, &dir, "temp-note").unwrap();
        assert!(!Path::new(&path).exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
