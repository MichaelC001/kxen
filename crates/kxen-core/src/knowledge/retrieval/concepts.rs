use super::{bm25_scores, fuse, normalize, retrieval_query, tokenize};
use crate::knowledge::Entry;
use std::collections::HashMap;

const TOP_K: usize = 12;
const LINK_BOOST: f64 = 0.15;

/// Generic/reference/history concept 只做发现，不获得 Rule/Skill/Command 的执行语义。
pub fn select_concepts<'a>(concepts: &[&'a Entry], task_query: Option<&str>, involved_rel: &[String]) -> Vec<&'a Entry> {
    select_concepts_with_runtime(concepts, task_query, involved_rel, None)
}

pub(crate) fn select_concepts_with_runtime<'a>(
    concepts: &[&'a Entry],
    task_query: Option<&str>,
    involved_rel: &[String],
    runtime: Option<&crate::knowledge::embedding::EmbeddingRuntime>,
) -> Vec<&'a Entry> {
    if concepts.is_empty() {
        return Vec::new();
    }
    let query = retrieval_query(task_query, involved_rel);
    if query.is_empty() {
        let mut catalog = concepts.to_vec();
        catalog.sort_by(|left, right| left.concept_id.cmp(&right.concept_id));
        catalog.truncate(TOP_K);
        return catalog;
    }

    let docs: Vec<String> = concepts.iter().map(|entry| concept_document(entry, None)).collect();
    let tokenized: Vec<Vec<String>> = docs.iter().map(|doc| tokenize(doc)).collect();
    let lexical = normalize(&bm25_scores(&tokenize(&query), &tokenized));
    let mut semantic = crate::knowledge::embedding::recall_lazy(&query, runtime, || {
        concepts.iter().map(|entry| concept_document(entry, Some(1500))).collect()
    });
    normalize_semantic(&mut semantic);
    let today = crate::knowledge::today();
    let mut scores: Vec<f64> = concepts
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let mut score = fuse(lexical[index], semantic.as_ref().and_then(|values| values[index]), entry.scope, &entry.date, &today);
            if entry.status.as_deref().is_some_and(|status| matches!(status.to_ascii_lowercase().as_str(), "deprecated" | "archived")) {
                score *= 0.25;
            }
            score
        })
        .collect();

    expand_links(concepts, &mut scores);
    let mut ranked: Vec<(f64, &Entry)> = scores.into_iter().zip(concepts.iter().copied()).filter(|(score, _)| *score > 0.0).collect();
    ranked.sort_by(|left, right| {
        right.0.partial_cmp(&left.0).unwrap_or(std::cmp::Ordering::Equal).then_with(|| left.1.concept_id.cmp(&right.1.concept_id))
    });
    ranked.truncate(TOP_K);
    ranked.into_iter().map(|(_, entry)| entry).collect()
}

fn expand_links(concepts: &[&Entry], scores: &mut [f64]) {
    let by_id: HashMap<&str, usize> = concepts.iter().enumerate().map(|(index, entry)| (entry.concept_id.as_str(), index)).collect();
    let direct = scores.to_vec();
    for (source_index, entry) in concepts.iter().enumerate() {
        if direct[source_index] <= 0.0 {
            continue;
        }
        for link in &entry.links {
            if let Some(target) = linked_concept_id(&entry.concept_id, link).and_then(|id| by_id.get(id.as_str()).copied()) {
                scores[target] += direct[source_index] * LINK_BOOST;
            }
        }
    }
}

fn concept_document(entry: &Entry, content_cap: Option<usize>) -> String {
    let content = content_cap.map(|cap| entry.content.chars().take(cap).collect::<String>()).unwrap_or_else(|| entry.content.clone());
    format!("{}\n{}\n{}\n{}\n{}\n{}", entry.concept_type, entry.concept_id, entry.title, entry.description, entry.tags.join(" "), content)
}

fn normalize_semantic(scores: &mut Option<Vec<Option<f64>>>) {
    let Some(scores) = scores else { return };
    let high = scores.iter().flatten().copied().fold(0.0f64, f64::max);
    scores.iter_mut().flatten().for_each(|score| *score = if high > 0.0 { score.max(0.0) / high } else { 0.0 });
}

fn linked_concept_id(source_id: &str, link: &str) -> Option<String> {
    use std::path::Component;
    let source_parent = std::path::Path::new(source_id).parent().unwrap_or_else(|| std::path::Path::new(""));
    let mut parts: Vec<String> = Vec::new();
    for component in source_parent.join(link).components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    let last = parts.last_mut()?;
    if let Some(stripped) = last.strip_suffix(".md") {
        *last = stripped.to_string();
    }
    Some(parts.join("/"))
}
