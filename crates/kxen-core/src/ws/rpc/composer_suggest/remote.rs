use super::{cap, session_history};
use crate::AppState;
use crate::composer_suggest::{LocalCandidate, Suggestion};
use crate::llm::{Message, ModelRef};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const REMOTE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const TEXT_CAP: usize = 8 * 1024;

pub(super) async fn remote(params: &Value, state: &Arc<AppState>) -> Result<Value, String> {
    let session_id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
    let request_id = params.get("request_id").and_then(Value::as_str).ok_or("missing request_id")?;
    let _lifecycle = crate::core::session_lifecycle::admit_mutation(&crate::core::paths::KxenPaths::user().sessions_dir(), session_id)?;
    if crate::core::shared::lock(&state.active_runs).contains_key(session_id) {
        return Err("Composer remote suggestions are disabled while the session is running".into());
    }
    let cancel = begin_request(session_id, request_id, state);
    let result = remote_inner(params, state, session_id, &cancel).await;
    finish_request(session_id, request_id, state);
    result
}

pub(super) fn cancel(params: &Value, state: &AppState) -> Result<Value, String> {
    let session_id = params.get("session_id").and_then(Value::as_str).ok_or("missing session_id")?;
    let request_id = params.get("request_id").and_then(Value::as_str);
    let mut requests = crate::core::shared::lock(&state.composer_suggestion_requests);
    let matches = requests.get(session_id).is_some_and(|(current, _)| request_id.is_none_or(|requested| requested == current));
    if matches && let Some((_, token)) = requests.remove(session_id) {
        token.cancel();
        return Ok(json!({ "cancelled": true }));
    }
    Ok(json!({ "cancelled": false }))
}

async fn remote_inner(
    params: &Value,
    state: &Arc<AppState>,
    session_id: &str,
    cancel: &crate::agent::cancel::CancelToken,
) -> Result<Value, String> {
    let config = crate::core::config_cache::cached_user_config_result()?;
    if !config.composer_suggestions.enabled || (!config.composer_suggestions.semantic && !config.composer_suggestions.llm) {
        return Ok(json!({ "suggestions": [], "warnings": [] }));
    }
    let runtime = state.runtime_for_session(session_id)?;
    let root = runtime.root();
    let draft = cap(params.get("draft").and_then(Value::as_str).unwrap_or(""), TEXT_CAP);
    let (history, _) = session_history(session_id)?;
    let selected_paths = string_array(params, "selected_paths");
    let candidate_ids = string_array(params, "candidate_ids");
    let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(6).clamp(1, 8) as usize;
    let trusted = crate::core::trust::is_trusted(root);
    let index_root = root.to_path_buf();
    let indexed = tokio::task::spawn_blocking(move || crate::composer_suggest::workspace_candidates(&index_root, trusted))
        .await
        .map_err(|error| format!("composer index task: {error}"))?;
    let candidates = shortlist(indexed, &candidate_ids);
    if candidates.is_empty() && !config.composer_suggestions.llm {
        return Ok(json!({ "suggestions": [], "warnings": [] }));
    }
    let store = crate::core::shared::lock(&state.auth_store).clone();
    let goal_id = crate::core::goal::Goal::focus_for_checked(&crate::core::paths::KxenPaths::user().goals_dir(), Some(session_id))
        .map_err(|error| error.to_string())?
        .map(|goal| goal.id);
    let reporter = crate::agent::agent_loop::UsageReporter::new(session_id.to_string(), state.session_tokens.clone(), state.bus.clone());
    let mut suggestions = Vec::new();
    let mut warnings = Vec::new();
    if config.composer_suggestions.semantic && !candidates.is_empty() {
        match semantic(
            root,
            &config.embedding,
            &store,
            &runtime.mrm(),
            &reporter,
            goal_id.as_deref(),
            &draft,
            &history,
            &selected_paths,
            &candidates,
            cancel,
            state,
            session_id,
            limit,
        )
        .await
        {
            Ok(ranked) => suggestions = ranked,
            Err(error) if !crate::llm::managed::is_cancelled_error(&error) && error != "embedding request cancelled" => {
                warnings.push(error)
            }
            Err(_) => return Err(crate::llm::managed::CANCELLED_ERROR.into()),
        }
    }
    if config.composer_suggestions.llm {
        match llm_suggestions(
            &runtime.mrm(),
            &store,
            &reporter,
            goal_id.as_deref(),
            &draft,
            &history,
            &selected_paths,
            &candidates,
            cancel,
            state,
            session_id,
        )
        .await
        {
            Ok(llm) => suggestions = merge(llm, suggestions, limit),
            Err(error) if !crate::llm::managed::is_cancelled_error(&error) => warnings.push(error),
            Err(_) => return Err(crate::llm::managed::CANCELLED_ERROR.into()),
        }
    }
    Ok(json!({ "suggestions": suggestions, "warnings": warnings }))
}

