use crate::AppState;
use crate::composer_suggest::{LocalSignals, LocalSuggestInput};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

#[path = "composer_suggest/remote.rs"]
mod remote_suggest;

const TEXT_CAP: usize = 8 * 1024;
const DIFF_CAP: usize = 8 * 1024;

pub(super) async fn local(params: &Value, state: &Arc<AppState>) -> Result<Value, String> {
    if !crate::core::config_cache::cached_user_config_result()?.composer_suggestions.enabled {
        return Ok(json!({ "suggestions": [], "trusted": false }));
    }
    let draft = cap(params.get("draft").and_then(Value::as_str).unwrap_or(""), TEXT_CAP);
    let session_id = params.get("session_id").and_then(Value::as_str).unwrap_or("");
    let selected_paths = params
        .get("selected_paths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(6).clamp(1, 8) as usize;
    let runtime = if session_id.is_empty() { state.active_runtime()? } else { state.runtime_for_session(session_id)? };
    let root = runtime.root().to_path_buf();
    let trusted = crate::core::trust::is_trusted(&root);
    let (history, context_paths) = session_history(session_id)?;
    let involved_paths = involved_paths(session_id, &root, state);
    let (changed_paths, diffs) = if trusted { git_context(&root).await } else { (Vec::new(), HashMap::new()) };
    let index_root = root.clone();
    let mut candidates = tokio::task::spawn_blocking(move || crate::composer_suggest::workspace_candidates(&index_root, trusted))
        .await
        .map_err(|error| format!("composer index task: {error}"))?;
    crate::composer_suggest::merge_diff_summaries(&mut candidates, &diffs);
    let now_unix = std::time::SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| duration.as_secs());
    let suggestions = crate::composer_suggest::rank_local(
        &LocalSuggestInput {
            draft,
            history,
            selected_paths,
            signals: LocalSignals { changed_paths, involved_paths, context_paths },
            now_unix,
            limit,
        },
        candidates,
    );
    Ok(json!({ "suggestions": suggestions, "trusted": trusted }))
}

pub(super) async fn remote(params: &Value, state: &Arc<AppState>) -> Result<Value, String> {
    remote_suggest::remote(params, state).await
}

pub(super) fn cancel(params: &Value, state: &AppState) -> Result<Value, String> {
    remote_suggest::cancel(params, state)
}

fn session_history(session_id: &str) -> Result<(Vec<String>, Vec<String>), String> {
    if session_id.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let messages = crate::core::session::load_messages_checked(&crate::core::paths::KxenPaths::user().sessions_dir(), session_id)
        .map_err(|error| format!("session {session_id}: {error}"))?;
    Ok(crate::composer_suggest::recent_session_context(&messages))
}

fn involved_paths(session_id: &str, root: &Path, state: &AppState) -> Vec<String> {
    if session_id.is_empty() {
        return Vec::new();
    }
    crate::core::shared::lock(&state.session_involved)
        .get(session_id)
        .into_iter()
        .flatten()
        .map(|path| path.strip_prefix(root).unwrap_or(path).to_string_lossy().into_owned())
        .collect()
}

async fn git_context(root: &Path) -> (Vec<String>, HashMap<String, String>) {
    let Ok(status) = crate::tools::worktree::status(root).await else { return (Vec::new(), HashMap::new()) };
    let changed: Vec<String> =
        status.into_iter().map(|entry| entry.path).filter(|path| !crate::composer_suggest::is_sensitive_path(path)).collect();
    let mut diffs = HashMap::new();
    let mut remaining = DIFF_CAP;
    for path in changed.iter().take(12) {
        if remaining == 0 {
            break;
        }
        if let Ok(diff) = crate::tools::worktree::diff_file(root, path).await {
            let diff = cap(&diff, remaining);
            remaining = remaining.saturating_sub(diff.len());
            diffs.insert(path.clone(), diff);
        }
    }
    (changed, diffs)
}

fn cap(text: &str, max_bytes: usize) -> String {
    let end = text.char_indices().map(|(index, _)| index).take_while(|index| *index <= max_bytes).last().unwrap_or(0);
    if text.len() <= max_bytes { text.to_string() } else { text[..end].to_string() }
}
