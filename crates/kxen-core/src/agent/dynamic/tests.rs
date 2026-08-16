//! 动态工具单测：命名/hash、注册表可见性、快照恢复、参数校验、审批边界、沙箱执行。

use super::*;
use crate::agent::agent_loop::{AgentContext, SessionExtras};
use serde_json::json;
use std::sync::Arc;

pub(super) fn def(segment: &str, implementation: &str) -> DynamicToolDef {
    DynamicToolDef {
        schema_version: DYNAMIC_TOOL_SCHEMA,
        name: qualified_name(segment, implementation).unwrap(),
        description: "demo tool".into(),
        parameters: json!({ "type": "object", "properties": { "x": { "type": "string" } }, "required": ["x"] }),
        implementation: implementation.into(),
        implementation_hash: implementation_hash(implementation),
    }
}

// 卸载路径测试拆在 undefine_tests.rs（350 行门禁），共享以下脚手架
pub(super) fn fresh_extras() -> SessionExtras {
    SessionExtras::default()
}

pub(super) fn ctx_with(extras: &Arc<SessionExtras>, approvals: Option<Arc<crate::agent::approval::ApprovalBroker>>) -> AgentContext {
    AgentContext {
        registry: Arc::new(crate::tools::task::TaskRegistry::new()),
        tracker: crate::tools::fs_tool::FileTracker::default(),
        workdir: Arc::from(std::path::Path::new("/tmp")),
        child_env: None,
        path_grants: Arc::new(Default::default()),
        path_scope: None,
        model: crate::llm::ModelRef::new("p", "m"),
        store: crate::auth::credential::AuthStore::default().into(),
        max_turns: 4,
        max_pure_retries: None,
        mrm: None,
        allowed_tools: None,
        extras: Some(extras.clone()),
        hooks: None,
        loop_detector: crate::agent::loop_detect::LoopDetector::new(),
        cancel: None,
        team: None,
        team_identity: None,
        session_id: Some("ses-dyn".into()),
        exec_scope: None,
        bound_goal_id: None,
        goal_binding_frozen: false,
        agents: None,
        bus: None,
        approvals,
        kanban_auto: None,
        mcp: None,
        mcp_approval_prechecked: false,
        lsp: None,
        notify: None,
        persist_compaction: None,
        persist_turn: None,
        tool_journal: None,
        domain_tools: None,
        code_orchestration: true,
        auxiliary_usage: Arc::default(),
        usage_reporter: None,
        on_event: Arc::new(|_| {}),
        stream_override: None,
    }
}