#[allow(clippy::too_many_arguments)]
async fn semantic(
    root: &std::path::Path,
    config: &crate::core::config::EmbeddingConfig,
    store: &crate::auth::credential::AuthStore,
    mrm: &Arc<crate::llm::mrm::ModelResourceManager>,
    reporter: &crate::agent::agent_loop::UsageReporter,
    goal_id: Option<&str>,
    draft: &str,
    history: &[String],
    selected: &[String],
    candidates: &[LocalCandidate],
    cancel: &crate::agent::cancel::CancelToken,
    state: &AppState,
    session_id: &str,
    limit: usize,
) -> Result<Vec<Suggestion>, String> {
    let endpoint = crate::knowledge::embedding::resolve_endpoint_with(config, store)
        .ok_or("Composer semantic suggest requires embedding config and credentials")?;
    let query = cap(&format!("{draft}\n{}\n{}", history.join("\n"), selected.join("\n")), TEXT_CAP);
    let docs: Vec<String> = candidates.iter().map(|candidate| cap(&format!("{}\n{}", candidate.path, candidate.summary), 1_200)).collect();
    let runtime = crate::knowledge::embedding::EmbeddingRuntime {
        endpoint: Some(Arc::new(endpoint.clone())),
        mrm: Arc::clone(mrm),
        cancel: Some(cancel.clone()),
        goal_id: goal_id.map(str::to_string),
        bus: Some(state.bus.clone()),
        session_id: Some(session_id.to_string()),
        usage_reporter: Some(reporter.clone()),
    };
    let scores = crate::composer_suggest::semantic_scores(root, &endpoint, &query, &docs, &runtime).await?;
    Ok(crate::composer_suggest::rank_semantic(candidates, &scores, limit))
}

#[allow(clippy::too_many_arguments)]
async fn llm_suggestions(
    mrm: &Arc<crate::llm::mrm::ModelResourceManager>,
    store: &crate::auth::credential::AuthStore,
    reporter: &crate::agent::agent_loop::UsageReporter,
    goal_id: Option<&str>,
    draft: &str,
    history: &[String],
    selected: &[String],
    candidates: &[LocalCandidate],
    cancel: &crate::agent::cancel::CancelToken,
    state: &AppState,
    session_id: &str,
) -> Result<Vec<Suggestion>, String> {
    let resolved = mrm.resolve("suggestion", store).await.ok_or("No available model for the suggestion role")?;
    let mut model = ModelRef::new(resolved.provider, resolved.model);
    model.account = resolved.account;
    let metadata: Vec<Value> =
        candidates.iter().map(|candidate| json!({ "id": format!("file:{}", candidate.path), "label": candidate.path })).collect();
    let messages = vec![
        Message::system(
            "Recommend up to 3 safe Composer actions from the supplied conversation metadata. Reply with ONLY a JSON array. Each item must be either {\"kind\":\"file\",\"candidate_id\":\"exact supplied id\",\"reason\":\"short reason\"} or {\"kind\":\"insert_text\",\"text\":\"short next user instruction\",\"reason\":\"short reason\"}. Never invent a file id. Do not send or execute anything.",
        ),
        Message::user(format!(
            "Draft:\n{}\n\nRecent session text:\n{}\n\nSelected attachment paths:\n{}\n\nLocal candidate metadata:\n{}",
            cap(draft, TEXT_CAP),
            cap(&history.join("\n"), TEXT_CAP),
            cap(&selected.join("\n"), 2_000),
            serde_json::to_string(&metadata).map_err(|error| error.to_string())?
        )),
    ];
    let text = managed_text(mrm, &model, &messages, store, reporter, goal_id, cancel, state, session_id).await?;
    Ok(crate::composer_suggest::parse_llm_suggestions(&text, candidates))
}

