//! 代理活动注册表：teammate / subagent / workflow 三类子代理的统一视图。

use crate::core::session::now_ms;
use crate::llm::ModelRef;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

pub(crate) const TRANSCRIPT_CAP: usize = 200;

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
    /// teammate 计划待 lead 批准（MemberStatus::AwaitingPlanApproval 透传）：
    /// 压成 Working 会让前端误显示「工作中」，看不出在等人批准
    AwaitingPlanApproval,
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
    /// teammate 转录写穿根目录（data_dir/teams，TeamManager 构造时注入）：内存 ring 重启即失，
    /// teammate 是常驻代理，transcript 由 <root>/<session>/transcripts/<name>.jsonl 兜底；None = 纯内存（测试默认）。
    team_root: Mutex<Option<std::path::PathBuf>>,
    /// subagent 转录/turn 历史根目录（sessions_dir，AppState 启动注入；布局见 activity_disk）。
    agents_root: Mutex<Option<std::path::PathBuf>>,
    /// 已从盘恢复过 subagent 条目的 session（惰性恢复只在首次访问时扫描一次）
    restored: Mutex<std::collections::HashSet<String>>,
}

impl AgentRegistry {
    pub fn register(&self, session_id: &str, name: &str, kind: AgentKind, model: &ModelRef) {
        let mut map = crate::core::shared::lock(&self.sessions);
        self.ensure_restored_locked(&mut map, session_id);
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
            transcript: self.rehydrate(session_id, name, kind),
        });
    }

    pub fn set_team_root(&self, root: std::path::PathBuf) {
        *crate::core::shared::lock(&self.team_root) = Some(root);
    }

    pub fn set_agents_root(&self, root: std::path::PathBuf) {
        *crate::core::shared::lock(&self.agents_root) = Some(root);
    }

    fn transcript_path(&self, session_id: &str, name: &str, kind: AgentKind) -> Option<std::path::PathBuf> {
        let team_root = crate::core::shared::lock(&self.team_root).clone();
        let agents_root = crate::core::shared::lock(&self.agents_root).clone();
        super::activity_disk::transcript_path(team_root.as_deref(), agents_root.as_deref(), kind, session_id, name)
    }

    /// subagent per-run turn 历史落点（dispatch 的 persist_turn 用）；None = 无持久化上下文。
    pub fn run_log_path(&self, session_id: &str, name: &str) -> Option<std::path::PathBuf> {
        let agents_root = crate::core::shared::lock(&self.agents_root).clone();
        super::activity_disk::run_log_path(agents_root.as_deref(), session_id, name)
    }

    /// 惰性恢复：进程重启后内存为空，首次访问该 session 时从盘重建 subagent 条目。
    /// 必须在 sessions 锁内调用（调用方均持锁）；恢复条目占位唯一名，
    /// 重启后的 register_unique 不会与落盘转录撞名（转录交错根因同 register_unique 注释）。
    fn ensure_restored_locked(&self, map: &mut HashMap<String, Vec<AgentActivity>>, session_id: &str) {
        let mut restored = crate::core::shared::lock(&self.restored);
        if !restored.insert(session_id.to_string()) {
            return;
        }
        drop(restored);
        let Some(root) = crate::core::shared::lock(&self.agents_root).clone() else { return };
        super::activity_disk::restore_into(map.entry(session_id.to_string()).or_default(), &root, session_id);
    }

    fn rehydrate(&self, session_id: &str, name: &str, kind: AgentKind) -> VecDeque<serde_json::Value> {
        if kind != AgentKind::Teammate {
            return VecDeque::new();
        }
        super::activity_disk::rehydrate(self.transcript_path(session_id, name, kind))
    }

    fn persist_line(&self, session_id: &str, name: &str, kind: AgentKind, payload: &serde_json::Value) {
        super::activity_disk::append_line(self.transcript_path(session_id, name, kind), payload);
    }

    /// 前缀定名注册（subagent/workflow 派发口）：「查重名 -> 生成唯一名 -> 插入」同一把锁内完成，
    /// 返回定名。拆成 unique_name + register 两次取锁时，真并发下同 role 两个派发拿到同名，
    /// register 去重把它们并成一条、两路转录交错写同一 agent。
    /// 重放与 register 同路：teammate 若经此口定名注册，磁盘转录照样注水（非 teammate 早退零开销）。
    pub fn register_unique(&self, session_id: &str, prefix: &str, kind: AgentKind, model: &ModelRef) -> String {
        let mut map = crate::core::shared::lock(&self.sessions);
        self.ensure_restored_locked(&mut map, session_id);
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
            transcript: self.rehydrate(session_id, &name, kind),
        });
        name
    }

    pub fn set_status(&self, session_id: &str, name: &str, status: ActivityStatus) {
        let mut map = crate::core::shared::lock(&self.sessions);
        if let Some(agent) = map.get_mut(session_id).and_then(|list| list.iter_mut().find(|a| a.name == name)) {
            agent.status = status;
        }
    }

    /// 追加一条转录（事件 payload），超过 cap 淘汰最旧；teammate/subagent 同步写穿落盘。
    /// 文件 append 在 sessions 锁内做：多线程推同一 (session, agent) 时行序不交错。
    pub fn push_transcript(&self, session_id: &str, name: &str, payload: serde_json::Value) {
        let mut map = crate::core::shared::lock(&self.sessions);
        if let Some(agent) = map.get_mut(session_id).and_then(|list| list.iter_mut().find(|a| a.name == name)) {
            self.persist_line(session_id, name, agent.kind, &payload);
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

    /// 移除终态条目（done/failed/shutdown）：chip 的关闭出口；运行中条目拒绝（要停走 agents.stop）。
    /// 连带清掉取消句柄，(session, name) 键不得随条目移除泄漏。
    pub fn dismiss(&self, session_id: &str, name: &str) -> bool {
        let mut map = crate::core::shared::lock(&self.sessions);
        let Some(list) = map.get_mut(session_id) else { return false };
        let Some(pos) = list.iter().position(|a| a.name == name) else { return false };
        if !matches!(list[pos].status, ActivityStatus::Done | ActivityStatus::Failed | ActivityStatus::Shutdown) {
            return false;
        }
        list.remove(pos);
        drop(map);
        crate::core::shared::lock(&self.cancels).remove(&(session_id.to_string(), name.to_string()));
        true
    }

    pub fn list(&self, session_id: &str) -> Vec<AgentActivity> {
        let mut map = crate::core::shared::lock(&self.sessions);
        self.ensure_restored_locked(&mut map, session_id);
        map.get(session_id).cloned().unwrap_or_default()
    }

    pub fn transcript(&self, session_id: &str, name: &str) -> Vec<serde_json::Value> {
        let mut map = crate::core::shared::lock(&self.sessions);
        self.ensure_restored_locked(&mut map, session_id);
        map.get(session_id)
            .and_then(|list| list.iter().find(|a| a.name == name))
            .map(|a| a.transcript.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn drop_session(&self, session_id: &str) {
        crate::core::shared::lock(&self.sessions).remove(session_id);
        crate::core::shared::lock(&self.restored).remove(session_id);
        let mut cancels = crate::core::shared::lock(&self.cancels);
        let tokens: Vec<_> = cancels.extract_if(|(sid, _), _| sid == session_id).map(|(_, token)| token).collect();
        drop(cancels);
        for token in tokens {
            token.cancel();
        }
    }
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
    fn teammate_transcript_write_through_and_rehydrate() {
        let dir = std::env::temp_dir().join(format!("kxen-transcript-{}", std::process::id()));
        let root = dir.join("teams");
        let model = ModelRef::new("p", "m");
        let reg = AgentRegistry::default();
        reg.set_team_root(root.clone());
        reg.register("s1", "w", AgentKind::Teammate, &model);
        reg.push_transcript("s1", "w", serde_json::json!({ "kind": "text", "text": "hello" }));
        reg.push_transcript("s1", "w", serde_json::json!({ "kind": "text", "text": "world" }));
        reg.register("s1", "sub", AgentKind::Subagent, &model);
        reg.push_transcript("s1", "sub", serde_json::json!({ "kind": "text", "text": "ephemeral" }));
        let file = root.join("s1/transcripts/w.jsonl");
        assert_eq!(std::fs::read_to_string(&file).unwrap().lines().count(), 2, "teammate 每条必须写穿一行");
        assert!(!root.join("s1/transcripts/sub.jsonl").exists(), "subagent 一次性派发不得落盘");
        let reg2 = AgentRegistry::default();
        reg2.set_team_root(root.clone());
        reg2.register("s1", "w", AgentKind::Teammate, &model);
        let t = reg2.transcript("s1", "w");
        assert_eq!(t.len(), 2);
        assert_eq!(t[0]["text"], "hello");
        assert_eq!(t[1]["text"], "world");
        reg2.register("s1", "../escape", AgentKind::Teammate, &model);
        reg2.push_transcript("s1", "../escape", serde_json::json!({ "x": 1 }));
        let names: Vec<_> = std::fs::read_dir(root.join("s1/transcripts")).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "非法 name 不得产生新文件: {names:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// register_unique 同路重放：teammate 经定名口注册时磁盘转录照样注水；
    /// subagent 经同口注册不重放（一次性派发，rehydrate 早退）。
    #[test]
    fn register_unique_rehydrates_teammate_transcript() {
        let dir = std::env::temp_dir().join(format!("kxen-rehydrate-unique-{}", std::process::id()));
        let root = dir.join("teams");
        let model = ModelRef::new("p", "m");
        let reg = AgentRegistry::default();
        reg.set_team_root(root.clone());
        reg.register("s1", "w-1", AgentKind::Teammate, &model);
        reg.push_transcript("s1", "w-1", serde_json::json!({ "kind": "text", "text": "persisted" }));
        let reg2 = AgentRegistry::default();
        reg2.set_team_root(root);
        let name = reg2.register_unique("s1", "w", AgentKind::Teammate, &model);
        assert_eq!(name, "w-1");
        let t = reg2.transcript("s1", "w-1");
        assert_eq!(t.len(), 1, "register_unique 注册 teammate 必须重放磁盘转录");
        assert_eq!(t[0]["text"], "persisted");
        let sub = reg2.register_unique("s1", "sub", AgentKind::Subagent, &model);
        assert!(reg2.transcript("s1", &sub).is_empty(), "subagent 一次性派发不得重放");
        std::fs::remove_dir_all(&dir).ok();
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

    /// dismiss 只放终态：运行中/不存在拒绝；移除条目连带清取消句柄
    #[test]
    fn dismiss_only_terminal_and_cleans_cancel_handle() {
        let reg = AgentRegistry::default();
        let model = ModelRef::new("xai", "grok");
        reg.register("s1", "a", AgentKind::Subagent, &model);
        reg.register_cancel("s1", "a", crate::agent::cancel::CancelToken::new());
        assert!(!reg.dismiss("s1", "a"), "working 不得 dismiss");
        assert!(!reg.dismiss("s1", "ghost"), "不存在的 name 返回 false");
        assert!(!reg.dismiss("s2", "a"), "跨 session 同名不得命中");
        reg.set_status("s1", "a", ActivityStatus::Done);
        assert!(reg.dismiss("s1", "a"));
        assert!(reg.list("s1").is_empty() && !reg.cancel("s1", "a"), "dismiss 移除条目并连带清取消句柄");
        for status in [ActivityStatus::Failed, ActivityStatus::Shutdown, ActivityStatus::AwaitingPlanApproval] {
            reg.register("s1", "b", AgentKind::Subagent, &model);
            reg.set_status("s1", "b", status);
            assert_eq!(reg.dismiss("s1", "b"), !matches!(status, ActivityStatus::AwaitingPlanApproval), "{status:?}");
        }
    }

    #[test]
    fn concurrent_register_same_prefix_gets_distinct_names() {
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
