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
    /// 子代理独立取消句柄 (session_id, name)：subagent/workflow 派发时挂载，agents.stop 按名停单个
    ///（teammate 不走这里，它的 token 在 TeamState.cancels，由 team shutdown 通道取消）。
    cancels: Mutex<HashMap<(String, String), crate::agent::cancel::CancelToken>>,
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

    /// 前缀定名注册（subagent/workflow 派发口）：「查重名 -> 生成唯一名 -> 插入」同一把锁内完成，
    /// 返回定名。拆成 unique_name + register 两次取锁时，真并发下同 role 两个派发拿到同名，
    /// register 去重把它们并成一条、两路转录交错写同一 agent。
    pub fn register_unique(&self, session_id: &str, prefix: &str, kind: AgentKind, model: &ModelRef) -> String {
        let mut map = crate::core::shared::lock(&self.sessions);
        let list = map.entry(session_id.to_string()).or_default();
        let name = (1..1000)
            .map(|i| format!("{prefix}-{i}"))
            .find(|candidate| !list.iter().any(|a| &a.name == candidate))
            .unwrap_or_else(|| format!("{prefix}-{}", now_ms() % 10_000));
        list.push(AgentActivity {
            name: name.clone(),
            kind,
            model: model.clone(),
            status: ActivityStatus::Working,
            started_at: now_ms(),
            transcript: VecDeque::new(),
        });
        name
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

    /// 登记子代理取消句柄：dispatch 定名后立即挂，agents.stop 才能停到运行早期的实例。
    pub fn register_cancel(&self, session_id: &str, name: &str, token: crate::agent::cancel::CancelToken) {
        crate::core::shared::lock(&self.cancels).insert((session_id.to_string(), name.to_string()), token);
    }

    /// 按名取消子代理；无句柄（未注册或 teammate）返回 false。
    pub fn cancel(&self, session_id: &str, name: &str) -> bool {
        let token = crate::core::shared::lock(&self.cancels).get(&(session_id.to_string(), name.to_string())).cloned();
        token.is_some_and(|t| {
            t.cancel();
            true
        })
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
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
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

        let name = reg.register_unique("s1", "review", AgentKind::Subagent, &model);
        assert_eq!(name, "review-1");
        assert_eq!(reg.register_unique("s1", "review", AgentKind::Subagent, &model), "review-2");
        assert_eq!(reg.list("s1").len(), 3);
    }

    #[test]
    fn cancel_by_name_only_with_registered_handle() {
        let reg = AgentRegistry::default();
        let token = crate::agent::cancel::CancelToken::new();
        reg.register_cancel("s1", "review-1", token.clone());
        assert!(!reg.cancel("s1", "ghost"), "未注册的 name 必须返回 false");
        assert!(!reg.cancel("s2", "review-1"), "跨 session 同名不得命中");
        assert!(reg.cancel("s1", "review-1"));
        assert!(token.is_cancelled(), "cancel 必须触发令牌");
    }

    #[test]
    fn concurrent_register_same_prefix_gets_distinct_names() {
        // 真并发下同 role 两次派发：拆锁实现会同名并条，单锁 register_unique 必须各自定名
        let reg = std::sync::Arc::new(AgentRegistry::default());
        let model = ModelRef::new("xai", "grok");
        let mut handles = Vec::new();
        for _ in 0..8 {
            let reg = reg.clone();
            let model = model.clone();
            handles.push(std::thread::spawn(move || reg.register_unique("s1", "review", AgentKind::Subagent, &model)));
        }
        let mut names: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 8, "并发注册不得重名: {names:?}");
        assert_eq!(reg.list("s1").len(), 8, "全部代理都在列表中");
    }
}
