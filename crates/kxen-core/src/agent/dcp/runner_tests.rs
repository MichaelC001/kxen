use std::sync::Arc;

use super::runner_support::{is_sensitive_child_env, validate_agent_output};
use super::*;

#[test]
fn json_output_contract_requires_an_object_and_all_fields() {
    let output = DcpAgentOutput { format: DcpAgentOutputFormat::Json, required_fields: vec!["summary".into(), "checks".into()] };
    assert!(validate_agent_output(&output, r#"{"summary":"ok","checks":[]}"#).is_ok());
    assert!(validate_agent_output(&output, r#"{"summary":"ok"}"#).unwrap_err().contains("checks"));
    assert!(validate_agent_output(&output, "[]").is_err());
}

#[test]
fn sensitive_environment_names_are_filtered_by_default() {
    assert!(is_sensitive_child_env("GH_TOKEN"));
    assert!(is_sensitive_child_env("AWS_SHARED_CREDENTIALS_FILE"));
    assert!(!is_sensitive_child_env("CI"));
    assert!(!is_sensitive_child_env("SSH_AUTH_SOCK"));
}

#[tokio::test]
async fn predefined_agent_runs_and_resumes_multiple_durable_tasks() {
    let root = std::env::temp_dir().join(format!("kxen-dcp-runtime-{}", uuid::Uuid::new_v4()));
    let workspace = root.join("workspace");
    let state = root.join("state");
    std::fs::create_dir_all(&workspace).unwrap();
    let config_file = root.join("config.toml");
    std::fs::write(&config_file, "[roles.execution]\nprovider = \"ollama\"\nmodel = \"test\"\n").unwrap();
    let agent_file = root.join("agent.yaml");
    let definition = DcpAgentDefinition {
        api_version: DCP_AGENT_API_VERSION.into(),
        kind: "DCPAgent".into(),
        metadata: DcpAgentMetadata { name: "runner_test".into(), description: None },
        spec: DcpAgentSpec {
            objective: "Return a deterministic result".into(),
            instructions: vec!["Complete the task".into()],
            success_criteria: vec!["A result is returned".into()],
            capabilities: DcpAgentCapabilities { required: vec!["read".into()], optional: Vec::new() },
            execution: DcpAgentExecution::default(),
            output: DcpAgentOutput::default(),
        },
    };
    std::fs::write(&agent_file, definition.to_yaml().unwrap()).unwrap();
    let stream: crate::llm::StreamFn =
        Arc::new(|_, _, _, _| Box::pin(futures::stream::iter(vec![crate::llm::Delta::Text("completed".into()), crate::llm::Delta::Done])));
    let runtime = DcpRuntime::new(
        DcpRuntimeOptions {
            data_dir: state,
            config_file,
            auth_file: root.join("auth.json"),
            policy_file: None,
            event_format: DcpEventFormat::Jsonl,
            allow_shell: false,
            allow_mcp: false,
            pass_env: Vec::new(),
        },
        Arc::new(|_| {}),
    )
    .unwrap()
    .with_stream_override(stream);
    let first = runtime
        .run(DcpRunRequest {
            session_id: None,
            task: Some("first".into()),
            agent_file: Some(agent_file),
            workspace: Some(workspace),
            rebind_workspace: false,
            cancel: None,
        })
        .await
        .unwrap();
    assert_eq!(first.status, DcpRunStatus::Completed, "error={:?} text={}", first.error, first.final_text);
    assert_eq!(first.final_text, "completed");
    let mut interrupted_settlement = runtime.store.load_run(&first.session_id, &first.run_id).unwrap();
    interrupted_settlement.settled = false;
    runtime.store.save_run(&interrupted_settlement).unwrap();
    let recovered = runtime
        .run(DcpRunRequest {
            session_id: Some(first.session_id.clone()),
            task: None,
            agent_file: None,
            workspace: None,
            rebind_workspace: false,
            cancel: None,
        })
        .await
        .unwrap();
    assert_eq!(recovered.status, DcpRunStatus::Completed);
    assert!(runtime.store.load_run(&first.session_id, &first.run_id).unwrap().settled);
    let second = runtime
        .run(DcpRunRequest {
            session_id: Some(first.session_id.clone()),
            task: Some("second".into()),
            agent_file: None,
            workspace: None,
            rebind_workspace: false,
            cancel: None,
        })
        .await
        .unwrap();
    assert_eq!(second.status, DcpRunStatus::Completed);
    assert_eq!(runtime.store.load_session(&first.session_id).unwrap().run_ids.len(), 2);
    assert!(
        runtime
            .run(DcpRunRequest {
                session_id: Some(first.session_id),
                task: None,
                agent_file: None,
                workspace: None,
                rebind_workspace: false,
                cancel: None,
            })
            .await
            .unwrap_err()
            .contains("NO_PENDING_WORK")
    );
    std::fs::remove_dir_all(root).ok();
}
