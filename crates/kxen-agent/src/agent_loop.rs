//! agent loop 单轮实现（LLM 流式 -> tool_call 累积 -> 工具执行 -> 结果回传 -> 继续）。

use futures::StreamExt;
use kxen_llm::tool::ToolCallAccumulator;
use kxen_llm::{Delta, LlmClient, Message, ModelRef};
use kxen_tools::dev_server::{dev_server, restart_task, DevServerParams, ReadySpec};
use kxen_tools::exec::{exec, ExecOutcome, ExecParams};
use kxen_tools::fs_tool::{delete, edit, read, write, EditSpec, FileTracker};
use kxen_tools::shell::ShellKind;
use kxen_tools::task::TaskRegistry;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    Text { text: String },
    Reasoning { text: String },
    ToolCall { name: String, summary: String },
    ToolResult { name: String, summary: String },
    Done { turns: u32 },
    Error { message: String },
}

#[derive(Debug)]
pub struct AgentOutcome {
    pub final_text: String,
    pub turns: u32,
}

pub struct AgentContext {
    pub registry: Arc<TaskRegistry>,
    pub tracker: FileTracker,
    pub workdir: PathBuf,
    pub model: ModelRef,
    pub store: kxen_auth::credential::AuthStore,
    pub max_turns: u32,
    pub on_event: Box<dyn Fn(AgentEvent) + Send>,
}

pub async fn run_turn(ctx: &mut AgentContext, mut messages: Vec<Message>) -> AgentOutcome {
    let tools = crate::tools_spec::core_tools();
    let mut turns = 0u32;
    let mut final_text = String::new();

    loop {
        turns += 1;
        if turns > ctx.max_turns {
            (ctx.on_event)(AgentEvent::Error { message: format!("max turns ({}) reached", ctx.max_turns) });
            break;
        }

        let mut acc = ToolCallAccumulator::default();
        let mut text = String::new();
        let mut stream = LlmClient::stream_with_tools(&ctx.model, &messages, &tools, &ctx.store);

        while let Some(delta) = stream.next().await {
            match delta {
                Delta::Text(t) => {
                    text.push_str(&t);
                    (ctx.on_event)(AgentEvent::Text { text: t });
                }
                Delta::Reasoning(r) => (ctx.on_event)(AgentEvent::Reasoning { text: r }),
                Delta::ToolFragments(fragments) => acc.push(&fragments),
                Delta::Usage { .. } => {}
                Delta::Done => break,
                Delta::Error(e) => {
                    (ctx.on_event)(AgentEvent::Error { message: e });
                    return AgentOutcome { final_text, turns };
                }
                Delta::ToolCall { .. } => {}
            }
        }

        let calls = acc.take();
        if calls.is_empty() {
            final_text = text;
            (ctx.on_event)(AgentEvent::Done { turns });
            break;
        }

        // assistant 消息带标准 tool_calls，结果用 Role::Tool 回传
        let assistant_calls: Vec<kxen_llm::types::AssistantToolCall> = calls
            .iter()
            .map(|c| kxen_llm::types::AssistantToolCall::function(c.id.clone(), c.name.clone(), c.arguments.clone()))
            .collect();
        messages.push(Message::assistant_with_tools(text, assistant_calls));
        for call in calls {
            let name = call.name;
            (ctx.on_event)(AgentEvent::ToolCall { name: name.clone(), summary: summarize_args(&call.arguments) });
            let result = execute_tool(&name, &call.arguments, ctx).await;
            let summary = result_summary(&name, &result);
            (ctx.on_event)(AgentEvent::ToolResult { name: name.clone(), summary: summary.clone() });
            messages.push(Message::tool_result(call.id.clone(), name, result_text(&result)));
        }
    }

    AgentOutcome { final_text, turns }
}

