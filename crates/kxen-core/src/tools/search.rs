//! glob / grep 搜索工具：ignore 遍历（尊重 .gitignore）+ globset 匹配 + regex 内容搜索。

use globset::{Glob, GlobSetBuilder};
use ignore::WalkBuilder;
use std::path::Path;

const MAX_GLOB_RESULTS: usize = 200;
const MAX_GREP_RESULTS: usize = 100;
const MAX_NAME_RESULTS: usize = 64;
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

/// 搜索结果：hits 可能被 cap 截断，total 始终是完整匹配数（agent 据此判断漏了多少）。
#[derive(Debug, serde::Serialize)]
pub struct SearchHits {
    pub hits: Vec<String>,
    pub total: usize,
}

impl SearchHits {
    pub fn truncated(&self) -> bool {
        self.total > self.hits.len()
    }
}

/// glob：返回匹配文件的相对路径（按修改时间倒序，截断到 MAX_GLOB_RESULTS）。
/// 为统计完整 total 必须走完整棵树，不再提前 break。
pub fn glob_files(pattern: &str, base: &Path) -> Result<SearchHits, SearchError> {
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
            let mtime = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            hits.push((rel, mtime));
        }
    }
    let total = hits.len();
    hits.sort_by_key(|h| std::cmp::Reverse(h.1));
    hits.truncate(MAX_GLOB_RESULTS);
    Ok(SearchHits { hits: hits.into_iter().map(|(rel, _)| rel).collect(), total })
}

/// grep：regex 搜索文件内容，返回 `path:line: content`（可 glob 过滤，截断到 MAX_GREP_RESULTS）。
/// 达到 cap 后继续遍历只计数不收集：total 必须完整，agent 才知道漏了多少。
pub fn grep_files(pattern: &str, base: &Path, glob_filter: Option<&str>) -> Result<SearchHits, SearchError> {
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
    let mut total = 0usize;
    for entry in WalkBuilder::new(base).hidden(false).build().flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap_or(path).to_string_lossy().into_owned();
        if let Some(f) = &filter
            && !f.is_match(&rel)
        {
            continue;
        }
        if entry.metadata().map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else { continue };
        for (i, line) in content.lines().enumerate() {
            if re.is_match(line) {
                total += 1;
                if out.len() < MAX_GREP_RESULTS {
                    out.push(format!("{rel}:{}: {}", i + 1, line.chars().take(200).collect::<String>()));
                }
            }
        }
    }
    Ok(SearchHits { hits: out, total })
}

#[derive(Debug, serde::Serialize)]
pub struct CompleteEntry {
    pub path: String,
    pub kind: &'static str, // "file" | "dir"
}

#[derive(Debug, serde::Serialize)]
pub struct NameMatch {
    pub path: String,
    pub size: u64,
}

/// fs.resolve_name 后端：按文件名（basename 精确匹配）全量遍历 workspace，返回 {path, size}。
/// 浏览器 File 只暴露 basename + size，真实相对路径靠这里反查（同名文件前端按 size 消歧）。
pub fn find_by_name(name: &str, base: &Path) -> Vec<NameMatch> {
    if name.is_empty() || !base.is_dir() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for entry in WalkBuilder::new(base).hidden(false).build().flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        if path.file_name().and_then(|n| n.to_str()) != Some(name) {
            continue;
        }
        let rel = path.strip_prefix(base).unwrap_or(path).to_string_lossy().into_owned();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        hits.push(NameMatch { path: rel, size });
        if hits.len() >= MAX_NAME_RESULTS {
            break;
        }
    }
    hits
}

/// @ 补全：子序列模糊匹配（大小写不敏感），按匹配质量 + 路径长度排序。
/// 评分：连续匹配段越长越好；起始匹配与路径越短越好。
pub fn complete(query: &str, base: &Path, limit: usize) -> Vec<CompleteEntry> {
    if !base.is_dir() {
        return Vec::new();
    }
    let query = query.to_lowercase();
    let mut hits: Vec<(i64, CompleteEntry)> = Vec::new();
    for entry in WalkBuilder::new(base).hidden(false).build().flatten() {
        let is_file = entry.file_type().is_some_and(|t| t.is_file());
        let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
        if !is_file && !is_dir {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap_or(path).to_string_lossy().into_owned();
        if rel.is_empty() {
            continue;
        }
        let score = if query.is_empty() { Some(0) } else { fuzzy_score(&query, &rel.to_lowercase()) };
        if let Some(score) = score {
            hits.push((score, CompleteEntry { path: rel, kind: if is_file { "file" } else { "dir" } }));
        }
        if hits.len() >= limit * 8 {
            break;
        }
    }
    // 分数高的在前；同分短路径在前
    hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.path.len().cmp(&b.1.path.len())));
    hits.truncate(limit);
    hits.into_iter().map(|(_, e)| e).collect()
}

