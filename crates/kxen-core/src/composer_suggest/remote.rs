use super::{LocalCandidate, Suggestion};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

pub fn rank_semantic(candidates: &[LocalCandidate], scores: &[Option<f64>], limit: usize) -> Vec<Suggestion> {
    let total = candidates.len().max(1) as f64;
    let mut ranked: Vec<Suggestion> = candidates
        .iter()
        .zip(scores)
        .enumerate()
        .filter_map(|(index, (candidate, semantic))| {
            let semantic = (*semantic)?.max(0.0);
            let local = 1.0 - index as f64 / total;
            Some(Suggestion {
                id: format!("file:{}", candidate.path),
                kind: "file".into(),
                path: candidate.path.clone(),
                label: candidate.path.rsplit('/').next().unwrap_or(&candidate.path).to_string(),
                reason: "Embedding 语义匹配".into(),
                source: "semantic".into(),
                score: semantic * 0.7 + local * 0.3,
            })
        })
        .collect();
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    ranked.truncate(limit);
    ranked
}

#[derive(Deserialize)]
struct RawSuggestion {
    kind: String,
    #[serde(default)]
    candidate_id: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    reason: String,
}

pub fn parse_llm_suggestions(text: &str, candidates: &[LocalCandidate]) -> Vec<Suggestion> {
    let Ok(raw) = serde_json::from_str::<Vec<RawSuggestion>>(text.trim()) else { return Vec::new() };
    let allowed: HashMap<String, &LocalCandidate> =
        candidates.iter().map(|candidate| (format!("file:{}", candidate.path), candidate)).collect();
    let mut seen = HashSet::new();
    raw.into_iter()
        .take(3)
        .filter_map(|item| match item.kind.as_str() {
            "file" => {
                let candidate = allowed.get(&item.candidate_id)?;
                if !seen.insert(item.candidate_id.clone()) {
                    return None;
                }
                Some(Suggestion {
                    id: item.candidate_id,
                    kind: "file".into(),
                    path: candidate.path.clone(),
                    label: candidate.path.rsplit('/').next().unwrap_or(&candidate.path).to_string(),
                    reason: normalize_text(&item.reason, 120).unwrap_or_else(|| "LLM 上下文推荐".into()),
                    source: "llm".into(),
                    score: 1.0,
                })
            }
            "insert_text" => {
                let text = normalize_text(&item.text, 200)?;
                let id = format!("text:{}", text_hash(&text));
                if !seen.insert(id.clone()) {
                    return None;
                }
                Some(Suggestion {
                    id,
                    kind: "insert_text".into(),
                    path: String::new(),
                    label: text,
                    reason: normalize_text(&item.reason, 120).unwrap_or_else(|| "LLM 建议的下一步".into()),
                    source: "llm".into(),
                    score: 1.0,
                })
            }
            _ => None,
        })
        .collect()
}

fn normalize_text(text: &str, limit: usize) -> Option<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(limit).collect())
}

fn text_hash(text: &str) -> String {
    use sha2::Digest;
    crate::core::shared::hex_lower(&sha2::Sha256::digest(text.as_bytes()))
}
