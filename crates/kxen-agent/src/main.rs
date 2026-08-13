mod args;
mod args_types;

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use kxen_core::agent::dcp::{
    DcpEventFormat, DcpEventSink, DcpRunRequest, DcpRunToolJournal, DcpRuntime, DcpRuntimeEvent, DcpRuntimeOptions,
};

fn main() -> ExitCode {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).with_writer(std::io::stderr).init();
    let command = match args::parse(std::env::args().skip(1)) {
        Ok(args::Parsed::Help) => {
            print!("{}", args::HELP);
            return ExitCode::SUCCESS;
        }
        Ok(args::Parsed::Version) => {
            println!("kxen-agent {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Ok(args::Parsed::Command(command)) => *command,
        Err(error) => {
            eprintln!("error: {error}\n\n{}", args::HELP);
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: create Tokio runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(run(command))
}

async fn run(command: args::Command) -> ExitCode {
    let output = command.output();
    let command = match command {
        args::Command::AgentValidate(command) => {
            return match validate_agent(command, output) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => fail(output, &error),
            };
        }
        command => command,
    };
    let sink = event_sink(output);
    let runtime = match DcpRuntime::new(
        DcpRuntimeOptions {
            data_dir: command.data_dir(),
            config_file: command.config_file(),
            auth_file: command.auth_file(),
            policy_file: command.policy_file(),
            event_format: output,
            allow_shell: command.allow_shell(),
            allow_mcp: command.allow_mcp(),
            pass_env: command.pass_env(),
        },
        sink,
    ) {
        Ok(runtime) => runtime,
        Err(error) => return fail(output, &error),
    };
    let result = match command {
        args::Command::Run(command) => run_agent(&runtime, command).await.map(|_| ()),
        args::Command::SessionList(_) => {
            let sessions = runtime.store().list_sessions();
            emit_value(output, &serde_json::json!({ "type": "session_list", "sessions": sessions }));
            Ok(())
        }
        args::Command::SessionShow(command) => show_session(&runtime, &command.session_id, output),
        args::Command::SessionFork(command) => fork_session(&runtime, command, output).await,
        args::Command::SessionExport(command) => export_session(&runtime, command, output),
        args::Command::SessionImport(command) => import_session(&runtime, command, output),
        args::Command::RunShow(command) => show_run(&runtime, command, output),
        args::Command::RunResolve(command) => resolve_run(&runtime, command, output),
        args::Command::AgentValidate(_) => unreachable!("handled before runtime initialization"),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(output, &error),
    }
}

fn validate_agent(command: args::AgentValidateCommand, output: DcpEventFormat) -> Result<(), String> {
    let text = std::fs::read_to_string(&command.file).map_err(|error| format!("read DCPAgent {}: {error}", command.file.display()))?;
    let definition = kxen_core::agent::dcp::DcpAgentDefinition::parse_yaml(&text)?;
    let hash = definition.content_hash()?;
    emit_value(output, &serde_json::json!({ "type": "agent_valid", "definitionHash": hash, "definition": definition }));
    Ok(())
}

fn show_run(runtime: &DcpRuntime, command: args::RunShowCommand, output: DcpEventFormat) -> Result<(), String> {
    let run = runtime.store().load_run(&command.session_id, &command.run_id)?;
    let journal = DcpRunToolJournal::open(&runtime.store().run_dir(&command.session_id, &command.run_id)?)?;
    emit_value(output, &serde_json::json!({ "type": "run", "run": run, "tools": journal.snapshot() }));
    Ok(())
}

async fn run_agent(runtime: &DcpRuntime, command: args::RunCommand) -> Result<(), String> {
    let task = read_task(command.task, command.task_file, command.stdin)?;
    let cancel = kxen_core::agent::cancel::CancelToken::new();
    let signal_cancel = cancel.clone();
    let signal = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal_cancel.cancel();
        }
    });
    let result = runtime
        .run(DcpRunRequest {
            session_id: command.resume,
            task,
            agent_file: command.agent,
            workspace: command.workspace,
            rebind_workspace: command.rebind_workspace,
            cancel: Some(cancel),
        })
        .await;
    signal.abort();
    let result = result?;
    if !matches!(result.status, kxen_core::agent::dcp::DcpRunStatus::Completed) {
        return Err(format!("DCPRun ended with status {:?}: {}", result.status, result.error.as_deref().unwrap_or(&result.final_text)));
    }
    Ok(())
}

fn show_session(runtime: &DcpRuntime, session_id: &str, output: DcpEventFormat) -> Result<(), String> {
    let dcp = runtime.store().load_session(session_id)?;
    let session = kxen_core::core::session::load_meta(runtime.store().sessions_dir(), session_id).map_err(|error| error.to_string())?;
    let runs = dcp.run_ids.iter().map(|run_id| runtime.store().load_run(session_id, run_id)).collect::<Result<Vec<_>, _>>()?;
    emit_value(output, &serde_json::json!({ "type": "session", "session": session, "dcp": dcp, "runs": runs }));
    Ok(())
}

