//! Composer 上下文候选的本地排序核心。
//! 默认路径不访问网络，只融合完整输入、近期 Session 文本和 workspace 本地信号。

use crate::knowledge::retrieval::{bm25_scores, normalize, tokenize};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

#[path = "composer_suggest/remote.rs"]
mod remote;
pub use remote::{parse_llm_suggestions, rank_semantic};
#[path = "composer_suggest/semantic.rs"]
mod semantic;
pub use semantic::semantic_scores;

const INDEX_TTL: Duration = Duration::from_secs(10);
const MAX_INDEX_FILES: usize = 2_000;
const MAX_SCAN_FILES: usize = 10_000;
const MAX_CONTENT_BYTES: u64 = 256 * 1024;
const SUMMARY_BYTES: u64 = 4 * 1024;
const HISTORY_TEXT_CAP: usize = 8 * 1024;
const HISTORY_MESSAGES: usize = 4;

#[derive(Clone)]
struct CachedIndex {
    created: Instant,
    trusted: bool,
    candidates: Vec<LocalCandidate>,
}

static INDEXES: OnceLock<Mutex<HashMap<std::path::PathBuf, CachedIndex>>> = OnceLock::new();

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalSignals {
    #[serde(default)]
    pub changed_paths: Vec<String>,
    #[serde(default)]
    pub involved_paths: Vec<String>,
    #[serde(default)]
    pub context_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSuggestInput {
    pub draft: String,
    pub history: Vec<String>,
    pub selected_paths: Vec<String>,
    pub signals: LocalSignals,
    pub now_unix: u64,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalCandidate {
    pub path: String,
    pub summary: String,
    pub modified_unix: u64,
    pub sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub label: String,
    pub reason: String,
    pub source: String,
    pub score: f64,
}

pub fn rank_local(input: &LocalSuggestInput, candidates: Vec<LocalCandidate>) -> Vec<Suggestion> {
    let selected: HashSet<&str> = input.selected_paths.iter().map(String::as_str).collect();
    let candidates: Vec<LocalCandidate> =
        candidates.into_iter().filter(|candidate| !candidate.sensitive && !selected.contains(candidate.path.as_str())).collect();
    if input.limit == 0 || candidates.is_empty() {
        return Vec::new();
    }

    let mut query = input.draft.clone();
    for item in &input.history {
        query.push(' ');
        query.push_str(item);
    }
    let mut related_query_terms: Vec<String> = input
        .selected_paths
        .iter()
        .chain(&input.signals.changed_paths)
        .chain(&input.signals.involved_paths)
        .chain(&input.signals.context_paths)
        .flat_map(|path| tokenize(path))
        .filter(|term| informative_path_term(term))
        .collect();
    related_query_terms.sort();
    related_query_terms.dedup();
    for term in &related_query_terms {
        query.push(' ');
        query.push_str(term);
    }
    let query_terms = tokenize(&query);
    let docs: Vec<Vec<String>> =
        candidates.iter().map(|candidate| tokenize(&format!("{} {}", candidate.path, candidate.summary))).collect();
    let lexical = normalize(&bm25_scores(&query_terms, &docs));
    let draft_terms: HashSet<String> = tokenize(&input.draft).into_iter().collect();
    let history_terms: HashSet<String> = input.history.iter().flat_map(|item| tokenize(item)).collect();
    let related_terms: HashSet<String> = related_query_terms.into_iter().collect();
    let changed: HashSet<&str> = input.signals.changed_paths.iter().map(String::as_str).collect();
    let involved: HashSet<&str> = input.signals.involved_paths.iter().map(String::as_str).collect();
    let context: HashSet<&str> = input.signals.context_paths.iter().map(String::as_str).collect();

    let mut ranked: Vec<Suggestion> = candidates
        .into_iter()
        .zip(docs)
        .zip(lexical)
        .filter_map(|((candidate, terms), lexical)| {
            let git = changed.contains(candidate.path.as_str());
            let session = involved.contains(candidate.path.as_str());
            let prior_context = context.contains(candidate.path.as_str());
            let signal = if git { 0.28 } else { 0.0 } + if session { 0.24 } else { 0.0 } + if prior_context { 0.2 } else { 0.0 };
            let recency = recency_boost(input.now_unix, candidate.modified_unix);
            if lexical <= 0.0 && signal <= 0.0 && recency <= 0.0 {
                return None;
            }
            let term_set: HashSet<&str> = terms.iter().map(String::as_str).collect();
            let draft_match = draft_terms.iter().any(|term| term_set.contains(term.as_str()));
            let history_match = history_terms.iter().any(|term| term_set.contains(term.as_str()));
            let related_match = related_terms.iter().any(|term| term_set.contains(term.as_str()));
            let reason = reason(git, session, prior_context, draft_match, history_match, related_match);
            Some(Suggestion {
                id: format!("file:{}", candidate.path),
                kind: "file".into(),
                label: candidate.path.rsplit('/').next().unwrap_or(&candidate.path).to_string(),
                path: candidate.path,
                reason,
                source: "local".into(),
                score: lexical * 0.72 + signal + recency,
            })
        })
        .collect();
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    ranked.truncate(input.limit);
    ranked
}

pub fn recent_session_context(messages: &[crate::core::session::Message]) -> (Vec<String>, Vec<String>) {
    use crate::agent::context::ContextItem;
    use crate::core::session::{Part, Role};
    let mut history = Vec::new();
    let mut context_paths = Vec::new();
    let mut remaining = HISTORY_TEXT_CAP;
    for message in messages.iter().rev() {
        for part in &message.parts {
            if let Part::ContextSources { items } = part {
                context_paths.extend(items.iter().filter_map(|item| match item {
                    ContextItem::File { path } | ContextItem::Dir { path } => Some(path.clone()),
                    ContextItem::Web { .. } | ContextItem::Docs { .. } | ContextItem::Note { .. } => None,
                }));
            }
        }
        if history.len() >= HISTORY_MESSAGES || remaining == 0 || !matches!(message.role, Role::User | Role::Assistant) {
            continue;
        }
        let text = message
            .parts
            .iter()
            .filter_map(|part| match part {
                Part::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !text.trim().is_empty() {
            let text = cap_text(&text, remaining);
            remaining = remaining.saturating_sub(text.len());
            history.push(text);
        }
    }
    history.reverse();
    context_paths.sort();
    context_paths.dedup();
    (history, context_paths)
}

/// Workspace-scoped 短时索引。WalkBuilder 尊重 gitignore 且不跟随 symlink；
/// 未信任 workspace 只读取路径和 mtime，不读取文件正文。
pub fn workspace_candidates(base: &Path, trusted: bool) -> Vec<LocalCandidate> {
    let root = std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    let indexes = INDEXES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = crate::core::shared::lock(indexes).get(&root)
        && cached.trusted == trusted
        && cached.created.elapsed() <= INDEX_TTL
    {
        return cached.candidates.clone();
    }
    let candidates = build_index(&root, trusted);
    crate::core::shared::lock(indexes).insert(root, CachedIndex { created: Instant::now(), trusted, candidates: candidates.clone() });
    candidates
}

pub fn merge_diff_summaries(candidates: &mut [LocalCandidate], diffs: &HashMap<String, String>) {
    for candidate in candidates {
        if let Some(diff) = diffs.get(&candidate.path) {
            candidate.summary.push(' ');
            candidate.summary.extend(diff.chars().take(2_000));
        }
    }
}

pub fn is_sensitive_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('/').collect();
    if crate::core::paths::KxenPaths::contains_project_state(&lower) {
        return true;
    }
    if parts.iter().any(|part| matches!(*part, ".git" | ".ssh" | ".aws" | ".gnupg" | "node_modules" | "target" | ".next" | "dist")) {
        return true;
    }
    let name = parts.last().copied().unwrap_or("");
    name == ".env"
        || name.starts_with(".env.")
        || matches!(name, ".npmrc" | ".pypirc" | ".netrc" | ".dockerconfigjson" | "id_rsa" | "id_ed25519" | "auth.json" | "credentials")
        || name.starts_with("credentials.")
        || name.starts_with("secrets.")
        || [".pem", ".key", ".p12", ".pfx", ".jks", ".keystore"].iter().any(|suffix| name.ends_with(suffix))
}

fn build_index(base: &Path, trusted: bool) -> Vec<LocalCandidate> {
    if !base.is_dir() {
        return Vec::new();
    }
    let mut files: Vec<(String, u64, u64)> = ignore::WalkBuilder::new(base)
        .hidden(false)
        .build()
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let rel = entry.path().strip_prefix(base).ok()?.to_string_lossy().into_owned();
            if is_sensitive_path(&rel) {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            let modified_unix =
                metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map_or(0, |duration| duration.as_secs());
            Some((rel, modified_unix, metadata.len()))
        })
        .take(MAX_SCAN_FILES)
        .collect();
    files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    files.truncate(MAX_INDEX_FILES);
    let empty = HashSet::new();
    files
        .into_iter()
        .map(|(path, modified_unix, size)| {
            let summary = if trusted && size <= MAX_CONTENT_BYTES {
                crate::tools::path_policy::resolve(&path, base, &empty)
                    .ok()
                    .and_then(|resolved| resolved.open().ok())
                    .and_then(read_summary)
                    .unwrap_or_default()
            } else {
                String::new()
            };
            LocalCandidate { path, summary, modified_unix, sensitive: false }
        })
        .collect()
}

fn read_summary(mut file: cap_std::fs::File) -> Option<String> {
    let mut bytes = Vec::new();
    file.by_ref().take(SUMMARY_BYTES).read_to_end(&mut bytes).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn cap_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let end = text.char_indices().map(|(index, _)| index).take_while(|index| *index <= max_bytes).last().unwrap_or(0);
    text[..end].to_string()
}

fn informative_path_term(term: &str) -> bool {
    term.chars().count() > 2 && !matches!(term, "src" | "lib" | "app" | "crate" | "crates" | "component" | "components" | "test" | "tests")
}

fn recency_boost(now: u64, modified: u64) -> f64 {
    const WEEK: u64 = 7 * 24 * 60 * 60;
    if modified == 0 || modified > now || now - modified > WEEK {
        return 0.0;
    }
    0.08 * (1.0 - (now - modified) as f64 / WEEK as f64)
}

fn reason(git: bool, session: bool, context: bool, draft: bool, history: bool, related: bool) -> String {
    if git {
        "Git 变更且与当前上下文相关".into()
    } else if session {
        "Session 最近涉及".into()
    } else if context {
        "Session 已用上下文".into()
    } else if draft && history {
        "匹配完整输入和 Session 历史".into()
    } else if draft {
        "匹配完整输入".into()
    } else if history {
        "匹配 Session 历史".into()
    } else if related {
        "与附件或近期上下文相关".into()
    } else {
        "Workspace 最近修改".into()
    }
}