#[allow(clippy::too_many_arguments)]
async fn managed_text(
    mrm: &crate::llm::mrm::ModelResourceManager,
    model: &ModelRef,
    messages: &[Message],
    store: &crate::auth::credential::AuthStore,
    reporter: &crate::agent::agent_loop::UsageReporter,
    goal_id: Option<&str>,
    cancel: &crate::agent::cancel::CancelToken,
    state: &AppState,
    session_id: &str,
) -> Result<String, String> {
    let mut attempt = reporter.begin(goal_id)?;
    let output = crate::llm::managed::collect_text_observed_with_policy_and_start_limited(
        mrm,
        model,
        messages,
        store,
        REMOTE_TIMEOUT,
        None,
        Some(cancel),
        crate::llm::managed::CircuitPolicy::Record,
        Some(4 * 1024),
        Some(Box::new(|| reporter.mark_started(&mut attempt))),
    )
    .await;
    let (result, started, usage, warning) = match output {
        Ok(output) => (Ok(output.text), true, output.usage, output.metering_warning),
        Err(error) => (Err(error.message), error.request_started, error.usage, error.metering_warning),
    };
    if !started {
        reporter.discard_unstarted(&attempt)?;
    } else {
        reporter.mark_started(&mut attempt)?;
        if let Some(usage) = usage {
            reporter.observe(&mut attempt, usage.input, usage.output)?;
        }
        let settled = reporter.settle(&attempt)?;
        for warning in settled.durability_warnings {
            state.bus.publish(crate::core::event::Event::notify(format!("用量持久化已修复：{warning}"), Some(session_id.to_string())));
        }
        if let Some(message) = settled.stop_message {
            return Err(message);
        }
    }
    if let Some(warning) = warning {
        state.bus.publish(crate::core::event::Event::notify(warning, Some(session_id.to_string())));
    }
    result
}

fn shortlist(indexed: Vec<LocalCandidate>, ids: &[String]) -> Vec<LocalCandidate> {
    let mut by_id: HashMap<String, LocalCandidate> =
        indexed.into_iter().map(|candidate| (format!("file:{}", candidate.path), candidate)).collect();
    ids.iter().take(8).filter_map(|id| by_id.remove(id)).collect()
}

fn string_array(params: &Value, key: &str) -> Vec<String> {
    params.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect()
}

fn merge(primary: Vec<Suggestion>, secondary: Vec<Suggestion>, limit: usize) -> Vec<Suggestion> {
    let mut seen = HashSet::new();
    primary.into_iter().chain(secondary).filter(|item| seen.insert(item.id.clone())).take(limit).collect()
}

fn begin_request(session_id: &str, request_id: &str, state: &AppState) -> crate::agent::cancel::CancelToken {
    let token = crate::agent::cancel::CancelToken::new();
    if let Some((_, previous)) = crate::core::shared::lock(&state.composer_suggestion_requests)
        .insert(session_id.to_string(), (request_id.to_string(), token.clone()))
    {
        previous.cancel();
    }
    token
}

fn finish_request(session_id: &str, request_id: &str, state: &AppState) {
    let mut requests = crate::core::shared::lock(&state.composer_suggestion_requests);
    if requests.get(session_id).is_some_and(|(current, _)| current == request_id) {
        requests.remove(session_id);
    }
}
