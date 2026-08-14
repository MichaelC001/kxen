use std::sync::Arc;

use super::runner_support::{
    DcpAutoApprove, ensure_private_dir, filtered_child_environment, is_sensitive_child_env, load_runtime_policy, same_message_content,
    validate_agent_output, workspace_scope,
};
use super::*;
use crate::tools::auto_approve::AutoApprove;

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
    assert!(is_sensitive_child_env("AWS_CONFIG_FILE"));
    assert!(is_sensitive_child_env("CLOUDSDK_CONFIG"));
    assert!(is_sensitive_child_env("GNUPGHOME"));
    assert!(is_sensitive_child_env("SSH_AUTH_SOCK"));
    assert!(!is_sensitive_child_env("CI"));
    assert!(!is_sensitive_child_env("GPG_TTY"));
}

#[test]
fn runtime_helpers_apply_policy_scope_and_durable_audit() {
    let root = std::env::temp_dir().join(format!("kxen-dcp-helpers-{}", uuid::Uuid::new_v4()));
    ensure_private_dir(&root).unwrap();
    let policy_file = root.join("policy.json");
    std::fs::write(
        &policy_file,
        r#"{"allowedCapabilities":["read","write","exec"],"deniedCapabilities":["write"],"passEnv":["CI"],"maxTurns":7}"#,
    )
    .unwrap();
    let options = DcpRuntimeOptions {
        data_dir: root.join("state"),
        config_file: root.join("config.toml"),
        auth_file: root.join("auth.json"),
        policy_file: Some(policy_file.clone()),
        event_format: DcpEventFormat::Text,
        allow_shell: true,
        allow_mcp: true,
        pass_env: vec!["SSH_AUTH_SOCK".into(), "CI".into()],
    };
    let policy = load_runtime_policy(&options).unwrap();
    assert!(policy.allow_shell);
    assert!(policy.allow_mcp);
    assert_eq!(policy.pass_env, ["CI", "SSH_AUTH_SOCK"]);

    let tool_home = root.join("tool-home");
    let environment = filtered_child_environment(&policy, &tool_home).unwrap();
    assert_eq!(environment.get(std::ffi::OsStr::new("HOME")).map(|value| value.as_os_str()), Some(tool_home.as_os_str()));
    assert_eq!(
        environment.get(std::ffi::OsStr::new("XDG_STATE_HOME")).map(|value| value.as_os_str()),
        Some(tool_home.join("state").as_os_str())
    );

    let read_scope = workspace_scope(&root, &["read".into()]);
    assert_eq!(read_scope.read.as_slice(), std::slice::from_ref(&root));
    assert!(read_scope.write.is_empty());
    assert!(read_scope.execute.is_empty());
    let execute_scope = workspace_scope(&root, &["exec".into()]);
    assert_eq!(execute_scope.read.as_slice(), std::slice::from_ref(&root));
    assert_eq!(execute_scope.write.as_slice(), std::slice::from_ref(&root));
    assert_eq!(execute_scope.execute.as_slice(), std::slice::from_ref(&root));
    let empty_scope = workspace_scope(&root, &[]);
    assert!(empty_scope.read.is_empty());

    let first = crate::core::session::new_message("ses_helpers", crate::core::session::Role::User, vec![]);
    let mut second = first.clone();
    second.id = "msg_other".into();
    second.created_at = second.created_at.saturating_add(1);
    assert!(same_message_content(&first, &second).unwrap());
    second.role = crate::core::session::Role::Assistant;
    assert!(!same_message_content(&first, &second).unwrap());

    let audit = root.join("approval.jsonl");
    DcpAutoApprove::new(audit.clone(), "shell").try_auto_allow("cargo test").unwrap();
    let entry: serde_json::Value = serde_json::from_str(std::fs::read_to_string(&audit).unwrap().trim()).unwrap();
    assert_eq!(entry["schemaVersion"], 1);
    assert_eq!(entry["category"], "shell");
    assert!(entry["commandHash"].as_str().is_some_and(|value| !value.is_empty()));

    std::fs::remove_file(audit).unwrap();
    std::fs::remove_file(policy_file).unwrap();
    std::fs::remove_dir(root).unwrap();
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

#[test]
fn unknown_tool_outcome_emits_input_required_without_terminal_settlement() {
    let root = std::env::temp_dir().join(format!("kxen-dcp-input-required-{}", uuid::Uuid::new_v4()));
    let workspace = root.join("workspace");
    let state = root.join("state");
    std::fs::create_dir_all(&workspace).unwrap();
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = events.clone();
    let runtime = DcpRuntime::new(
        DcpRuntimeOptions {
            data_dir: state,
            config_file: root.join("config.toml"),
            auth_file: root.join("auth.json"),
            policy_file: None,
            event_format: DcpEventFormat::Jsonl,
            allow_shell: false,
            allow_mcp: false,
            pass_env: Vec::new(),
        },
        Arc::new(move |event| crate::core::shared::lock(&captured).push(event)),
    )
    .unwrap();
    let definition = DcpAgentDefinition {
        api_version: DCP_AGENT_API_VERSION.into(),
        kind: "DCPAgent".into(),
        metadata: DcpAgentMetadata { name: "unknown_test".into(), description: None },
        spec: DcpAgentSpec {
            objective: "Verify UNKNOWN recovery".into(),
            instructions: vec!["Stop for owner input".into()],
            success_criteria: Vec::new(),
            capabilities: DcpAgentCapabilities { required: vec!["read".into()], optional: Vec::new() },
            execution: DcpAgentExecution::default(),
            output: DcpAgentOutput::default(),
        },
    };
    let lock = DcpRuntimePolicy::default().resolve_lock(definition, &DcpRuntime::base_capabilities()).unwrap();
    let mut session = runtime.store().create_session(WorkspaceBinding::capture(&workspace).unwrap(), lock).unwrap();
    let run = runtime.store().create_run(&mut session, "inspect".into()).unwrap();
    let result = runtime
        .finish_run(
            &mut session,
            run,
            crate::agent::agent_loop::AgentOutcome {
                final_text: String::new(),
                turns: 1,
                aborted: false,
                stats: None,
                terminal: crate::agent::agent_loop::AgentEvent::Aborted,
                provider_model: None,
            },
            crate::llm::ModelRef::new("test", "model"),
            vec!["op_unknown".into()],
        )
        .unwrap();
    assert_eq!(result.status, DcpRunStatus::InputRequired);
    assert!(result.error.as_deref().is_some_and(|error| error.contains("op_unknown")));
    let stored = runtime.store().load_run(&result.session_id, &result.run_id).unwrap();
    assert_eq!(stored.status, DcpRunStatus::InputRequired);
    assert!(!stored.settled);
    assert!(
        crate::core::shared::lock(&events)
            .iter()
            .any(|event| { matches!(event, DcpRuntimeEvent::RunInputRequired { operation_ids, .. } if operation_ids == &["op_unknown"]) })
    );
    assert!(!crate::core::shared::lock(&events).iter().any(|event| matches!(event, DcpRuntimeEvent::RunFinished { .. })));
    std::fs::remove_dir_all(root).ok();
}