pub(super) struct AutoAllow;
impl crate::tools::auto_approve::AutoApprove for AutoAllow {
    fn try_auto_allow(&self, _command: &str) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn qualified_name_embeds_implementation_hash() {
    let first = qualified_name("greet", "return 'a';").unwrap();
    let second = qualified_name("greet", "return 'b';").unwrap();
    assert!(first.starts_with("dyn__greet_"), "{first}");
    assert_ne!(first, second, "同 name 改实现必须换新名（重定义无歧义）");
    assert_eq!(qualified_name("greet", "return 'a';").unwrap(), first, "同实现同名（幂等）");
    assert!(qualified_name("bad name", "x").is_err());
    assert!(qualified_name("", "x").is_err());
    assert!(qualified_name(&"a".repeat(NAME_MAX), "x").is_err(), "总长超 64 字节拒绝");
}

#[test]
fn validate_def_rejects_tampered_content() {
    let mut tampered = def("greet", "return 'a';");
    tampered.implementation = "return 'evil';".into();
    assert!(validate_def(&tampered).unwrap_err().contains("hash mismatch"));
    let mut renamed = def("greet", "return 'a';");
    renamed.name = "dyn__other_00000000".into();
    assert!(validate_def(&renamed).is_err());
    let mut bad_params = def("greet", "return 'a';");
    bad_params.parameters = json!([]);
    assert!(validate_def(&bad_params).is_err());
}

#[test]
fn visible_defs_follow_identity() {
    let extras = fresh_extras();
    let tool = def("greet", "return 'a';");
    crate::core::shared::lock(&extras.dynamic_tools).insert(tool.name.clone(), tool.clone());
    // Full 身份：全量可见
    assert_eq!(visible_defs(Some(&extras), None).len(), 1);
    // 族名放行（DCP dynamic-tools）
    let family = vec![FAMILY.to_string()];
    assert_eq!(visible_defs(Some(&extras), Some(&family)).len(), 1);
    // 精确限定名白名单（restricted 自定义）
    let exact = vec![tool.name.clone()];
    assert_eq!(visible_defs(Some(&extras), Some(&exact)).len(), 1);
    // 其余 restricted：不可见
    let readonly = vec!["read".to_string()];
    assert!(visible_defs(Some(&extras), Some(&readonly)).is_empty());
    assert!(visible_defs(None, None).is_empty());
    // 定义带 [dynamic] 标注（同构 [mcp:server] 前缀风格）
    assert!(visible_defs(Some(&extras), None)[0].function.description.starts_with("[dynamic] "));
}

#[test]
fn snapshot_roundtrip_restores_registry() {
    let tool = def("greet", "return 'hi';");
    let message = crate::core::session::new_message("ses", crate::core::session::Role::Assistant, vec![snapshot_part(&tool)]);
    // fork 语义：同一份历史进新 extras，定义不丢
    let forked = fresh_extras();
    assert_eq!(restore_from_history(&forked, std::slice::from_ref(&message)), 1);
    assert!(crate::core::shared::lock(&forked.dynamic_tools).contains_key(&tool.name));
    // 幂等：重复恢复不重复计数
    assert_eq!(restore_from_history(&forked, &[message]), 0);
}

#[test]
fn tampered_snapshot_is_not_restored() {
    let mut tool = def("greet", "return 'hi';");
    tool.implementation = "return 'evil';".into();
    let json = serde_json::to_string(&tool).unwrap();
    let message = crate::core::session::new_message(
        "ses",
        crate::core::session::Role::Assistant,
        vec![crate::core::session::Part::Context { text: format!("[kxen:dynamic-tool] {json}").into() }],
    );
    let extras = fresh_extras();
    assert_eq!(restore_from_history(&extras, &[message]), 0, "hash 不符的快照不得进注册表");
}

#[test]
fn validate_args_enforces_required_and_declared_types() {
    let tool = def("greet", "return 'a';");
    assert!(validate_args(&tool.parameters, &json!({ "x": "ok" })).is_ok());
    assert!(validate_args(&tool.parameters, &json!({})).unwrap_err().contains("missing required argument: x"));
    assert!(validate_args(&tool.parameters, &json!({ "x": 1 })).unwrap_err().contains("must be of type string"));
    assert!(validate_args(&tool.parameters, &json!({ "x": "ok", "extra": true })).is_ok(), "未声明属性放行");
    assert!(validate_args(&json!({ "type": "object" }), &json!({ "anything": 1 })).is_ok());
}

#[tokio::test]
async fn define_fails_closed_without_approval_channel() {
    let extras = Arc::new(fresh_extras());
    let ctx = ctx_with(&extras, None);
    let error = define(&json!({ "name": "greet", "description": "d", "implementation": "return 'a';" }), &ctx).await.unwrap_err();
    assert!(error.contains("no approval channel"), "{error}");
    assert!(crate::core::shared::lock(&extras.dynamic_tools).is_empty(), "未审批不得注册");
}

#[tokio::test]
async fn define_with_approval_registers_and_persists_snapshot() {
    let root = std::env::temp_dir().join(format!("kxen-dyn-define-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let broker = Arc::new(crate::agent::approval::ApprovalBroker::new().with_sessions_dir(root.clone()));
    let session = crate::core::session::create(&root, "/tmp").unwrap();
    let extras = Arc::new(fresh_extras());
    let mut ctx = ctx_with(&extras, Some(broker));
    ctx.session_id = Some(session.id.clone());
    ctx.kanban_auto = Some(Arc::new(AutoAllow));
    let args = json!({ "name": "greet", "description": "打招呼", "implementation": "return 'hi ' + args.x;" });
    let out = define(&args, &ctx).await.unwrap();
    let name = qualified_name("greet", "return 'hi ' + args.x;").unwrap();
    assert!(out.contains(&name), "{out}");
    assert!(crate::core::shared::lock(&extras.dynamic_tools).contains_key(&name));
    // 快照落盘 -> 模拟 resume：新 extras 从历史重建
    let history = crate::core::session::load_history_checked(&root, &session.id).unwrap();
    let resumed = fresh_extras();
    assert_eq!(restore_from_history(&resumed, &history), 1);
    // 幂等：重复定义同实现不再审批直接回执
    let again = define(&args, &ctx).await.unwrap();
    assert!(again.contains("already defined"), "{again}");
    // 同会话调用 dyn__*：沙箱执行（args 注入 + 顶层 return）
    let result = execute_defined(&name, &json!({ "x": "there" }), &ctx).await.unwrap();
    assert!(result.contains("hi there"), "{result}");
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn dynamic_tool_cannot_define_or_recurse_inside_sandbox() {
    let extras = Arc::new(fresh_extras());
    let broker_dir = std::env::temp_dir().join(format!("kxen-dyn-guard-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&broker_dir).unwrap();
    let broker = Arc::new(crate::agent::approval::ApprovalBroker::new().with_sessions_dir(broker_dir.clone()));
    let session = crate::core::session::create(&broker_dir, "/tmp").unwrap();
    let mut ctx = ctx_with(&extras, Some(broker));
    ctx.session_id = Some(session.id.clone());
    ctx.kanban_auto = Some(Arc::new(AutoAllow));
    // 沙箱内 tool("tool_define") 拒绝（与 C 的递归拒绝口径一致）
    let args = json!({ "name": "sneaky", "description": "d", "implementation": "return await tool('tool_define', {});" });
    let name = qualified_name("sneaky", "return await tool('tool_define', {});").unwrap();
    define(&args, &ctx).await.unwrap();
    let error = execute_defined(&name, &json!({}), &ctx).await.unwrap_err();
    assert!(error.contains("not allowed"), "{error}");
    // 沙箱内 tool("tool_undefine") 同规拒绝（卸载动作的审批边界在宿主侧）
    let args = json!({ "name": "sneaky2", "description": "d", "implementation": "return await tool('tool_undefine', {});" });
    let name = qualified_name("sneaky2", "return await tool('tool_undefine', {});").unwrap();
    define(&args, &ctx).await.unwrap();
    let error = execute_defined(&name, &json!({}), &ctx).await.unwrap_err();
    assert!(error.contains("not allowed"), "{error}");
    // 沙箱内 tool("workflow") 拒绝
    let args = json!({ "name": "recurse", "description": "d", "implementation": "return await tool('workflow', {});" });
    let name = qualified_name("recurse", "return await tool('workflow', {});").unwrap();
    define(&args, &ctx).await.unwrap();
    let error = execute_defined(&name, &json!({}), &ctx).await.unwrap_err();
    assert!(error.contains("not allowed"), "{error}");
    std::fs::remove_dir_all(broker_dir).ok();
}

#[tokio::test]
async fn dcp_mode_proposes_and_activates_without_current_run_effect() {
    let root = std::env::temp_dir().join(format!("kxen-dyn-macro-{}", uuid::Uuid::new_v4()));
    let macro_dir = root.join("dynamic-tools");
    let extras = Arc::new(fresh_extras());
    *crate::core::shared::lock(&extras.dynamic_macro_dir) = Some(macro_dir.clone());
    let broker_dir = root.join("sessions");
    std::fs::create_dir_all(&broker_dir).unwrap();
    let broker = Arc::new(crate::agent::approval::ApprovalBroker::new().with_sessions_dir(broker_dir));
    let mut ctx = ctx_with(&extras, Some(broker));
    ctx.kanban_auto = Some(Arc::new(AutoAllow));
    let args = json!({ "name": "greet", "description": "d", "implementation": "return 'hi';" });
    let out = define(&args, &ctx).await.unwrap();
    let name = qualified_name("greet", "return 'hi';").unwrap();
    assert!(out.contains("new sessions") || out.contains("NEW sessions"), "{out}");
    // 当前 run 不生效：注册表为空
    assert!(crate::core::shared::lock(&extras.dynamic_tools).is_empty());
    // 提案与激活件都落盘
    assert!(macro_dir.join("proposals").join(format!("{name}.json")).exists());
    assert!(macro_dir.join(format!("{name}.json")).exists());
    // 宏目录加载进新注册表（下会话生效）
    let next = fresh_extras();
    assert_eq!(macros::load_into_extras(&macro_dir, &next).unwrap(), 1);
    assert!(crate::core::shared::lock(&next.dynamic_tools).contains_key(&name));
    assert_eq!(crate::core::shared::lock(&next.dynamic_macro_dir).as_deref(), Some(macro_dir.as_path()));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn macro_load_fails_closed_on_hash_mismatch() {
    let root = std::env::temp_dir().join(format!("kxen-dyn-tamper-{}", uuid::Uuid::new_v4()));
    let tool = def("greet", "return 'hi';");
    macros::activate(&root, &tool).unwrap();
    // 篡改实现内容：hash 不符，整目录不可用
    let mut tampered = tool.clone();
    tampered.implementation = "return 'evil';".into();
    let path = root.join(format!("{}.json", tool.name));
    std::fs::write(&path, serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();
    let error = macros::load_active(&root).unwrap_err();
    assert!(error.contains("hash mismatch"), "{error}");
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn verify_history_references_fails_closed_on_missing_macro() {
    let extras = fresh_extras();
    let message = crate::core::session::new_message(
        "ses",
        crate::core::session::Role::Assistant,
        vec![crate::core::session::Part::ToolCall {
            name: "dyn__gone_01234567".into(),
            input: json!({}),
            output: "ok".into(),
            args: None,
            id: None,
            started_at: None,
            finished_at: None,
        }],
    );
    let error = super::dispatch::verify_history_references(&[message], &extras).unwrap_err();
    assert!(error.contains("dyn__gone_01234567"), "{error}");
    // 注册表覆盖后不报错
    let tool = def("gone", "return 'x';");
    let name = tool.name.clone();
    crate::core::shared::lock(&extras.dynamic_tools).insert(name.clone(), tool);
    let message = crate::core::session::new_message(
        "ses",
        crate::core::session::Role::Assistant,
        vec![crate::core::session::Part::ToolCall {
            name,
            input: json!({}),
            output: "ok".into(),
            args: None,
            id: None,
            started_at: None,
            finished_at: None,
        }],
    );
    super::dispatch::verify_history_references(&[message], &extras).unwrap();
}
