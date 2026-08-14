use std::path::PathBuf;

use kxen_core::agent::dcp::DcpEventFormat;

pub use crate::args_types::*;

pub fn parse(args: impl Iterator<Item = String>) -> Result<Parsed, String> {
    let args = args.collect::<Vec<_>>();
    if args.iter().any(|argument| matches!(argument.as_str(), "-h" | "--help")) {
        return Ok(Parsed::Help);
    }
    if args.first().is_some_and(|argument| matches!(argument.as_str(), "-V" | "--version")) {
        return Ok(Parsed::Version);
    }
    if args.first().is_some_and(|argument| argument == "session") {
        return parse_session(&args[1..]).map(|command| Parsed::Command(Box::new(command)));
    }
    if args.first().is_some_and(|argument| argument == "agent") {
        return parse_agent(&args[1..]).map(|command| Parsed::Command(Box::new(command)));
    }
    if args.first().is_some_and(|argument| argument == "run") && args.get(1).is_some_and(|argument| argument == "resolve") {
        return parse_resolve(&args[2..]).map(|command| Parsed::Command(Box::new(command)));
    }
    if args.first().is_some_and(|argument| argument == "run") && args.get(1).is_some_and(|argument| argument == "show") {
        return parse_run_show(&args[2..]).map(|command| Parsed::Command(Box::new(command)));
    }
    let run_args = if args.first().is_some_and(|argument| argument == "run") { &args[1..] } else { &args[..] };
    parse_run(run_args).map(Command::Run).map(|command| Parsed::Command(Box::new(command)))
}

fn parse_agent(args: &[String]) -> Result<Command, String> {
    match args.first().map(String::as_str) {
        Some("validate") => {
            let file = args.get(1).map(PathBuf::from).ok_or("agent validate requires FILE")?;
            let mut common = Common::default();
            parse_common_tail(&args[2..], &mut common)?;
            Ok(Command::AgentValidate(AgentValidateCommand { common, file }))
        }
        Some(action) => Err(format!("unknown agent action: {action}")),
        None => Err("missing agent action".into()),
    }
}

fn parse_run_show(args: &[String]) -> Result<Command, String> {
    let session_id = args.first().cloned().ok_or("missing SESSION_ID")?;
    let run_id = args.get(1).cloned().ok_or("missing RUN_ID")?;
    let mut common = Common::default();
    parse_common_tail(&args[2..], &mut common)?;
    Ok(Command::RunShow(RunShowCommand { common, session_id, run_id }))
}

fn parse_run(args: &[String]) -> Result<RunCommand, String> {
    let mut common = Common::default();
    let mut task = None;
    let mut task_file = None;
    let mut stdin = false;
    let mut agent = None;
    let mut workspace = None;
    let mut resume = None;
    let mut rebind_workspace = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--task" => task = Some(value(args, &mut index, "--task")?),
            "--task-file" => task_file = Some(PathBuf::from(value(args, &mut index, "--task-file")?)),
            "--stdin" => stdin = true,
            "--agent" => agent = Some(PathBuf::from(value(args, &mut index, "--agent")?)),
            "--workspace" => workspace = Some(PathBuf::from(value(args, &mut index, "--workspace")?)),
            "--resume" => resume = Some(value(args, &mut index, "--resume")?),
            "--rebind-workspace" => rebind_workspace = true,
            option => parse_common(option, args, &mut index, &mut common)?,
        }
        index += 1;
    }
    if resume.is_none() && task.is_none() && task_file.is_none() && !stdin {
        return Err("new DCP Session requires --task, --task-file or --stdin".into());
    }
    if resume.is_some() && agent.is_some() {
        return Err("--agent cannot replace the immutable DCPAgentLock during resume".into());
    }
    Ok(RunCommand { common, task, task_file, stdin, agent, workspace, resume, rebind_workspace })
}

fn parse_session(args: &[String]) -> Result<Command, String> {
    let action = args.first().map(String::as_str).ok_or("missing session action")?;
    match action {
        "list" => {
            let mut common = Common::default();
            parse_common_tail(&args[1..], &mut common)?;
            Ok(Command::SessionList(common))
        }
        "show" => {
            let session_id = args.get(1).cloned().ok_or("missing SESSION_ID")?;
            let mut common = Common::default();
            parse_common_tail(&args[2..], &mut common)?;
            Ok(Command::SessionShow(IdCommand { common, session_id }))
        }
        "fork" => parse_fork(&args[1..]),
        "export" => parse_export(&args[1..]),
        "import" => parse_import(&args[1..]),
        other => Err(format!("unknown session action: {other}")),
    }
}

