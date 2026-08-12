use super::*;

#[test]
fn approval_and_input_are_explicit_interruptions() {
    let repo = repository("interruptions");
    let run_id = id("brun_interruptions");
    let running = write(&repo, &run_id, queued(&repo, &run_id).event_version, "idem_start", RunCommand::Start { at_ms: 20 });
    let operation_id = id("op_one");
    let prepared = write(
        &repo,
        &run_id,
        running.event_version,
        "idem_approval_prepare",
        RunCommand::PrepareTool {
            operation_id: operation_id.clone(),
            generation: id("gen_approval"),
            intent: ToolIntent { call_id: id("call_approval"), capability_id: id("write"), arguments_json: "{}".into() },
            at_ms: 25,
        },
    );
    let waiting = write(
        &repo,
        &run_id,
        prepared.event_version,
        "idem_approval",
        RunCommand::RequestApproval {
            request: ApprovalRequest {
                approval_id: id("approval_one"),
                operation_id: Some(operation_id.clone()),
                summary: "write output".into(),
            },
            at_ms: 30,
        },
    );
    assert_eq!(waiting.status, RunStatus::ApprovalRequired);
    let denied = write(
        &repo,
        &run_id,
        waiting.event_version,
        "idem_deny",
        RunCommand::ResolveApproval { approval_id: id("approval_one"), decision: ApprovalDecision::Denied, at_ms: 40 },
    );
    assert_eq!(denied.status, RunStatus::Rejected);
    assert_eq!(
        denied.tool_operations[&operation_id].attempt.as_ref().unwrap().phase,
        crate::core::operation::AttemptPhase::CanceledBeforeStart
    );
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn input_pause_settles_current_tool_and_replays_bound_input() {
    let repo = repository("input-pause");
    let run_id = id("brun_input_pause");
    let running = write(&repo, &run_id, queued(&repo, &run_id).event_version, "idem_start", RunCommand::Start { at_ms: 20 });
    let operation_id = id("op_need_input");
    let generation = id("gen_need_input");
    let prepared = write(
        &repo,
        &run_id,
        running.event_version,
        "idem_prepare_input",
        RunCommand::PrepareTool {
            operation_id: operation_id.clone(),
            generation: generation.clone(),
            intent: ToolIntent { call_id: id("call_need_input"), capability_id: id("bot_task"), arguments_json: "{}".into() },
            at_ms: 30,
        },
    );
    let started = write(
        &repo,
        &run_id,
        prepared.event_version,
        "idem_start_input_tool",
        RunCommand::MarkToolStarted { operation_id: operation_id.clone(), generation: generation.clone(), at_ms: 31 },
    );
    let paused = write(
        &repo,
        &run_id,
        started.event_version,
        "idem_require_input",
        RunCommand::RequireInput { request: InputRequest { request_id: id("input_one"), prompt: "Which region?".into() }, at_ms: 32 },
    );
    let outcome = write(
        &repo,
        &run_id,
        paused.event_version,
        "idem_input_tool_outcome",
        RunCommand::RecordToolOutcome {
            operation_id: operation_id.clone(),
            generation: generation.clone(),
            outcome: OperationOutcome::Succeeded { value: ToolExecutionResult { output: "input requested".into(), is_error: false } },
            evidence: Vec::new(),
            at_ms: 33,
        },
    );
    let settled = write(
        &repo,
        &run_id,
        outcome.event_version,
        "idem_input_tool_settle",
        RunCommand::SettleTool { operation_id, generation, at_ms: 34 },
    );
    assert_eq!(settled.status, RunStatus::InputRequired);
    let bound = write(
        &repo,
        &run_id,
        settled.event_version,
        "idem_bind_input",
        RunCommand::BindInput {
            request_id: id("input_one"),
            parts: vec![ProviderNeutralPart::Text { text: "Asia/Dubai".into() }],
            at_ms: 35,
        },
    );
    assert_eq!(bound.status, RunStatus::Running);
    assert_eq!(bound.bound_inputs, [ProviderNeutralPart::Text { text: "Asia/Dubai".into() }]);
    std::fs::remove_dir_all(repo.root()).ok();
}
