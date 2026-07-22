//! 代理活动注册表：teammate / subagent / workflow 三类子代理的统一视图。
//! 每个 session 一份名单 + 每代理 200 条转录 ring buffer（内存态，不持久化）。

use crate::llm::ModelRef;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

const TRANSCRIPT_CAP: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Teammate,
    Subagent,
    Workflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStatus {
    Working,
    Idle,
    Done,
    Failed,
    Shutdown,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentActivity {
    pub name: String,
    pub kind: AgentKind,
    pub model: ModelRef,
    pub status: ActivityStatus,
    pub started_at: u64,
    #[serde(skip)]
    pub transcript: VecDeque<serde_json::Value>,
}

#[derive(Default)]
pub struct AgentRegistry {
    sessions: Mutex<HashMap<String, Vec<AgentActivity>>>,
}

impl AgentRegistry {
    pub fn register(&self, session_id: &str, name: &str, kind: AgentKind, model: &ModelRef) {
        let mut map = crate::core::shared::lock(&self.sessions);
        let list = map.entry(session_id.to_string()).or_default();
        if let Some(existing) = list.iter_mut().find(|a| a.name == name) {
            existing.status = ActivityStatus::Working;
            existing.kind = kind;
            return;
        }
        list.push(AgentActivity {
            name: name.to_string(),
            kind,
            model: model.clone(),
            status: ActivityStatus::Working,
            started_at: now_ms(),
            transcript: VecDeque::new(),
        });
    }

    pub fn set_status(&self, session_id: &str, name: &str, status: ActivityStatus) {
        let mut map = crate::core::shared::lock(&self.sessions);
        if let Some(agent) = map.get_mut(session_id).and_then(|list| list.iter_mut().find(|a| a.name == name)) {
            agent.status = status;
        }
    }

    /// 追加一条转录（事件 payload），超过 cap 淘汰最旧。
    pub fn push_transcript(&self, session_id: &str, name: &str, payload: serde_json::Value) {
        let mut map = crate::core::shared::lock(&self.sessions);
        if let Some(agent) = map.get_mut(session_id).and_then(|list| list.iter_mut().find(|a| a.name == name)) {
            if agent.transcript.len() >= TRANSCRIPT_CAP {
                agent.transcript.pop_front();
            }
            agent.transcript.push_back(payload);
        }
    }

    pub fn list(&self, session_id: &str) -> Vec<AgentActivity> {
        crate::core::shared::lock(&self.sessions).get(session_id).cloned().unwrap_or_default()
    }

    pub fn transcript(&self, session_id: &str, name: &str) -> Vec<serde_json::Value> {
        crate::core::shared::lock(&self.sessions)
            .get(session_id)
            .and_then(|list| list.iter().find(|a| a.name == name))
            .map(|a| a.transcript.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 生成唯一代理名（role-序号）。
    pub fn unique_name(&self, session_id: &str, prefix: &str) -> String {
        let map = crate::core::shared::lock(&self.sessions);
        let list = map.get(session_id);
        for i in 1..1000 {
            let candidate = format!("{prefix}-{i}");
            if !list.is_some_and(|l| l.iter().any(|a| a.name == candidate)) {
                return candidate;
            }
        }
        format!("{prefix}-{}", now_ms() % 10_000)
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_and_transcript_cap() {
        let reg = AgentRegistry::default();
        let model = ModelRef::new("xai", "grok");
        reg.register("s1", "alpha", AgentKind::Subagent, &model);
        reg.set_status("s1", "alpha", ActivityStatus::Done);
        let list = reg.list("s1");
        assert_eq!(list.len(), 1);
        assert!(matches!(list[0].status, ActivityStatus::Done));

        for i in 0..250 {
            reg.push_transcript("s1", "alpha", serde_json::json!({ "i": i }));
        }
        let t = reg.transcript("s1", "alpha");
        assert_eq!(t.len(), TRANSCRIPT_CAP);
        assert_eq!(t[0]["i"], 50, "最旧 50 条应被淘汰");

        let name = reg.unique_name("s1", "review");
        assert_eq!(name, "review-1");
        reg.register("s1", &name, AgentKind::Subagent, &model);
        assert_eq!(reg.unique_name("s1", "review"), "review-2");
    }
}
