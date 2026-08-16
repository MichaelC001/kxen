use super::*;
use crate::kanban::driver::tests as driver_tests;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn temp(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("kxen-kanban-tool-{tag}-{}-{nanos}", std::process::id()))
}

fn ctx(workspace: &Path) -> AgentContext {
    AgentContext {
        registry: Arc::new(crate::tools::task::TaskRegistry::new()),
        tracker: crate::tools::fs_tool::FileTracker::default(),
        workdir: Arc::from(workspace),
        child_env: None,
        path_grants: Arc::new(Default::default()),
        path_scope: None,
        model: crate::llm::ModelRef::new("p", "m"),
        store: crate::auth::credential::AuthStore::default().into(),
        max_turns: 4,
        max_pure_retries: None,
        mrm: None,
        allowed_tools: None,
        extras: None,
        hooks: None,
        loop_detector: crate::agent::loop_detect::LoopDetector::new(),
        cancel: None,
        team: None,
        team_identity: None,
        session_id: None,
        exec_scope: None,
        bound_goal_id: None,
        goal_binding_frozen: false,
        agents: None,
        bus: None,
        approvals: None,
        kanban_auto: None,
        mcp: None,
        mcp_approval_prechecked: false,
        lsp: None,
        notify: None,
        persist_compaction: None,
        persist_turn: None,
        tool_journal: None,
        domain_tools: None,
        code_orchestration: false,
        auxiliary_usage: Arc::default(),
        usage_reporter: None,
        on_event: Arc::new(|_| {}),
        stream_override: None,
    }
}

fn call(workspace: &Path, name: &str, args: Value) -> Result<String, String> {
    execute_kanban_tool(name, &args, &ctx(workspace))
}

fn create_default_board(workspace: &Path) -> String {
    call(workspace, "kanban_board_create", json!({"board": "board_t", "title": "管线"})).expect("board_create")
}

