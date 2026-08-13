use super::*;
use crate::core::goal::{Goal, GoalBudget, GoalContract, GoalStatus};
use crate::core::schedule::CronJob;
use crate::kanban::KanbanDigest;

#[test]
fn touch_orders_by_recency() {
    let dir = std::env::temp_dir().join(format!("kxen-ws-{}", std::process::id()));
    touch(&dir, "/a").unwrap();
    touch(&dir, "/b").unwrap();
    touch(&dir, "/a").unwrap();
    let all = list(&dir).unwrap();
    assert_eq!(all[0].path, "/a");
    assert_eq!(all.len(), 2);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn corrupt_recent_list_blocks_touch_without_overwrite() {
    let dir = std::env::temp_dir().join(format!("kxen-ws-corrupt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(file(&dir), "{not json").unwrap();
    assert!(list(&dir).is_err());
    assert!(touch(&dir, "/new").is_err());
    assert_eq!(std::fs::read_to_string(file(&dir)).unwrap(), "{not json");
    std::fs::remove_dir_all(&dir).ok();
}

fn session(id: &str, dir: &str, updated: u64) -> crate::core::session::Session {
    crate::core::session::Session {
        id: id.into(),
        title: format!("标题-{id}"),
        directory: dir.into(),
        parent_id: None,
        branch_root_id: None,
        fork_point: None,
        fork_kind: None,
        created_at: 0,
        updated_at: updated,
        message_revision: 0,
        pinned: false,
        sort_order: None,
        model: None,
    }
}

fn goal(id: &str, sid: Option<&str>, status: GoalStatus, updated: u64) -> Goal {
    Goal {
        id: id.into(),
        contract: GoalContract {
            objective: format!("目标-{id}"),
            completion_criteria: "标准".into(),
            constraints: None,
            budget: GoalBudget::default(),
        },
        status,
        created_at: 0,
        updated_at: updated,
        activated_at: None,
        turns_used: 0,
        tokens_used: 0,
        unmetered_calls: 0,
        acknowledged_unmetered_calls: 0,
        last_block_reason: None,
        consecutive_blocks: 0,
        block_reason: None,
        verification_evidence: None,
        session_id: sid.map(String::from),
        paused_ms: 0,
        paused_at: None,
        metering_receipts: Vec::new(),
        completion_attempt: None,
    }
}

fn cron_job(id: &str, sid: &str) -> CronJob {
    CronJob {
        id: id.into(),
        cron: "* * * * *".into(),
        prompt: "p".into(),
        session_id: sid.into(),
        once: false,
        next_fire: 0,
        enabled: true,
        history: std::collections::VecDeque::new(),
        dispatch_id: None,
    }
}

#[test]
fn overview_aggregates_sessions() {
    let ws = vec![Workspace { path: "/a".into(), last_used: 100 }, Workspace { path: "/b".into(), last_used: 200 }];
    let sessions = vec![session("s1", "/a", 500), session("s2", "/a", 900), session("s3", "/b", 300)];
    let running: HashSet<String> = ["s2".to_string()].into_iter().collect();
    let cards = overview(ws, &sessions, &running, &HashMap::new(), &[], &[], &OverviewInjections::default());
    assert_eq!(cards[0].sessions, 2);
    assert_eq!(cards[0].running, 1);
    assert_eq!(cards[0].last_activity, 900, "会话 updated_at 优先于 workspace last_used");
    assert_eq!(cards[1].sessions, 1);
    assert_eq!(cards[1].running, 0);
    assert_eq!(cards[1].last_activity, 300);
    assert!(cards[0].dirty.is_none(), "/a 非 git 仓库");
}

#[test]
fn overview_board_fields() {
    let ws = vec![Workspace { path: "/a".into(), last_used: 100 }, Workspace { path: "/b".into(), last_used: 200 }];
    let sessions = vec![session("s1", "/a", 500), session("s2", "/a", 900), session("s3", "/b", 300)];
    let running: HashSet<String> = ["s2".to_string()].into_iter().collect();
    let queued: HashMap<String, usize> = [("s1".to_string(), 2), ("s2".to_string(), 1), ("s3".to_string(), 5)].into_iter().collect();
    let goals = vec![
        goal("g1", Some("s1"), GoalStatus::Active, 100),
        goal("g2", Some("s2"), GoalStatus::Blocked, 200),
        goal("g3", Some("s1"), GoalStatus::Complete, 300),
        goal("g4", None, GoalStatus::Active, 400),
    ];
    let cron = vec![cron_job("c1", "s1"), cron_job("c2", "s3"), cron_job("c3", "s9")];
    let mut worktrees: HashMap<String, Vec<WorktreeDigest>> = HashMap::new();
    worktrees.insert(
        "/a".to_string(),
        vec![WorktreeDigest {
            name: "exp".into(),
            branch: "kxen/exp".into(),
            path: "/a/.kxen/worktrees/exp".into(),
            dirty: Some(3),
            sessions: 0,
            running: 0,
        }],
    );
    let mut kanban: HashMap<String, Vec<KanbanDigest>> = HashMap::new();
    kanban.insert(
        "/a".to_string(),
        vec![KanbanDigest {
            board_id: "board_1".into(), title: "交付板".into(), total_cards: 4, waiting_human: 2, running: 1, blocked: 1
        }],
    );

    let cards = overview(ws, &sessions, &running, &queued, &goals, &cron, &OverviewInjections { worktrees, kanban });
    let a = &cards[0];
    let b = &cards[1];

    assert_eq!(a.running_sessions.len(), 1);
    assert_eq!(a.running_sessions[0].id, "s2");
    assert_eq!(a.running_sessions[0].title, "标题-s2");
    assert_eq!(a.running_sessions[0].queued, 1, "运行中会话带自身排队数");
    assert_eq!(a.queued, 3, "workspace 排队总数 = 各会话队列之和");
    assert_eq!(b.queued, 5);

    let g = a.goal.as_ref().expect("活态 goal 应命中");
    assert_eq!(g.id, "g2", "多个活态 goal 取最近更新");
    assert_eq!(g.status, "blocked");
    assert!(b.goal.is_none(), "g4 是全局 goal，不归属任何 workspace 卡片");

    assert_eq!(a.cron, 1, "只数绑定到本 workspace 会话的 job");
    assert_eq!(b.cron, 1);
    assert_eq!(a.worktrees.len(), 1);
    assert_eq!(a.worktrees[0].branch, "kxen/exp");
    assert_eq!(a.worktrees[0].dirty, Some(3));
    assert_eq!(a.worktrees[0].sessions, 0, "无会话 directory 落在该树下");
    assert_eq!(a.worktrees[0].running, 0);
    assert!(b.worktrees.is_empty());

    assert_eq!(a.kanban.len(), 1, "digest 按 workspace 路径注入");
    assert_eq!(a.kanban[0].board_id, "board_1");
    assert_eq!(a.kanban[0].waiting_human, 2);
    assert_eq!(a.kanban[0].running, 1);
    assert_eq!(a.kanban[0].blocked, 1);
    assert!(b.kanban.is_empty(), "无注入的 workspace 是空列表而不是缺失");
}

#[test]
fn overview_worktree_binding() {
    let ws = vec![Workspace { path: "/a".into(), last_used: 100 }];
    let tree = "/a/.kxen/worktrees/exp";
    let sessions = vec![
        session("s1", tree, 500),                         // 根部精确匹配
        session("s2", "/a/.kxen/worktrees/exp/sub", 600), // 子目录前缀匹配
        session("s3", "/a/.kxen/worktrees/exp2", 700),    // 同前缀不同树：不算绑定
        session("s4", "/a", 800),                         // 主仓会话：不算绑定
    ];
    let running: HashSet<String> = ["s2".to_string()].into_iter().collect();
    let mut worktrees: HashMap<String, Vec<WorktreeDigest>> = HashMap::new();
    worktrees.insert(
        "/a".to_string(),
        vec![WorktreeDigest { name: "exp".into(), branch: "kxen/exp".into(), path: tree.into(), dirty: None, sessions: 0, running: 0 }],
    );

    let cards = overview(ws, &sessions, &running, &HashMap::new(), &[], &[], &OverviewInjections { worktrees, ..Default::default() });
    let t = &cards[0].worktrees[0];
    assert_eq!(t.sessions, 2, "根部 + 子目录算绑定");
    assert_eq!(t.running, 1, "运行中只数绑定会话里的 s2");
    assert_eq!(cards[0].sessions, 1, "绑定到树的会话不计入主仓会话数");
}
