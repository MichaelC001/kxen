//! DCP 动态工具验收：policy 开关（allowDynamicTools）、提案/宏目录落盘、新 session 生效、
//! 宏内容篡改 fail-closed。harness 同 runner_tests.rs（stream override + 临时目录）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::llm::tool::{ChunkFunction, ChunkToolCall};
use crate::llm::{Delta, StreamFn};

const IMPL: &str = "return 'echo:' + args.x;";

fn qualified() -> String {
    crate::agent::dynamic::qualified_name("dyn_echo", IMPL).unwrap()
}

fn definition(optional: Vec<String>) -> DcpAgentDefinition {
    DcpAgentDefinition {
        api_version: DCP_AGENT_API_VERSION.into(),
        kind: "DCPAgent".into(),
        metadata: DcpAgentMetadata { name: "dynamic_tool_test".into(), description: None },
        spec: DcpAgentSpec {
            objective: "Exercise dynamic tools".into(),
            instructions: vec!["Use the declared tools".into()],
            success_criteria: vec!["The run completes".into()],
            capabilities: DcpAgentCapabilities { required: vec!["read".into()], optional },
            execution: DcpAgentExecution::default(),
            output: DcpAgentOutput::default(),
        },
    }
}

fn scaffold(root: &Path, policy_json: Option<&str>, optional: Vec<String>) -> (PathBuf, PathBuf, DcpRuntimeOptions) {
    let workspace = root.join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let config_file = root.join("config.toml");
    std::fs::write(&config_file, "[roles.execution]\nprovider = \"ollama\"\nmodel = \"test\"\n").unwrap();
    let agent_file = root.join("agent.yaml");
    std::fs::write(&agent_file, definition(optional).to_yaml().unwrap()).unwrap();
    let policy_file = policy_json.map(|json| {
        let path = root.join("policy.json");
        std::fs::write(&path, json).unwrap();
        path
    });
    (
        workspace,
        agent_file,
        DcpRuntimeOptions {
            data_dir: root.join("state"),
            config_file,
            auth_file: root.join("auth.json"),
            consume_auth_file: false,
            policy_file,
            event_format: DcpEventFormat::Jsonl,
            allow_shell: false,
            allow_mcp: false,
            pass_env: Vec::new(),
        },
    )
}

fn tool_call(name: &str, arguments: String) -> Delta {
    Delta::ToolFragments(vec![ChunkToolCall {
        index: Some(0),
        id: Some("call_1".into()),
        function: Some(ChunkFunction { name: Some(name.into()), arguments: Some(arguments) }),
    }])
}

fn scripted(first: Delta, then: &'static str) -> StreamFn {
    let calls = Arc::new(AtomicUsize::new(0));
    Arc::new(move |_, _, _, _| {
        let first = first.clone();
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Box::pin(futures::stream::iter(vec![first, Delta::Done]))
        } else {
            Box::pin(futures::stream::iter(vec![Delta::Text(then.into()), Delta::Done]))
        }
    })
}

async fn run(runtime: &DcpRuntime, workspace: &Path, agent_file: &Path) -> DcpRunResult {
    runtime
        .run(DcpRunRequest {
            session_id: None,
            task: Some("go".into()),
            agent_file: Some(agent_file.to_path_buf()),
            workspace: Some(workspace.to_path_buf()),
            rebind_workspace: false,
            cancel: None,
        })
        .await
        .unwrap()
}