#[test]
fn board_create_default_template_and_duplicate_guard() {
    let workspace = temp("board");
    let result = create_default_board(&workspace);
    assert!(result.contains("board created: board_t"), "{result}");
    assert!(result.contains("event kev_") && result.contains("seq 1"), "{result}");
    for column in ["requirements", "implementing", "testing", "review", "done"] {
        assert!(result.contains(column), "默认模板缺列 {column}: {result}");
    }
    let error = call(&workspace, "kanban_board_create", json!({"board": "board_t", "title": "again"})).unwrap_err();
    assert!(error.contains("board already created"), "{error}");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn arguments_fail_closed_on_unknown_fields_and_bad_enums() {
    let workspace = temp("args");
    create_default_board(&workspace);
    let error = call(&workspace, "kanban_board_create", json!({"title": "x", "bogus": 1})).unwrap_err();
    assert!(error.contains("invalid arguments") && error.contains("bogus"), "{error}");
    let error = call(&workspace, "kanban_card_move", json!({"board": "board_t", "card_id": "c", "outcome": "sideways"})).unwrap_err();
    assert!(error.contains("invalid arguments"), "{error}");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn column_add_success_and_dangling_target_guard() {
    let workspace = temp("column");
    create_default_board(&workspace);
    let ok = call(
        &workspace,
        "kanban_column_add",
        json!({"board": "board_t", "column": {"id": "archive", "title": "归档", "on_enter": {"kind": "none"}}}),
    )
    .expect("column_add");
    assert!(ok.contains("column added: archive"), "{ok}");
    let error = call(
        &workspace,
        "kanban_column_add",
        json!({"board": "board_t", "column": {"id": "x2", "title": "x", "on_enter": {"kind": "none"}, "transitions": {"on_success": "nowhere"}}}),
    )
    .unwrap_err();
    assert!(error.contains("column not found: nowhere"), "{error}");
    let error = call(&workspace, "kanban_column_add", json!({"board": "board_t", "column": {"id": "done", "title": "dup"}})).unwrap_err();
    assert!(error.contains("column already exists: done"), "{error}");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn card_lifecycle_and_transition_guards() {
    let workspace = temp("card");
    create_default_board(&workspace);
    let created = call(&workspace, "kanban_card_create", json!({"board": "board_t", "title": "Add login", "body": "Email login"}))
        .expect("card_create");
    assert!(created.contains("in column requirements"), "{created}");
    let card_id = created.strip_prefix("card created: ").and_then(|rest| rest.split(' ').next()).expect("card id").to_string();
    // human_gate approve = card_move success，目标列由流转表推导
    let moved =
        call(&workspace, "kanban_card_move", json!({"board": "board_t", "card_id": card_id, "outcome": "success"})).expect("approve move");
    assert!(moved.contains("requirements -> implementing"), "{moved}");
    // requirements 列无 on_failure 出边：reject 必须被流转表守卫拒绝且讲清原因
    let back = call(&workspace, "kanban_card_move", json!({"board": "board_t", "card_id": card_id, "outcome": "failure"})).unwrap();
    assert!(back.contains("implementing -> requirements"), "on_failure 回流: {back}");
    let error = call(&workspace, "kanban_card_move", json!({"board": "board_t", "card_id": card_id, "outcome": "failure"})).unwrap_err();
    assert!(error.contains("invalid transition") && error.contains("requirements"), "{error}");
    let error =
        call(&workspace, "kanban_card_move", json!({"board": "board_t", "card_id": "card_nope", "outcome": "success"})).unwrap_err();
    assert!(error.contains("card not found"), "{error}");
    let commented =
        call(&workspace, "kanban_card_comment", json!({"board": "board_t", "card_id": card_id, "body": "先做这条"})).expect("card_comment");
    assert!(commented.contains(&format!("comment added on {card_id}")), "{commented}");
    let error = call(&workspace, "kanban_card_comment", json!({"board": "board_t", "card_id": "card_nope", "body": "x"})).unwrap_err();
    assert!(error.contains("card not found"), "{error}");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn agent_create_saves_file_and_event_and_validates_first() {
    let workspace = temp("agent");
    create_default_board(&workspace);
    let ok = call(
        &workspace,
        "kanban_agent_create",
        json!({"board": "board_t", "name": "qa-x", "role": "review", "model": "auto", "permission_profile": "readonly+test",
               "prompt": "Verify the card and declare a verdict."}),
    )
    .expect("agent_create");
    assert!(ok.contains("agent defined: qa-x"), "{ok}");
    let file = workspace.join(".kxen/kanban/agents/qa-x.md");
    assert!(file.is_file(), "定义文件必须落盘");
    let board = Board::open(&workspace, "board_t").unwrap();
    assert_eq!(board.state().agents["qa-x"].permission_profile, "readonly+test", "agent_defined 事件登记元数据");
    // 守卫失败零副作用：未知 profile 不落文件、不落事件
    let error = call(
        &workspace,
        "kanban_agent_create",
        json!({"board": "board_t", "name": "bad", "role": "r", "model": "auto", "permission_profile": "root", "prompt": "x"}),
    )
    .unwrap_err();
    assert!(error.contains("unknown permission_profile"), "{error}");
    assert!(!workspace.join(".kxen/kanban/agents/bad.md").exists());
    assert!(!board.state().agents.contains_key("bad"));
    // 未建板拒绝
    let error = call(
        &workspace,
        "kanban_agent_create",
        json!({"board": "board_none", "name": "qa-x", "role": "r", "model": "auto", "permission_profile": "readonly", "prompt": "x"}),
    )
    .unwrap_err();
    assert!(error.contains("board not created"), "{error}");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn agent_create_custom_profile_tools_roundtrip_and_fail_closed() {
    let workspace = temp("agentcustom");
    create_default_board(&workspace);
    // custom + tools 成功：文件落盘、事件 payload 带 tools
    let ok = call(
        &workspace,
        "kanban_agent_create",
        json!({"board": "board_t", "name": "go-editor", "role": "execution", "model": "auto", "permission_profile": "custom",
               "tools": ["read", "glob", "grep", "edit", "write", "exec", "lsp"],
               "prompt": "Edit Go code and declare a verdict."}),
    )
    .expect("custom agent_create");
    assert!(ok.contains("agent defined: go-editor"), "{ok}");
    let file = workspace.join(".kxen/kanban/agents/go-editor.md");
    assert!(file.is_file(), "定义文件必须落盘");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("tools: read,glob,grep,edit,write,exec,lsp"), "tools 行必须进 frontmatter: {text}");
    let board = Board::open(&workspace, "board_t").unwrap();
    let agent = &board.state().agents["go-editor"];
    assert_eq!(agent.permission_profile, "custom");
    assert_eq!(agent.tools.as_ref().unwrap().len(), 7, "tools 必须进 agent_defined 事件 payload");
    let seq_before = board.state().seq;
    // custom 缺 tools -> Err 且零副作用
    let error = call(
        &workspace,
        "kanban_agent_create",
        json!({"board": "board_t", "name": "bad-a", "role": "r", "model": "auto", "permission_profile": "custom", "prompt": "x"}),
    )
    .unwrap_err();
    assert!(error.contains("requires"), "{error}");
    // tools 含闭集外名（派发面）-> Err 且零副作用
    let error = call(
        &workspace,
        "kanban_agent_create",
        json!({"board": "board_t", "name": "bad-b", "role": "r", "model": "auto", "permission_profile": "custom",
               "tools": ["read", "kanban_agent_create"], "prompt": "x"}),
    )
    .unwrap_err();
    assert!(error.contains("allowlist"), "{error}");
    assert!(!workspace.join(".kxen/kanban/agents/bad-a.md").exists());
    assert!(!workspace.join(".kxen/kanban/agents/bad-b.md").exists());
    let board = Board::open(&workspace, "board_t").unwrap();
    assert!(!board.state().agents.contains_key("bad-a") && !board.state().agents.contains_key("bad-b"));
    assert_eq!(board.state().seq, seq_before, "非法输入不得新增事件");
    std::fs::remove_dir_all(workspace).ok();
}

#[tokio::test]
async fn agent_run_claim_is_adopted_by_runner_and_executed() {
    let workspace = temp("run");
    crate::kanban::save_agent_definition(&workspace, &driver_tests::agent_def()).unwrap();
    call(
        &workspace,
        "kanban_board_create",
        json!({"board": "board_t", "title": "t", "columns": [
            {"id": "implementing", "title": "实现中", "on_enter": {"kind": "agent_run", "agent": "exec-impl"},
             "transitions": {"on_success": "done", "on_failure": "implementing"}},
            {"id": "done", "title": "完成", "on_enter": {"kind": "none"}}
        ]}),
    )
    .expect("board_create");
    let created = call(&workspace, "kanban_card_create", json!({"board": "board_t", "title": "Add login"})).expect("card_create");
    let card_id = created.strip_prefix("card created: ").and_then(|rest| rest.split(' ').next()).unwrap().to_string();
    // Runner 先于 claim 创建（生产语义：显式 claim 晚于 boot，走收养而非 orphan 恢复）
    let runner = crate::kanban::Runner::new();
    let claimed = call(&workspace, "kanban_agent_run", json!({"board": "board_t", "card_id": card_id})).expect("agent_run");
    assert!(claimed.contains("run claimed: board_t:") && claimed.contains("attempt: 1"), "{claimed}");
    // 在飞守卫：同卡重复 claim 拒绝
    let error = call(&workspace, "kanban_agent_run", json!({"board": "board_t", "card_id": card_id})).unwrap_err();
    assert!(error.contains("run in progress"), "{error}");
    let launched =
        runner.scan_once(&workspace, &driver_tests::deps(&workspace, driver_tests::text_stream("done\nVERDICT: success"))).await.unwrap();
    assert_eq!(launched, 1, "工具提交的显式 claim 必须被 runner 收养执行");
    let board = Board::open(&workspace, "board_t").unwrap();
    let run = board.state().runs.values().next().unwrap();
    for _ in 0..100 {
        if run_outcome(&workspace, run.id.clone()).is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let board = Board::open(&workspace, "board_t").unwrap();
    assert_eq!(board.state().runs[&run.id].outcome, Some(Outcome::Success));
    assert_eq!(board.state().cards[&card_id].column_id, "done", "on_success 流转到 done");
    std::fs::remove_dir_all(workspace).ok();
}

fn run_outcome(workspace: &Path, run_id: String) -> Option<Outcome> {
    Board::open(workspace, "board_t").ok()?.state().runs.get(&run_id)?.outcome
}

#[test]
fn agent_run_rejects_non_executable_column() {
    let workspace = temp("ranguard");
    create_default_board(&workspace);
    let created = call(&workspace, "kanban_card_create", json!({"board": "board_t", "title": "x"})).unwrap();
    let card_id = created.strip_prefix("card created: ").and_then(|rest| rest.split(' ').next()).unwrap().to_string();
    let error = call(&workspace, "kanban_agent_run", json!({"board": "board_t", "card_id": card_id})).unwrap_err();
    assert!(error.contains("has no agent_run/workflow on_enter"), "{error}");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn board_show_renders_state_and_rejects_missing_board() {
    let workspace = temp("show");
    let error = call(&workspace, "kanban_board_show", json!({"board": "board_t"})).unwrap_err();
    assert!(error.contains("board not created"), "{error}");
    create_default_board(&workspace);
    let created = call(&workspace, "kanban_card_create", json!({"board": "board_t", "title": "Add login"})).unwrap();
    let card_id = created.strip_prefix("card created: ").and_then(|rest| rest.split(' ').next()).unwrap().to_string();
    let shown = call(&workspace, "kanban_board_show", json!({"board": "board_t"})).expect("board_show");
    assert!(shown.contains("board board_t"), "{shown}");
    assert!(shown.contains("- requirements") && shown.contains("on_enter=human_gate"), "{shown}");
    assert!(shown.contains(&format!("* {card_id}")) && shown.contains("status=waiting_human"), "{shown}");
    assert!(shown.contains("runs:\n- none") && shown.contains("agents:\n- none"), "{shown}");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn board_show_renders_policy_state() {
    let workspace = temp("showpolicy");
    create_default_board(&workspace);
    let shown = call(&workspace, "kanban_board_show", json!({"board": "board_t"})).expect("board_show");
    assert!(shown.contains("policy:\n- none"), "{shown}");
    let mut board = Board::open(&workspace, "board_t").unwrap();
    board
        .apply(KanbanCommand::PolicySet {
            policy: crate::kanban::PolicySpec {
                allowlist: vec!["cargo".into(), "git status".into()],
                expires_at_ms: None,
                max_uses: Some(5),
            },
        })
        .unwrap();
    let shown = call(&workspace, "kanban_board_show", json!({"board": "board_t"})).expect("board_show");
    assert!(shown.contains("policy:\n- allowlist=2 used=0 max_uses=5 expires_at_ms=none"), "{shown}");
    std::fs::remove_dir_all(workspace).ok();
}

#[test]
fn kanban_tools_are_deferred_discoverable_and_identity_filtered() {
    let names: Vec<String> = crate::agent::tools_spec::deferred_tools().into_iter().map(|tool| tool.function.name.clone()).collect();
    for tool in crate::agent::tools_kanban::kanban_tools() {
        assert!(names.contains(&tool.function.name), "deferred 目录缺 {}", tool.function.name);
    }
    // tool_search 的过滤口径（execute.rs）：query 词命中 name+description
    let query = "kanban";
    let matches: Vec<_> = crate::agent::tools_spec::deferred_tools()
        .into_iter()
        .filter(|tool| format!("{} {}", tool.function.name, tool.function.description).to_lowercase().contains(query))
        .map(|tool| tool.function.name.clone())
        .collect();
    assert_eq!(matches.len(), 8, "tool_search 必须能检索到全部 kanban 工具: {matches:?}");
    // 身份白名单（helpers.deferred_visible = 挂载集 ∩ 白名单）：主 Agent 无白名单全可见，
    // readonly 受限身份即使同 session 已挂载也不可见
    let extras = crate::agent::agent_loop::SessionExtras::default();
    extras.extra_tools.lock().expect("tools").insert("kanban_board_show".to_string());
    let visible: Vec<_> =
        super::super::helpers::deferred_visible(Some(&extras), None).into_iter().map(|tool| tool.function.name.clone()).collect();
    assert_eq!(visible, ["kanban_board_show"]);
    let readonly: Vec<String> = ["read", "glob", "grep"].iter().map(|name| name.to_string()).collect();
    assert!(super::super::helpers::deferred_visible(Some(&extras), Some(&readonly)).is_empty());
}