fn parse_fork(args: &[String]) -> Result<Command, String> {
    let session_id = args.first().cloned().ok_or("missing SESSION_ID")?;
    let mut common = Common::default();
    let mut message_id = None;
    let mut before = false;
    let mut worktree = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--at" => message_id = Some(value(args, &mut index, "--at")?),
            "--before" => before = true,
            "--worktree" => worktree = Some(value(args, &mut index, "--worktree")?),
            option => parse_common(option, args, &mut index, &mut common)?,
        }
        index += 1;
    }
    Ok(Command::SessionFork(ForkCommand {
        common,
        session_id,
        message_id: message_id.ok_or("session fork requires --at MESSAGE_ID")?,
        before,
        worktree,
    }))
}

fn parse_export(args: &[String]) -> Result<Command, String> {
    let session_id = args.first().cloned().ok_or("missing SESSION_ID")?;
    let mut common = Common::default();
    let mut file = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--output" => file = Some(PathBuf::from(value(args, &mut index, "--output")?)),
            option => parse_common(option, args, &mut index, &mut common)?,
        }
        index += 1;
    }
    Ok(Command::SessionExport(ExportCommand { common, session_id, file: file.ok_or("session export requires --output FILE")? }))
}

fn parse_import(args: &[String]) -> Result<Command, String> {
    let file = args.first().map(PathBuf::from).ok_or("missing bundle FILE")?;
    let mut common = Common::default();
    let mut workspace = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--workspace" => workspace = Some(PathBuf::from(value(args, &mut index, "--workspace")?)),
            option => parse_common(option, args, &mut index, &mut common)?,
        }
        index += 1;
    }
    Ok(Command::SessionImport(ImportCommand { common, file, workspace: workspace.ok_or("session import requires --workspace PATH")? }))
}

fn parse_resolve(args: &[String]) -> Result<Command, String> {
    let session_id = args.first().cloned().ok_or("missing SESSION_ID")?;
    let run_id = args.get(1).cloned().ok_or("missing RUN_ID")?;
    let operation_id = args.get(2).cloned().ok_or("missing OPERATION_ID")?;
    let mut common = Common::default();
    let mut output = None;
    let mut is_error = false;
    let mut index = 3;
    while index < args.len() {
        match args[index].as_str() {
            "--output" => output = Some(value(args, &mut index, "--output")?),
            "--is-error" => is_error = true,
            option => parse_common(option, args, &mut index, &mut common)?,
        }
        index += 1;
    }
    Ok(Command::RunResolve(ResolveCommand {
        common,
        session_id,
        run_id,
        operation_id,
        output: output.ok_or("run resolve requires --output TEXT")?,
        is_error,
    }))
}

fn parse_common_tail(args: &[String], common: &mut Common) -> Result<(), String> {
    let mut index = 0;
    while index < args.len() {
        parse_common(&args[index], args, &mut index, common)?;
        index += 1;
    }
    Ok(())
}

fn parse_common(option: &str, args: &[String], index: &mut usize, common: &mut Common) -> Result<(), String> {
    match option {
        "--state-dir" => common.state_dir = Some(PathBuf::from(value(args, index, option)?)),
        "--config" => common.config = Some(PathBuf::from(value(args, index, option)?)),
        "--auth-file" => common.auth_file = Some(PathBuf::from(value(args, index, option)?)),
        "--consume-auth-file" => common.consume_auth_file = true,
        "--policy" => common.policy = Some(PathBuf::from(value(args, index, option)?)),
        "--allow-shell" => common.allow_shell = true,
        "--allow-mcp" => common.allow_mcp = true,
        "--pass-env" => common.pass_env.push(value(args, index, option)?),
        "--format" => {
            common.output = match value(args, index, option)?.as_str() {
                "jsonl" => DcpEventFormat::Jsonl,
                "text" => DcpEventFormat::Text,
                value => return Err(format!("unsupported output format: {value}")),
            }
        }
        other => return Err(format!("unknown option: {other}")),
    }
    Ok(())
}

fn value(args: &[String], index: &mut usize, option: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index).cloned().ok_or_else(|| format!("{option} requires a value"))
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