fn history_outputs(runtime: &DcpRuntime, session_id: &str) -> Vec<String> {
    crate::core::session::load_history_checked(runtime.store().sessions_dir(), session_id)
        .unwrap()
        .iter()
        .flat_map(|message| &message.parts)
        .filter_map(|part| match part {
            crate::core::session::Part::ToolCall { name, output, .. } => Some(format!("{name}: {output}")),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn define_proposes_and_activates_for_new_sessions() {
    let root = std::env::temp_dir().join(format!("kxen-dcp-dyn-{}", uuid::Uuid::new_v4()));
    let (workspace, agent_file, options) = scaffold(&root, Some(r#"{"allowDynamicTools":true}"#), vec!["dynamic-tools".into()]);
    let macro_dir = root.join("dynamic-tools");
    let name = qualified();

    let define_args = serde_json::json!({"name": "dyn_echo", "description": "echo x", "implementation": IMPL}).to_string();
    let runtime = DcpRuntime::new(options, Arc::new(|_| {}))
        .unwrap()
        .with_stream_override(scripted(tool_call("tool_define", define_args), "defined"));
    let first = run(&runtime, &workspace, &agent_file).await;
    assert_eq!(first.status, DcpRunStatus::Completed, "error={:?}", first.error);
    // 提案留痕 + 审批后激活 + 自主授权审计落盘；当前 run 注册表不含该工具
    assert!(macro_dir.join("proposals").join(format!("{name}.json")).is_file());
    assert!(macro_dir.join(format!("{name}.json")).is_file());
    let run_dir = runtime.store().run_dir(&first.session_id, &first.run_id).unwrap();
    assert!(std::fs::read_to_string(run_dir.join("dynamic-tool-audit.jsonl")).unwrap().contains("dynamic_tool_define"));
    assert!(history_outputs(&runtime, &first.session_id).iter().any(|line| line.contains("NEW sessions")));
    assert!(!history_outputs(&runtime, &first.session_id).iter().any(|line| line.contains("echo:")));

    // 新 session（新 state 目录）同 policy 同 agent 定义：族进锁 -> mount 加载宏目录 -> dyn__ 可调用
    let mut options2 = scaffold(&root, Some(r#"{"allowDynamicTools":true}"#), vec!["dynamic-tools".into()]).2;
    options2.data_dir = root.join("state2");
    let runtime = DcpRuntime::new(options2, Arc::new(|_| {}))
        .unwrap()
        .with_stream_override(scripted(tool_call(&name, r#"{"x":"hi"}"#.into()), "used"));
    let second = run(&runtime, &workspace, &agent_file).await;
    assert_eq!(second.status, DcpRunStatus::Completed, "error={:?}", second.error);
    assert!(history_outputs(&runtime, &second.session_id).iter().any(|line| line.starts_with(&name) && line.contains("echo:hi")));
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn dynamic_tools_disabled_by_default() {
    let root = std::env::temp_dir().join(format!("kxen-dcp-dyn-off-{}", uuid::Uuid::new_v4()));
    let (workspace, agent_file, options) = scaffold(&root, None, vec!["dynamic-tools".into()]);
    let define_args = serde_json::json!({"name": "dyn_echo", "description": "echo x", "implementation": IMPL}).to_string();
    let runtime =
        DcpRuntime::new(options, Arc::new(|_| {})).unwrap().with_stream_override(scripted(tool_call("tool_define", define_args), "done"));
    let result = run(&runtime, &workspace, &agent_file).await;
    assert_eq!(result.status, DcpRunStatus::Completed, "error={:?}", result.error);
    assert!(history_outputs(&runtime, &result.session_id).iter().any(|line| line.contains("tool not allowed in this role: tool_define")));
    assert!(!root.join("dynamic-tools").exists());
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn tampered_macro_directory_fails_closed() {
    let root = std::env::temp_dir().join(format!("kxen-dcp-dyn-tamper-{}", uuid::Uuid::new_v4()));
    let (workspace, agent_file, options) = scaffold(&root, Some(r#"{"allowDynamicTools":true}"#), vec!["dynamic-tools".into()]);
    // 宏文件实现被改（hash 留在名字与字段里）：加载即拒，session 创建 fail-closed
    let name = qualified();
    let macro_dir = root.join("dynamic-tools");
    std::fs::create_dir_all(&macro_dir).unwrap();
    let tampered = serde_json::json!({
        "schemaVersion": 1,
        "name": name,
        "description": "echo x",
        "parameters": {"type": "object", "properties": {}},
        "implementation": "return 'pwned';",
        "implementationHash": crate::agent::dynamic::implementation_hash(IMPL).as_str(),
    });
    std::fs::write(macro_dir.join(format!("{name}.json")), serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();

    let runtime =
        DcpRuntime::new(options, Arc::new(|_| {})).unwrap().with_stream_override(scripted(Delta::Text("unused".into()), "unused"));
    let error = runtime
        .run(DcpRunRequest {
            session_id: None,
            task: Some("go".into()),
            agent_file: Some(agent_file),
            workspace: Some(workspace),
            rebind_workspace: false,
            cancel: None,
        })
        .await
        .unwrap_err();
    assert!(error.contains("hash mismatch"), "unexpected error: {error}");
    std::fs::remove_dir_all(root).ok();
}