async fn fork_session(runtime: &DcpRuntime, command: args::ForkCommand, output: DcpEventFormat) -> Result<(), String> {
    let position =
        if command.before { kxen_core::core::session::ForkPosition::Before } else { kxen_core::core::session::ForkPosition::After };
    let workspace = match command.worktree {
        Some(name) => {
            let source = runtime.store().load_session(&command.session_id)?;
            let info = kxen_core::tools::worktree::create(Path::new(&source.workspace.root), &name).await?;
            Some(kxen_core::agent::dcp::WorkspaceBinding::capture(&info.path)?)
        }
        None => None,
    };
    let fork = runtime.store().fork_session(
        &command.session_id,
        &command.message_id,
        position,
        kxen_core::core::session::ForkKind::Manual,
        workspace,
    )?;
    emit_value(output, &serde_json::json!({ "type": "session_forked", "session": fork }));
    Ok(())
}

fn export_session(runtime: &DcpRuntime, command: args::ExportCommand, output: DcpEventFormat) -> Result<(), String> {
    let bundle = runtime.store().export_bundle(&command.session_id)?;
    let bytes = serde_json::to_vec_pretty(&bundle).map_err(|error| error.to_string())?;
    write_private(&command.file, &bytes)?;
    emit_value(output, &serde_json::json!({ "type": "session_exported", "sessionId": command.session_id, "file": command.file }));
    Ok(())
}

fn import_session(runtime: &DcpRuntime, command: args::ImportCommand, output: DcpEventFormat) -> Result<(), String> {
    let bytes = std::fs::read(&command.file).map_err(|error| format!("read Session bundle {}: {error}", command.file.display()))?;
    let bundle = serde_json::from_slice(&bytes).map_err(|error| format!("parse Session bundle {}: {error}", command.file.display()))?;
    let session = runtime.store().import_bundle(bundle, &command.workspace)?;
    emit_value(output, &serde_json::json!({ "type": "session_imported", "session": session }));
    Ok(())
}

fn resolve_run(runtime: &DcpRuntime, command: args::ResolveCommand, output: DcpEventFormat) -> Result<(), String> {
    let run = runtime.store().load_run(&command.session_id, &command.run_id)?;
    let journal = DcpRunToolJournal::open(&runtime.store().run_dir(&command.session_id, &command.run_id)?)?;
    let operation = journal.resolve_unknown(&command.operation_id, &command.output, command.is_error)?;
    emit_value(output, &serde_json::json!({ "type": "run_operation_resolved", "runId": run.run_id, "operation": operation }));
    Ok(())
}

fn read_task(task: Option<String>, task_file: Option<PathBuf>, stdin: bool) -> Result<Option<String>, String> {
    let mut sources = usize::from(task.is_some()) + usize::from(task_file.is_some()) + usize::from(stdin);
    if sources == 0 {
        return Ok(None);
    }
    if sources > 1 {
        return Err("use exactly one of --task, --task-file or --stdin".into());
    }
    if let Some(task) = task {
        return Ok(Some(task));
    }
    if let Some(path) = task_file {
        return std::fs::read_to_string(&path).map(Some).map_err(|error| format!("read task file {}: {error}", path.display()));
    }
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).map_err(|error| format!("read stdin: {error}"))?;
    sources = input.trim().is_empty() as usize;
    if sources != 0 {
        return Err("stdin task is empty".into());
    }
    Ok(Some(input))
}

fn event_sink(format: DcpEventFormat) -> DcpEventSink {
    Arc::new(move |event| match format {
        DcpEventFormat::Jsonl => emit_value(format, &event),
        DcpEventFormat::Text => match event {
            DcpRuntimeEvent::Agent { event, .. } if event["kind"] == "text" => {
                if let Some(text) = event["text"].as_str() {
                    print!("{text}");
                }
            }
            DcpRuntimeEvent::RunFinished { result } => println!("\n{}", result.final_text),
            _ => {}
        },
    })
}

fn emit_value(format: DcpEventFormat, value: &impl serde::Serialize) {
    match serde_json::to_string(value) {
        Ok(json) => println!("{json}"),
        Err(error) if format == DcpEventFormat::Jsonl => println!("{{\"type\":\"error\",\"message\":{:?}}}", error.to_string()),
        Err(_) => {}
    }
}

fn fail(format: DcpEventFormat, error: &str) -> ExitCode {
    match format {
        DcpEventFormat::Jsonl => emit_value(format, &serde_json::json!({ "type": "error", "message": error })),
        DcpEventFormat::Text => eprintln!("error: {error}"),
    }
    ExitCode::FAILURE
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    kxen_core::core::durability::atomic_replace(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("secure {}: {error}", path.display()))?;
    }
    Ok(())
}
