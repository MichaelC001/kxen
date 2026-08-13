use std::path::PathBuf;

use kxen_core::agent::dcp::DcpEventFormat;

pub const HELP: &str = r#"kxen-agent - standalone non-interactive DCPAgent runtime

USAGE:
  kxen-agent [run] [OPTIONS]
  kxen-agent --resume SESSION_ID [OPTIONS]
  kxen-agent agent validate FILE [COMMON OPTIONS]
  kxen-agent session list [COMMON OPTIONS]
  kxen-agent session show SESSION_ID [COMMON OPTIONS]
  kxen-agent session fork SESSION_ID --at MESSAGE_ID [--before] [--worktree NAME] [COMMON OPTIONS]
  kxen-agent session export SESSION_ID --output FILE [COMMON OPTIONS]
  kxen-agent session import FILE --workspace PATH [COMMON OPTIONS]
  kxen-agent run show SESSION_ID RUN_ID [COMMON OPTIONS]
  kxen-agent run resolve SESSION_ID RUN_ID OPERATION_ID --output TEXT [--is-error] [COMMON OPTIONS]

RUN OPTIONS:
  --task TEXT                 Task text
  --task-file FILE            Read task from a file
  --stdin                     Read task from stdin
  --agent FILE                Load a predefined dcpagent.yaml; omitted means Builder mode
  --workspace PATH            Workspace for a new Session or explicit resume rebind
  --resume SESSION_ID         Resume a durable Session
  --rebind-workspace          Accept a relocated Workspace after identity validation

COMMON OPTIONS:
  --state-dir PATH            Runtime state root; defaults to KXEN_AGENT_STATE_DIR or kxen/agent
  --config FILE               Provider/MRM config; defaults to ~/.config/kxen/config.toml
  --auth-file FILE            Credential store; defaults to the kxen auth.json
  --policy FILE               JSON runtime policy restricting capabilities and budgets
  --allow-shell               Permit noninteractive shell after immutable safety denies
  --allow-mcp                 Permit DCPAgent-requested MCP tools and policy Ask entries
  --pass-env NAME             Pass one sensitive environment variable to tools; repeatable
  --format jsonl|text         Event output format; defaults to jsonl
  -h, --help                  Show help
  -V, --version               Show version

Provider API keys may be supplied through the existing auth.json or supported environment variables.
Credentials are never written to Session bundles.
"#;

#[derive(Clone, Debug)]
pub struct Common {
    pub state_dir: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub auth_file: Option<PathBuf>,
    pub policy: Option<PathBuf>,
    pub output: DcpEventFormat,
    pub allow_shell: bool,
    pub allow_mcp: bool,
    pub pass_env: Vec<String>,
}

impl Default for Common {
    fn default() -> Self {
        Self {
            state_dir: None,
            config: None,
            auth_file: None,
            policy: None,
            output: DcpEventFormat::Jsonl,
            allow_shell: false,
            allow_mcp: false,
            pass_env: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunCommand {
    pub common: Common,
    pub task: Option<String>,
    pub task_file: Option<PathBuf>,
    pub stdin: bool,
    pub agent: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
    pub resume: Option<String>,
    pub rebind_workspace: bool,
}

#[derive(Clone, Debug)]
pub struct IdCommand {
    pub common: Common,
    pub session_id: String,
}

#[derive(Clone, Debug)]
pub struct ForkCommand {
    pub common: Common,
    pub session_id: String,
    pub message_id: String,
    pub before: bool,
    pub worktree: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ExportCommand {
    pub common: Common,
    pub session_id: String,
    pub file: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ImportCommand {
    pub common: Common,
    pub file: PathBuf,
    pub workspace: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ResolveCommand {
    pub common: Common,
    pub session_id: String,
    pub run_id: String,
    pub operation_id: String,
    pub output: String,
    pub is_error: bool,
}

#[derive(Clone, Debug)]
pub struct RunShowCommand {
    pub common: Common,
    pub session_id: String,
    pub run_id: String,
}

#[derive(Clone, Debug)]
pub struct AgentValidateCommand {
    pub common: Common,
    pub file: PathBuf,
}

#[derive(Clone, Debug)]
pub enum Command {
    Run(RunCommand),
    SessionList(Common),
    SessionShow(IdCommand),
    SessionFork(ForkCommand),
    SessionExport(ExportCommand),
    SessionImport(ImportCommand),
    RunShow(RunShowCommand),
    RunResolve(ResolveCommand),
    AgentValidate(AgentValidateCommand),
}

impl Command {
    pub fn common(&self) -> &Common {
        match self {
            Self::Run(command) => &command.common,
            Self::SessionList(common) => common,
            Self::SessionShow(command) => &command.common,
            Self::SessionFork(command) => &command.common,
            Self::SessionExport(command) => &command.common,
            Self::SessionImport(command) => &command.common,
            Self::RunShow(command) => &command.common,
            Self::RunResolve(command) => &command.common,
            Self::AgentValidate(command) => &command.common,
        }
    }

    pub fn data_dir(&self) -> PathBuf {
        self.common()
            .state_dir
            .clone()
            .or_else(|| std::env::var_os("KXEN_AGENT_STATE_DIR").map(PathBuf::from))
            .unwrap_or_else(|| kxen_core::core::paths::data_dir().join("agent"))
    }

    pub fn config_file(&self) -> PathBuf {
        self.common().config.clone().unwrap_or_else(|| kxen_core::core::paths::config_dir().join("config.toml"))
    }

    pub fn auth_file(&self) -> PathBuf {
        self.common().auth_file.clone().unwrap_or_else(kxen_core::core::paths::auth_file)
    }

    pub fn policy_file(&self) -> Option<PathBuf> {
        self.common().policy.clone()
    }

    pub fn output(&self) -> DcpEventFormat {
        self.common().output
    }

    pub fn allow_shell(&self) -> bool {
        self.common().allow_shell
    }

    pub fn allow_mcp(&self) -> bool {
        self.common().allow_mcp
    }

    pub fn pass_env(&self) -> Vec<String> {
        self.common().pass_env.clone()
    }
}

pub enum Parsed {
    Help,
    Version,
    Command(Box<Command>),
}
