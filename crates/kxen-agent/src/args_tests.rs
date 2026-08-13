use super::*;

fn args<'a>(values: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
    values.iter().map(|value| value.to_string())
}

#[test]
fn top_level_resume_shortcut_parses() {
    let Parsed::Command(command) = parse(args(&["--resume", "ses_one", "--task", "continue"])).unwrap() else { panic!("run") };
    let Command::Run(run) = *command else { panic!("run") };
    assert_eq!(run.resume.as_deref(), Some("ses_one"));
    assert_eq!(run.task.as_deref(), Some("continue"));
}

#[test]
fn resume_rejects_agent_replacement() {
    assert!(parse(args(&["--resume", "ses_one", "--agent", "x.yaml"])).is_err());
}

#[test]
fn fork_parses_conversation_and_worktree_axes() {
    let Parsed::Command(command) =
        parse(args(&["session", "fork", "ses_one", "--at", "msg_one", "--before", "--worktree", "try_fix"])).unwrap()
    else {
        panic!("fork")
    };
    let Command::SessionFork(fork) = *command else { panic!("fork") };
    assert!(fork.before);
    assert_eq!(fork.worktree.as_deref(), Some("try_fix"));
}

#[test]
fn sensitive_environment_passes_are_explicit_and_repeatable() {
    let Parsed::Command(command) =
        parse(args(&["--task", "fix", "--allow-shell", "--pass-env", "GITHUB_TOKEN", "--pass-env", "CI"])).unwrap()
    else {
        panic!("run")
    };
    let Command::Run(run) = *command else { panic!("run") };
    assert!(run.common.allow_shell);
    assert_eq!(run.common.pass_env, ["GITHUB_TOKEN", "CI"]);
}

#[test]
fn help_is_available_at_every_command_depth() {
    assert!(matches!(parse(args(&["run", "--help"])).unwrap(), Parsed::Help));
    assert!(matches!(parse(args(&["session", "fork", "--help"])).unwrap(), Parsed::Help));
    assert!(matches!(parse(args(&["agent", "validate", "--help"])).unwrap(), Parsed::Help));
}
