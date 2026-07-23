//! Agent Teams：lead（主会话）+ teammates（常驻 inbox loop，可绑不同订阅模型）。
//! 存储 `data_dir/teams/<session_id>/`：config.json（members）+ tasks.json + inboxes/<name>.json。
//! 协调：tasks 依赖自动解锁（进程内 Mutex 串行 claim）；mailbox 追加写 + 读取校验；plan 审批门。

mod inbox;
mod manager;
mod member_loop;
mod spawn;
mod tasks;
mod types;

use std::sync::Arc;

pub use manager::TeamManager;
pub use types::{Member, MemberStatus, SpawnDeps, TeamTask, TeamTaskStatus};
pub(crate) use types::TeamState;

// ---------------- 测试（存储与任务逻辑，不触网） ----------------

#[cfg(test)]
mod tests {
    use super::inbox::{append_inbox, drain_inbox};
    use super::tasks::{claim_task, complete_task, create_task};
    use super::*;
    use crate::core::event::EventBus;
    use crate::core::shared::lock;
    use crate::llm::mrm::ModelResourceManager;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn deps() -> SpawnDeps {
        let config = crate::core::config::Config::default();
        SpawnDeps {
            registry: Arc::new(crate::tools::task::TaskRegistry::new()),
            workdir: Arc::from(Path::new("/tmp")),
            store: crate::auth::credential::AuthStore::default(),
            mrm: Arc::new(std::sync::RwLock::new(Arc::new(ModelResourceManager::new(config)))),
            hooks: None,
            extras: Arc::new(crate::agent::agent_loop::SessionExtras::default()),
            agents: Arc::new(crate::agent::activity::AgentRegistry::default()),
            approvals: None,
            mcp: None,
            lsp: None,
        }
    }

    fn manager(tag: &str) -> (Arc<TeamManager>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("kxen-team-{tag}-{}", std::process::id()));
        let mgr = TeamManager::new(dir.clone(), deps(), EventBus::default());
        (mgr, dir)
    }

    #[tokio::test]
    async fn task_dependency_unlocks_on_complete() {
        let (mgr, dir) = manager("deps");
        let state = mgr.state_for("s1");
        let t1 = create_task(&state, "first", vec![]);
        let _t2 = create_task(&state, "second", vec![t1.id]);
        assert!(claim_task(&state, "a").unwrap().contains("first"));
        assert!(claim_task(&state, "b").is_err(), "t2 应被依赖阻塞");
        complete_task(&state, "a", t1.id).await.unwrap();
        assert!(claim_task(&state, "b").unwrap().contains("second"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inbox_drain_validates_and_clears() {
        let (mgr, dir) = manager("inbox");
        let state = mgr.state_for("s1");
        append_inbox(&state.dir, "a", "x", "hello").unwrap();
        let path = dir.join("s1/inboxes/a.json");
        let mut content = std::fs::read_to_string(&path).unwrap();
        content.push_str("not json\n");
        std::fs::write(&path, content).unwrap();
        let drained = drain_inbox(&state.dir, "a");
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].1, "hello");
        assert!(drain_inbox(&state.dir, "a").is_empty(), "drain 后应清空");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lead_inbox_via_manager() {
        let (mgr, dir) = manager("lead");
        let state = mgr.state_for("s1");
        mgr.send(&state, "worker1", "lead", "result here").unwrap();
        let drained = mgr.drain_lead_inbox("s1");
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].0, "worker1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn push_member(state: &TeamState, name: &str, role: &str) {
        lock(&state.members).push(Member {
            name: name.into(),
            role: role.into(),
            model: crate::llm::ModelRef::new("p", "m"),
            status: MemberStatus::Idle,
            plan_approval: false,
        });
    }

    #[test]
    fn observer_receives_traffic_copy() {
        let (mgr, dir) = manager("observer");
        let state = mgr.state_for("s1");
        push_member(&state, "a", "execution");
        push_member(&state, "b", "execution");
        push_member(&state, "c", "observer");
        // teammate 互发抄送
        mgr.send(&state, "a", "b", "ping").unwrap();
        let feed = drain_inbox(&state.dir, "c");
        assert_eq!(feed.len(), 1);
        assert_eq!(feed[0].0, "feed", "observer 抄送 from=feed，防误判为 lead 直发");
        assert!(feed[0].1.contains("[observed a -> b] ping"));
        // 上报 lead 也抄送
        mgr.send(&state, "a", "lead", "done").unwrap();
        let feed2 = drain_inbox(&state.dir, "c");
        assert_eq!(feed2.len(), 1);
        assert!(feed2[0].1.contains("[observed a -> lead]"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn roster_injected_into_system_prompt() {
        let (mgr, dir) = manager("roster");
        let state = mgr.state_for("s1");
        push_member(&state, "a", "execution");
        let sys = super::member_loop::teammate_system(&state, "a", "execution", true);
        assert!(sys.contains("Current team roster:"));
        assert!(sys.contains("- a (role: execution"));
        let obs = super::member_loop::teammate_system(&state, "c", "observer", true);
        assert!(obs.contains("OBSERVER"), "observer 角色应有专属指引");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[allow(dead_code)]
fn _assert_futures_send(mgr: &Arc<TeamManager>, args: &serde_json::Value) {
    fn assert_send<T: Send>(_: T) {}
    assert_send(mgr.lead_action("s", args));
}

#[allow(dead_code)]
fn _assert_resolve_send(mrm: &crate::llm::mrm::ModelResourceManager) {
    fn assert_send<T: Send>(_: T) {}
    assert_send(mrm.resolve("thinking", &crate::auth::credential::AuthStore::new()));
}