/// 子序列匹配评分：None = 不匹配；分数 = 连续段奖励 - 间隙惩罚。
fn fuzzy_score(query: &str, candidate: &str) -> Option<i64> {
    let mut qs = query.chars().peekable();
    let mut score = 0i64;
    let mut run = 0i64;
    for c in candidate.chars() {
        if qs.peek() == Some(&c) {
            qs.next();
            run += 1;
            score += 2 + run * 2; // 连续命中递增奖励
        } else {
            run = 0;
            score -= 1; // 间隙惩罚
        }
    }
    if qs.next().is_none() { Some(score) } else { None }
}

#[cfg(test)]
mod complete_tests {
    use super::*;

    #[test]
    fn fuzzy_ranks_contiguous_higher() {
        let dir = std::env::temp_dir().join(format!("kxen-complete-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src/agent")).unwrap();
        std::fs::write(dir.join("src/agent/agent_loop.rs"), "").unwrap();
        std::fs::write(dir.join("src/agent/okf.rs"), "").unwrap();
        std::fs::write(dir.join("README.md"), "").unwrap();

        let hits = complete("agent", &dir, 10);
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|h| h.path.contains("agent_loop.rs")));
        assert_eq!(hits[0].kind, "dir", "目录精确命中应排最前");

        let none = complete("zzzqqq", &dir, 10);
        assert!(none.is_empty());

        let all = complete("", &dir, 10);
        assert_eq!(all.len(), 5, "src/ + src/agent/ + 2 文件 + README");
        std::fs::remove_dir_all(&dir).ok();
    }
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
        assert_eq!(hits.hits.len(), 2);
        assert_eq!(hits.total, 2);
        assert!(!hits.truncated());
        assert!(hits.hits.iter().any(|h| h.ends_with("main.rs")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn glob_reports_full_total_when_capped() {
        let dir = std::env::temp_dir().join(format!("kxen-search-globcap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..205 {
            std::fs::write(dir.join(format!("f{i:03}.rs")), "").unwrap();
        }
        let hits = glob_files("*.rs", &dir).unwrap();
        assert_eq!(hits.hits.len(), MAX_GLOB_RESULTS);
        assert_eq!(hits.total, 205, "cap 后 total 仍是完整匹配数");
        assert!(hits.truncated());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn grep_with_filter() {
        let dir = fixture("grep");
        let hits = grep_files("hello", &dir, Some("*.md")).unwrap();
        assert_eq!(hits.hits.len(), 1);
        assert_eq!(hits.total, 1);
        assert!(hits.hits[0].contains("README.md:2"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn grep_reports_full_total_when_capped() {
        let dir = std::env::temp_dir().join(format!("kxen-search-grepcap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let body: String = (0..150).map(|i| format!("hit line {i}\n")).collect();
        std::fs::write(dir.join("big.txt"), body).unwrap();
        let hits = grep_files("hit", &dir, None).unwrap();
        assert_eq!(hits.hits.len(), MAX_GREP_RESULTS);
        assert_eq!(hits.total, 150, "cap 后仍继续计数");
        assert!(hits.truncated());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bad_regex_is_error() {
        let dir = fixture("badre");
        assert!(matches!(grep_files("(unclosed", &dir, None), Err(SearchError::Regex(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn find_by_name_matches_basename() {
        let dir = fixture("name");
        let hits = find_by_name("main.rs", &dir);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "src/main.rs");
        assert!(hits[0].size > 0);
        // 同名多命中全部返回（size 消歧留给前端）
        std::fs::write(dir.join("src/nested/main.rs"), "fn dup() {}\n").unwrap();
        assert_eq!(find_by_name("main.rs", &dir).len(), 2);
        assert!(find_by_name("nope.xyz", &dir).is_empty());
        assert!(find_by_name("", &dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
