//! subagent 真实验证：主 agent 自主用 agent 工具派发 thinking/review 子代理。

use kxen_agent::agent_loop::{run_turn, AgentContext, AgentEvent};
use kxen_llm::mrm::ModelResourceManager;
use kxen_llm::{Message, ModelRef};
use kxen_tools::fs_tool::FileTracker;
use kxen_tools::task::TaskRegistry;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let workdir = std::env::temp_dir().join(format!("kxen-subdemo-{}", std::process::id()));
    std::fs::create_dir_all(&workdir).unwrap();
    std::fs::write(workdir.join("calc.py"), "def add(a, b):\n    return a - b\n").unwrap();

    let auth_path = kxen_core::paths::auth_file();
    let mut store = kxen_auth::credential::read_auth_file(&auth_path);
    kxen_auth::probe_all(&mut store);

    let config = kxen_core::config::Config::load(&kxen_core::paths::config_dir().join("config.toml"), None).unwrap();
    let mrm = Arc::new(ModelResourceManager::new(config));

    let mut ctx = AgentContext {
        registry: Arc::new(TaskRegistry::new()),
        tracker: FileTracker::default(),
        workdir: Arc::from(workdir.as_path()),
        model: ModelRef::new("xai", "grok-build-0.1"),
        store,
        max_turns: 8,
        mrm: Some(mrm),
        allowed_tools: None,
        loop_detector: kxen_agent::loop_detect::LoopDetector::new(),
        on_event: Arc::new(|event| match event {
            AgentEvent::Text { text } => print!("{text}"),
            AgentEvent::Reasoning { text } => eprint!("[r:{}]", first(&text, 30)),
            AgentEvent::ToolCall { name, summary } => println!("\n>>> {name}: {}", first(&summary, 90)),
            AgentEvent::ToolResult { name, summary } => println!("<<< {name}: {}", first(&summary, 90)),
            AgentEvent::Phase { name } => println!("\n--- PHASE: {name} ---"),
            AgentEvent::Done { turns } => println!("\n=== DONE {turns} turns ==="),
            AgentEvent::Error { message } => println!("\n!!! {message}"),
        }),
    };

    let messages = vec![
        Message::system("You are a coding agent with tools including `agent` (dispatch subagents by role). For code review tasks, dispatch the review role subagent instead of doing it yourself, then summarize its findings."),
        Message::user(format!("Review {} for bugs. Use the agent tool with role review.", workdir.join("calc.py").display())),
    ];

    let outcome = run_turn(&mut ctx, messages).await;
    println!("\nfinal: {}", outcome.final_text);
}

fn first(s: &str, max: usize) -> String {
    let mut c = s.chars();
    let t: String = c.by_ref().take(max).collect();
    if c.next().is_some() { format!("{t}…") } else { t }
}
