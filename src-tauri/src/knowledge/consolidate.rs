//! 后台记忆 consolidation：周期整理（30min 轮，宿主 cron loop 驱动）。
//! 近 24h 活跃会话尾部蒸馏进 notes（同 slug 自然去重），按会话记录水位避免重复蒸馏；静默失败。

use crate::llm::ModelRef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const WINDOW_MS: u64 = 24 * 3600 * 1000;

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    /// session_id -> 上次蒸馏到的 updated_at 水位
    distilled: HashMap<String, u64>,
}

fn state_file() -> std::path::PathBuf {
    crate::core::paths::data_dir().join("consolidate.json")
}

fn load_state() -> State {
    std::fs::read_to_string(state_file())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// 水位推进：仅蒸馏成功（Ok）才写新水位，Err 留旧水位、下轮自动重试同批消息；
/// Ok(0)（成功零沉淀）同样推进——否则同会话每轮白跑一次 LLM。
fn advance_watermark(state: &mut State, session_id: &str, result: &Result<usize, String>, updated_at: u64) {
    if result.is_ok() {
        state.distilled.insert(session_id.to_string(), updated_at);
    }
}

/// 一轮整理：返回蒸馏写入条数（任何单会话失败跳过，不阻断后续）。
pub async fn run_once(model: &ModelRef, store: &crate::auth::credential::AuthStore) -> usize {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let since = now.saturating_sub(WINDOW_MS);
    let mut state = load_state();
    let mut written = 0;
    for meta in crate::core::session::list(&crate::core::paths::sessions_dir()) {
        if meta.updated_at < since {
            continue;
        }
        let water = state.distilled.get(&meta.id).copied().unwrap_or(0);
        if meta.updated_at <= water {
            continue;
        }
        let transcript: Vec<String> = crate::core::session::load_messages(&crate::core::paths::sessions_dir(), &meta.id)
            .into_iter()
            .rev()
            .take(20)
            .rev()
            .map(|m| {
                m.parts
                    .iter()
                    .filter_map(|p| match p {
                        crate::core::session::Part::Text { text } | crate::core::session::Part::Context { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|t| !t.is_empty())
            .collect();
        if transcript.len() < 2 {
            continue;
        }
        let workdir = std::path::PathBuf::from(&meta.directory);
        let result = crate::knowledge::distill::distill_on_delete(model, store, &workdir, transcript).await;
        advance_watermark(&mut state, &meta.id, &result, meta.updated_at);
        written += result.unwrap_or(0);
    }
    let _ = std::fs::write(state_file(), serde_json::to_string_pretty(&state).unwrap_or_default());
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_keeps_watermark_then_retry_advances() {
        let mut state = State::default();
        state.distilled.insert("s1".into(), 100);
        advance_watermark(&mut state, "s1", &Err("boom".into()), 200);
        assert_eq!(state.distilled.get("s1"), Some(&100), "失败留旧水位");
        advance_watermark(&mut state, "s1", &Ok(2), 200);
        assert_eq!(state.distilled.get("s1"), Some(&200), "重试成功后推进");
    }

    #[test]
    fn success_zero_notes_still_advances() {
        let mut state = State::default();
        advance_watermark(&mut state, "s1", &Ok(0), 300);
        assert_eq!(state.distilled.get("s1"), Some(&300), "零沉淀也推进，防同会话每轮白跑 LLM");
    }
}
