//! tool_undefine 卸载路径单测：审批边界、事件流重放、再注册 hash 行为、DCP 提案模式拒绝。
//! 脚手架（def/ctx_with/AutoAllow）复用 tests.rs（350 行门禁拆分）。

use super::tests::{AutoAllow, ctx_with, def, fresh_extras};
use super::*;
use serde_json::json;
use std::sync::Arc;

#[test]
fn restore_replays_removal_events_in_order() {
    let tool = def("greet", "return 'hi';");
    let define_msg =
        |tool: &DynamicToolDef| crate::core::session::new_message("ses", crate::core::session::Role::Assistant, vec![snapshot_part(tool)]);
    let remove_msg = |text: String| {
        crate::core::session::new_message(
            "ses",
            crate::core::session::Role::Assistant,
            vec![crate::core::session::Part::Context { text: text.into() }],
        )
    };
    // 定义 -> 卸载：resume 后注册表为空
    let extras = fresh_extras();
    assert_eq!(restore_from_history(&extras, &[define_msg(&tool), remove_msg(format!("[kxen:dynamic-tool-remove] {}", tool.name))]), 1);
    assert!(crate::core::shared::lock(&extras.dynamic_tools).is_empty(), "卸载事件必须随事件流重放");
    // 定义 -> 卸载 -> 再定义（同实现同限定名）：恢复为已注册
    let extras = fresh_extras();
    let history = [define_msg(&tool), remove_msg(format!("[kxen:dynamic-tool-remove] {}", tool.name)), define_msg(&tool)];
    restore_from_history(&extras, &history);
    assert!(crate::core::shared::lock(&extras.dynamic_tools).contains_key(&tool.name));
    // 损坏的卸载事件（非限定名）不产生效果：宁可不卸也不错卸
    let extras = fresh_extras();
    restore_from_history(&extras, &[define_msg(&tool), remove_msg("[kxen:dynamic-tool-remove] exec".into())]);
    assert!(crate::core::shared::lock(&extras.dynamic_tools).contains_key(&tool.name));
}

#[tokio::test]
async fn undefine_requires_registered_tool_and_approval_channel() {
    let extras = Arc::new(fresh_extras());
    let ctx = ctx_with(&extras, None);
    // 未注册的名字：直接拒绝（不打扰审批）
    let error = undefine(&json!({ "name": "dyn__none_01234567" }), &ctx).await.unwrap_err();
    assert!(error.contains("unknown dynamic tool"), "{error}");
    // 非限定名拒绝
    let error = undefine(&json!({ "name": "exec" }), &ctx).await.unwrap_err();
    assert!(error.contains("qualified dynamic tool name"), "{error}");
    // 已注册但无审批通道：fail-closed，注册表不变
    let tool = def("greet", "return 'a';");
    crate::core::shared::lock(&extras.dynamic_tools).insert(tool.name.clone(), tool.clone());
    let error = undefine(&json!({ "name": tool.name }), &ctx).await.unwrap_err();
    assert!(error.contains("no approval channel"), "{error}");
    assert!(crate::core::shared::lock(&extras.dynamic_tools).contains_key(&tool.name), "未审批不得卸载");
}

#[tokio::test]
async fn undefine_removes_and_survives_resume_and_redefine() {
    let root = std::env::temp_dir().join(format!("kxen-dyn-undefine-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let broker = Arc::new(crate::agent::approval::ApprovalBroker::new().with_sessions_dir(root.clone()));
    let session = crate::core::session::create(&root, "/tmp").unwrap();
    let extras = Arc::new(fresh_extras());
    let mut ctx = ctx_with(&extras, Some(broker));
    ctx.session_id = Some(session.id.clone());
    ctx.kanban_auto = Some(Arc::new(AutoAllow));
    // 注册 greet（实现 A）-> 可调用
    let args_a = json!({ "name": "greet", "description": "d", "implementation": "return 'a';" });
    define(&args_a, &ctx).await.unwrap();
    let name_a = qualified_name("greet", "return 'a';").unwrap();
    assert!(execute_defined(&name_a, &json!({}), &ctx).await.is_ok());
    // 卸载：注册表摘除，新调用 fail-closed，展示侧同步消失
    let out = undefine(&json!({ "name": name_a }), &ctx).await.unwrap();
    assert!(out.contains("unregistered"), "{out}");
    assert!(crate::core::shared::lock(&extras.dynamic_tools).is_empty());
    assert!(visible_defs(Some(&extras), None).is_empty());
    let error = execute_defined(&name_a, &json!({}), &ctx).await.unwrap_err();
    assert!(error.contains("unknown dynamic tool"), "{error}");
    // resume：新 extras 从历史重放（定义 + 卸载）-> 仍为空
    let history = crate::core::session::load_history_checked(&root, &session.id).unwrap();
    let resumed = fresh_extras();
    restore_from_history(&resumed, &history);
    assert!(crate::core::shared::lock(&resumed.dynamic_tools).is_empty(), "卸载事件必须随事件流重放");
    // 同名不同实现 -> 新限定名（hash 进名字）
    let args_b = json!({ "name": "greet", "description": "d", "implementation": "return 'b';" });
    define(&args_b, &ctx).await.unwrap();
    let name_b = qualified_name("greet", "return 'b';").unwrap();
    assert_ne!(name_a, name_b);
    // 卸载后同实现可再注册（不再命中 already defined 短路）
    let out = define(&args_a, &ctx).await.unwrap();
    assert!(out.contains("registered"), "{out}");
    let registry = crate::core::shared::lock(&extras.dynamic_tools);
    assert!(registry.contains_key(&name_a) && registry.contains_key(&name_b));
    drop(registry);
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn undefine_fails_closed_in_dcp_proposal_mode() {
    let extras = Arc::new(fresh_extras());
    *crate::core::shared::lock(&extras.dynamic_macro_dir) = Some(std::path::PathBuf::from("/tmp/kxen-dyn-macros"));
    let tool = def("greet", "return 'a';");
    crate::core::shared::lock(&extras.dynamic_tools).insert(tool.name.clone(), tool.clone());
    let ctx = ctx_with(&extras, None);
    let error = undefine(&json!({ "name": tool.name }), &ctx).await.unwrap_err();
    assert!(error.contains("macro directory"), "{error}");
    assert!(crate::core::shared::lock(&extras.dynamic_tools).contains_key(&tool.name), "提案模式下不得摘除");
}
