//! AgentDefined 命令的 custom/tools 口径守卫：custom 必须显式工具集且过闭集，固定档禁止自带。
use super::*;

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-command-tools-{tag}-{}-{nanos}", std::process::id()))
}

fn open_board(workspace: &Path) -> Board {
    let mut board = Board::open(workspace, "board_t").unwrap();
    board.apply(KanbanCommand::BoardCreate { title: "测试看板".into(), columns: None }).unwrap();
    board
}

fn agent_defined(permission_profile: &str, tools: Option<Vec<String>>) -> KanbanCommand {
    KanbanCommand::AgentDefined {
        name: "go-editor".into(),
        role: "r".into(),
        model: "auto".into(),
        permission_profile: permission_profile.into(),
        tools,
    }
}

#[test]
fn custom_requires_tools_and_fixed_profiles_forbid_them() {
    let workspace = temp("guard");
    let mut board = open_board(&workspace);
    let seq_before = board.state().seq;
    // custom 缺 tools 拒
    assert!(matches!(board.apply(agent_defined("custom", None)), Err(KanbanError::InvalidAgentDef(_))));
    // custom 空 tools 拒
    assert!(matches!(board.apply(agent_defined("custom", Some(vec![]))), Err(KanbanError::InvalidAgentDef(_))));
    // custom tools 含闭集外名拒
    let bad = Some(vec!["read".into(), "agent".into()]);
    assert!(matches!(board.apply(agent_defined("custom", bad)), Err(KanbanError::InvalidAgentDef(_))));
    // 固定三档带 tools 拒（权限语义单一来源）
    for profile in ["readonly", "readonly+test", "full"] {
        let tools = Some(vec!["read".into()]);
        assert!(matches!(board.apply(agent_defined(profile, tools)), Err(KanbanError::InvalidAgentDef(_))), "{profile} 不得自带 tools");
    }
    assert_eq!(board.state().seq, seq_before, "拒绝的命令不得落事件");
    // custom + 合法 tools 通过，投影带 tools
    let tools = Some(vec!["read".into(), "lsp".into()]);
    board.apply(agent_defined("custom", tools.clone())).unwrap();
    assert_eq!(board.state().agents["go-editor"].tools, tools);
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn legacy_agent_defined_event_without_tools_replays() {
    // 旧事件流（无 tools 字段的 JSON 行）必须能正常 replay：serde default 兼容
    let workspace = temp("legacy");
    let mut board = open_board(&workspace);
    let event = board.apply(agent_defined("full", None)).unwrap();
    let json = serde_json::to_string(&event).unwrap();
    let legacy = json.replace(r#","tools":null"#, "");
    assert_ne!(json, legacy, "测试前提：序列化带 tools 字段，剔除后模拟旧行");
    let parsed: KanbanEvent = serde_json::from_str(&legacy).unwrap();
    assert!(matches!(&parsed.kind, EventKind::AgentDefined(payload) if payload.tools.is_none()));
    let mut state = BoardState::new("board_t");
    projection::reduce(&mut state, &parsed).unwrap();
    std::fs::remove_dir_all(workspace).ok();
}
