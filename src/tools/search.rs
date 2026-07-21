//! glob / grep 搜索工具：ignore 遍历（尊重 .gitignore）+ globset 匹配 + regex 内容搜索。

use globset::{Glob, GlobSetBuilder};
use ignore::WalkBuilder;
use std::path::Path;

const MAX_GLOB_RESULTS: usize = 200;
const MAX_GREP_RESULTS: usize = 100;
const MAX_FILE_BYTES: u64 = 512 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("invalid glob: {0}")]
    Glob(String),
    #[error("invalid regex: {0}")]
    Regex(String),
    #[error("path not found: {0}")]
    Path(String),
}

/// glob：返回匹配文件的相对路径（按修改时间倒序，截断）。
pub fn glob_files(pattern: &str, base: &Path) -> Result<Vec<String>, SearchError> {
    if !base.is_dir() {
        return Err(SearchError::Path(base.to_string_lossy().into_owned()));
    }
    let mut builder = GlobSetBuilder::new();
    builder.add(Glob::new(pattern).map_err(|e| SearchError::Glob(e.to_string()))?);
    let set = builder.build().map_err(|e| SearchError::Glob(e.to_string()))?;

    let mut hits: Vec<(String, u64)> = Vec::new();
    for entry in WalkBuilder::new(base).hidden(false).build().flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap_or(path).to_string_lossy().into_owned();
        if set.is_match(&rel) {
            let mtime = entry.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            hits.push((rel, mtime));
        }
        if hits.len() >= MAX_GLOB_RESULTS * 4 {
            break;
        }
    }
    hits.sort_by(|a, b| b.1.cmp(&a.1));
    hits.truncate(MAX_GLOB_RESULTS);
    Ok(hits.into_iter().map(|(rel, _)| rel).collect())
}

/// grep：regex 搜索文件内容，返回 `path:line: content`（可 glob 过滤，截断）。
pub fn grep_files(pattern: &str, base: &Path, glob_filter: Option<&str>) -> Result<Vec<String>, SearchError> {
    if !base.is_dir() {
        return Err(SearchError::Path(base.to_string_lossy().into_owned()));
    }
    let re = regex::Regex::new(pattern).map_err(|e| SearchError::Regex(e.to_string()))?;
    let filter = match glob_filter {
        Some(g) => {
            let mut builder = GlobSetBuilder::new();
            builder.add(Glob::new(g).map_err(|e| SearchError::Glob(e.to_string()))?);
            Some(builder.build().map_err(|e| SearchError::Glob(e.to_string()))?)
        }
        None => None,
    };

    let mut out = Vec::new();
    'walk: for entry in WalkBuilder::new(base).hidden(false).build().flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap_or(path).to_string_lossy().into_owned();
        if let Some(f) = &filter {
            if !f.is_match(&rel) {
                continue;
            }
        }
        if entry.metadata().map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else { continue };
        for (i, line) in content.lines().enumerate() {
            if re.is_match(line) {
                out.push(format!("{rel}:{}: {}", i + 1, line.chars().take(200).collect::<String>()));
                if out.len() >= MAX_GREP_RESULTS {
                    break 'walk;
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kxen-search-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src/nested")).unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main() { println!(\"hi\"); }\n").unwrap();
        std::fs::write(dir.join("src/nested/lib.rs"), "pub fn helper() {}\n").unwrap();
        std::fs::write(dir.join("README.md"), "# demo\nhello world\n").unwrap();
        dir
    }

    #[test]
    fn glob_matches_recursive() {
        let dir = fixture("glob");
        let hits = glob_files("**/*.rs", &dir).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.ends_with("main.rs")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn grep_with_filter() {
        let dir = fixture("grep");
        let hits = grep_files("hello", &dir, Some("*.md")).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].contains("README.md:2"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bad_regex_is_error() {
        let dir = fixture("badre");
        assert!(matches!(grep_files("(unclosed", &dir, None), Err(SearchError::Regex(_))));
        std::fs::remove_dir_all(&dir).ok();
    }
}
