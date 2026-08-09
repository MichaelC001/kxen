use super::*;
use crate::agent::activity::AgentKind;

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-activity-disk-{tag}-{}-{nanos}", std::process::id()))
}

#[test]
fn paths_split_by_kind_and_reject_invalid_ids() {
    let team_root = PathBuf::from("/teams");
    let agents_root = PathBuf::from("/sessions");
    assert_eq!(
        transcript_path(Some(&team_root), None, AgentKind::Teammate, "s1", "w").unwrap(),
        PathBuf::from("/teams/s1/transcripts/w.jsonl")
    );
    assert_eq!(
        transcript_path(None, Some(&agents_root), AgentKind::Subagent, "s1", "review-1").unwrap(),
        PathBuf::from("/sessions/s1/agents/review-1.transcript.jsonl")
    );
    assert_eq!(run_log_path(Some(&agents_root), "s1", "review-1").unwrap(), PathBuf::from("/sessions/s1/agents/review-1.turns.jsonl"));
    assert!(transcript_path(None, Some(&agents_root), AgentKind::Workflow, "s1", "wf-1").is_none(), "workflow 有 journal 不双写");
    assert!(transcript_path(None, Some(&agents_root), AgentKind::Subagent, "s1", "../escape").is_none());
    assert!(transcript_path(None, None, AgentKind::Subagent, "s1", "a").is_none(), "无 root = 纯内存");
    assert!(run_log_path(None, "s1", "a").is_none());
}

#[test]
fn scan_restores_done_and_interrupted_runs() {
    let dir = temp("scan");
    let agents = dir.join("s1/agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(agents.join("review-1.transcript.jsonl"), "{\"kind\":\"text\",\"text\":\"looking\"}\n{\"kind\":\"done\",\"turns\":2}\n")
        .unwrap();
    std::fs::write(agents.join("exec-1.transcript.jsonl"), "{\"kind\":\"text\",\"text\":\"half\"}\nnot-json\n").unwrap();
    // turns 文件与无关文件不参与恢复；其他 session 不串
    std::fs::write(agents.join("review-1.turns.jsonl"), "{}\n").unwrap();
    std::fs::create_dir_all(dir.join("s2/agents")).unwrap();

    let restored = scan_session(&dir, "s1");
    assert_eq!(restored.len(), 2);
    let done = restored.iter().find(|a| a.name == "review-1").unwrap();
    assert_eq!(done.status, ActivityStatus::Done);
    assert_eq!(done.transcript.len(), 2);
    let interrupted = restored.iter().find(|a| a.name == "exec-1").unwrap();
    assert_eq!(interrupted.status, ActivityStatus::Shutdown, "无 done 事件 = 进程中断");
    assert_eq!(interrupted.transcript.len(), 1, "坏行跳过");
    assert!(scan_session(&dir, "s2").is_empty());
    assert!(scan_session(&dir, "../escape").is_empty());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn scan_records_terminal_kind_and_keeps_shutdown_status_mapping() {
    let dir = temp("terminal");
    let agents = dir.join("s1/agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(agents.join("done-1.transcript.jsonl"), "{\"kind\":\"done\",\"turns\":1}\n").unwrap();
    std::fs::write(agents.join("abort-1.transcript.jsonl"), "{\"kind\":\"text\",\"text\":\"x\"}\n{\"kind\":\"aborted\"}\n").unwrap();
    std::fs::write(agents.join("err-1.transcript.jsonl"), "{\"kind\":\"error\",\"message\":\"boom\"}\n").unwrap();
    std::fs::write(agents.join("half-1.transcript.jsonl"), "{\"kind\":\"text\",\"text\":\"x\"}\n").unwrap();

    let restored = scan_session(&dir, "s1");
    let find = |name: &str| restored.iter().find(|a| a.name == name).unwrap();
    assert_eq!(find("done-1").terminal, Some(TerminalKind::Done));
    assert_eq!(find("done-1").status, ActivityStatus::Done);
    // UI 映射回归：aborted/error 仍是 Shutdown，仅 terminal 分流恢复决策
    assert_eq!(find("abort-1").terminal, Some(TerminalKind::Aborted));
    assert_eq!(find("abort-1").status, ActivityStatus::Shutdown);
    assert_eq!(find("err-1").terminal, Some(TerminalKind::Error));
    assert_eq!(find("err-1").status, ActivityStatus::Shutdown);
    assert_eq!(find("half-1").terminal, None, "无终态 = 进程死在完结前");
    assert_eq!(find("half-1").status, ActivityStatus::Shutdown);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn scan_caps_transcript_tail() {
    let dir = temp("cap");
    let agents = dir.join("s1/agents");
    std::fs::create_dir_all(&agents).unwrap();
    let lines: Vec<String> = (0..250).map(|i| format!("{{\"kind\":\"text\",\"text\":\"{i}\"}}")).collect();
    std::fs::write(agents.join("a-1.transcript.jsonl"), lines.join("\n") + "\n").unwrap();
    let restored = scan_session(&dir, "s1");
    assert_eq!(restored[0].transcript.len(), TRANSCRIPT_CAP);
    assert_eq!(restored[0].transcript[0]["text"], "50", "最旧的应被淘汰");
    std::fs::remove_dir_all(dir).ok();
}

/// registry 端到端：subagent 写穿 -> 进程重启（新 registry 同 root）-> 惰性恢复可检查记录；
/// 恢复条目占位唯一名，重启后新派发不与落盘转录撞名（转录交错防线）。
#[test]
fn subagent_transcript_write_through_and_lazy_restore_after_restart() {
    use crate::agent::activity::{ActivityStatus, AgentRegistry};
    let dir = temp("registry-restore");
    let sessions_root = dir.join("sessions");
    let model = crate::llm::ModelRef::new("p", "m");
    let reg = AgentRegistry::default();
    reg.set_agents_root(sessions_root.clone());
    let name = reg.register_unique("s1", "review", AgentKind::Subagent, &model);
    assert_eq!(name, "review-1");
    reg.push_transcript("s1", &name, serde_json::json!({ "kind": "text", "text": "finding" }));
    reg.push_transcript("s1", &name, serde_json::json!({ "kind": "done", "turns": 1 }));
    let file = sessions_root.join("s1/agents/review-1.transcript.jsonl");
    assert_eq!(std::fs::read_to_string(&file).unwrap().lines().count(), 2, "subagent 每条必须写穿一行");

    let reg2 = AgentRegistry::default();
    reg2.set_agents_root(sessions_root.clone());
    let list = reg2.list("s1");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "review-1");
    assert!(matches!(list[0].status, ActivityStatus::Done), "含 done 事件 = 已完结");
    assert_eq!(reg2.transcript("s1", "review-1").len(), 2, "转录必须从盘重建");
    assert_eq!(reg2.register_unique("s1", "review", AgentKind::Subagent, &model), "review-2");

    // 中断 run（无 done 事件）恢复为 Shutdown
    let reg3 = AgentRegistry::default();
    reg3.set_agents_root(sessions_root.clone());
    let half = reg3.register_unique("s1", "exec", AgentKind::Subagent, &model);
    reg3.push_transcript("s1", &half, serde_json::json!({ "kind": "text", "text": "half" }));
    let reg4 = AgentRegistry::default();
    reg4.set_agents_root(sessions_root);
    let list = reg4.list("s1");
    let restored = list.iter().find(|a| a.name == half).unwrap();
    assert!(matches!(restored.status, ActivityStatus::Shutdown), "无 done 事件 = 进程中断");
    assert_eq!(list.len(), 2);
    std::fs::remove_dir_all(dir).ok();
}