async fn execute_tool(name: &str, arguments: &str, ctx: &mut AgentContext) -> Result<String, String> {
    let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    let cwd = ctx.workdir.to_string_lossy().to_string();

    match name {
        "exec" => {
            let params = ExecParams {
                shell_type: parse_shell(args.get("type").and_then(Value::as_str).unwrap_or("zsh"))?,
                path: args.get("path").and_then(Value::as_str).unwrap_or(&cwd).to_string(),
                command: args.get("command").and_then(Value::as_str).ok_or("missing command")?.to_string(),
                timeout_ms: args.get("timeout_ms").and_then(Value::as_u64),
                background: args.get("background").and_then(Value::as_bool).unwrap_or(false),
            };
            match exec(params, &ctx.registry, &cwd).await {
                Ok(ExecOutcome::Foreground { output, exit_code, truncated }) => {
                    Ok(format!("exit {exit_code}{}\n{output}", if truncated { " (truncated)" } else { "" }))
                }
                Ok(ExecOutcome::Background { task_id }) => Ok(format!("backgrounded: {task_id} (notified on completion)")),
                Err(e) => Err(e.to_string()),
            }
        }
        "read" => {
            let path = resolve_path(args.get("path").and_then(Value::as_str).ok_or("missing path")?, &ctx.workdir);
            read(&path, &ctx.tracker, &cwd).map(|r| {
                let mut out = r.content;
                if r.truncated {
                    out.push_str(&format!("\n... ({} total lines, truncated)", r.total_lines));
                }
                out
            })
            .map_err(|e| e.to_string())
        }
        "edit" => {
            let path = resolve_path(args.get("path").and_then(Value::as_str).ok_or("missing path")?, &ctx.workdir);
            let spec = match args.get("mode").and_then(Value::as_str) {
                Some("anchors") => EditSpec::Anchors {
                    edits: serde_json::from_value(args.get("edits").cloned().unwrap_or(json!([]))).map_err(|e| e.to_string())?,
                },
                _ => EditSpec::Match {
                    old_string: args.get("old_string").and_then(Value::as_str).ok_or("missing old_string")?.to_string(),
                    new_string: args.get("new_string").and_then(Value::as_str).unwrap_or("").to_string(),
                    expected_replacements: args.get("expected_replacements").and_then(Value::as_u64).map(|n| n as usize),
                },
            };
            edit(&path, &spec, &ctx.tracker, &cwd).map(|r| format!("{}\n{}", r.diff_summary, r.diff)).map_err(|e| e.to_string())
        }
        "write" => {
            let path = resolve_path(args.get("path").and_then(Value::as_str).ok_or("missing path")?, &ctx.workdir);
            let content = args.get("content").and_then(Value::as_str).unwrap_or("");
            write(&path, content, &ctx.tracker, &cwd).map(|_| format!("wrote {} bytes", content.len())).map_err(|e| e.to_string())
        }
        "delete" => {
            let path = resolve_path(args.get("path").and_then(Value::as_str).ok_or("missing path")?, &ctx.workdir);
            delete(&path, &cwd).map(|_| "moved to Trash".to_string()).map_err(|e| e.to_string())
        }
        "task_output" => {
            let id = args.get("task_id").and_then(Value::as_str).ok_or("missing task_id")?;
            ctx.registry
                .output(id)
                .map(|(output, truncated, status)| format!("status: {status:?}{}\n{output}", if truncated { " (truncated)" } else { "" }))
                .ok_or_else(|| format!("task not found: {id}"))
        }
        "kill_task" => {
            let id = args.get("task_id").and_then(Value::as_str).ok_or("missing task_id")?;
            Ok(if ctx.registry.kill(id).await { format!("killed {id}") } else { format!("task not found: {id}") })
        }
        "list_tasks" => {
            let list = ctx.registry.list();
            Ok(if list.is_empty() { "no tasks".into() } else { serde_json::to_string_pretty(&list).unwrap_or_default() })
        }
        "dev_server" => {
            let params = DevServerParams {
                command: args.get("command").and_then(Value::as_str).ok_or("missing command")?.to_string(),
                workdir: resolve_path(args.get("workdir").and_then(Value::as_str).unwrap_or(&cwd), &ctx.workdir).to_string_lossy().into_owned(),
                ready: args.get("ready").map(|r| ReadySpec {
                    pattern: r.get("pattern").and_then(Value::as_str).map(String::from),
                    port: r.get("port").and_then(Value::as_u64).map(|p| p as u16),
                    timeout_ms: r.get("timeout_ms").and_then(Value::as_u64),
                }),
                shell: None,
            };
            dev_server(params, &ctx.registry)
                .await
                .map(|s| format!("ready: {} (task {})", s.url.unwrap_or_else(|| "(no url)".into()), s.task_id))
                .map_err(|e| e.to_string())
        }
        "restart_task" => {
            let id = args.get("task_id").and_then(Value::as_str).ok_or("missing task_id")?;
            restart_task(id, &ctx.registry).await.map(|new_id| format!("restarted as {new_id}")).map_err(|e| e.to_string())
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

fn parse_shell(s: &str) -> Result<ShellKind, String> {
    match s {
        "zsh" => Ok(ShellKind::Zsh),
        "bash" => Ok(ShellKind::Bash),
        "fish" => Ok(ShellKind::Fish),
        other => Err(format!("invalid shell type: {other} (must be zsh/bash/fish)")),
    }
}

fn resolve_path(input: &str, workdir: &Path) -> PathBuf {
    let p = PathBuf::from(input);
    if p.is_absolute() { p } else { workdir.join(p) }
}

fn summarize_args(arguments: &str) -> String {
    let trimmed = arguments.trim();
    if trimmed.len() <= 120 { trimmed.to_string() } else { format!("{}…", &trimmed[..trimmed.floor_char_boundary(120)]) }
}

fn result_summary(name: &str, result: &Result<String, String>) -> String {
    match result {
        Ok(text) => format!("{name}: {}", first_line(text, 100)),
        Err(e) => format!("{name} error: {}", first_line(e, 100)),
    }
}

fn result_text(result: &Result<String, String>) -> String {
    match result {
        Ok(text) => text.clone(),
        Err(e) => format!("ERROR: {e}"),
    }
}

fn first_line(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or("");
    if line.len() <= max { line.to_string() } else { format!("{}…", &line[..line.floor_char_boundary(max)]) }
}
