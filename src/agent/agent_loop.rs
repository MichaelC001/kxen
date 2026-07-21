//! agent loop 单轮实现（LLM 流式 -> tool_call 累积 -> 工具执行 -> 结果回传 -> 继续）。

use futures::StreamExt;
use crate::llm::tool::ToolCallAccumulator;
use crate::llm::{Delta, LlmClient, Message, ModelRef};
use crate::tools::dev_server::{dev_server, restart_task, DevServerParams, ReadySpec};
use crate::tools::exec::{exec, ExecOutcome, ExecParams};
use crate::tools::fs_tool::{delete, edit, read, write, EditSpec, FileTracker};
use crate::tools::shell::ShellKind;
use crate::tools::task::TaskRegistry;
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
    Phase { name: String },
    Done { turns: u32, stats: Option<RunStats> },
    Aborted,
    Error { message: String },
}

/// 单轮运行统计（TTFT / 耗时 / tok/s / tokens）。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RunStats {
    pub ttft_ms: u64,
    pub duration_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tokens_per_sec: u64,
}

#[derive(Debug)]
pub struct AgentOutcome {
    pub final_text: String,
    pub turns: u32,
    pub aborted: bool,
    pub stats: Option<RunStats>,
}

/// 会话级共享态：tool_search 挂载的 deferred 工具 + todo 清单。
/// 放 AppState，跨 send_message 存续；子代理不继承（各自独立）。
#[derive(Default)]
pub struct SessionExtras {
    pub extra_tools: std::sync::Mutex<std::collections::HashSet<String>>,
    pub todos: crate::tools::todo::TodoStore,
    /// 已装载 skill（"name\x1fargs" 键）：同 args 禁止重调（调研 §2）。
    pub loaded_skills: std::sync::Mutex<std::collections::HashSet<String>>,
    /// skill -> skill 递归深度（cap 3）。
    pub skill_depth: std::sync::atomic::AtomicU32,
}

pub struct AgentContext {
    pub registry: Arc<TaskRegistry>,
    pub tracker: FileTracker,
    pub workdir: Arc<Path>,
    pub model: ModelRef,
    pub store: crate::auth::credential::AuthStore,
    pub max_turns: u32,
    pub mrm: Option<Arc<crate::llm::mrm::ModelResourceManager>>,
    /// 子代理工具白名单（None = 全部常驻工具）。
    pub allowed_tools: Option<&'static [&'static str]>,
    pub extras: Option<Arc<SessionExtras>>,
    pub hooks: Option<Arc<crate::tools::hooks::HookRunner>>,
    pub loop_detector: crate::agent::loop_detect::LoopDetector,
    /// 取消令牌：loop 顶 / stream 消费 / 工具执行 三处检查点；子代理级联继承。
    pub cancel: Option<crate::agent::cancel::CancelToken>,
    /// lead 身份的 team 访问（None = 无 team 能力：subagent/workflow 子环境）。
    pub team: Option<Arc<crate::agent::team::TeamManager>>,
    /// teammate 身份（session_id, agent_name）：决定 send_message/team_task 可用。
    pub team_identity: Option<(String, String)>,
    /// lead 的 session id（team 工具路由用）。
    pub session_id: Option<String>,
    pub on_event: Arc<dyn Fn(AgentEvent) + Send + Sync>,
}

pub async fn run_turn(ctx: &mut AgentContext, mut messages: Vec<Message>) -> AgentOutcome {
    let base_tools = match ctx.allowed_tools {
        Some(allowed) => crate::agent::tools_spec::core_tools()
            .into_iter()
            .filter(|t| allowed.contains(&t.function.name.as_str()))
            .collect(),
        None => crate::agent::tools_spec::core_tools(),
    };
    let mut turns = 0u32;
    let mut final_text = String::new();
    let mut aborted = false;

    // 统计：TTFT（首个 Text/Reasoning delta）/ 总耗时 / tokens
    let started = std::time::Instant::now();
    let mut ttft: Option<std::time::Duration> = None;
    let mut usage: Option<(u64, u64)> = None;
    let stats = |ttft: Option<std::time::Duration>, usage: Option<(u64, u64)>| {
        let (input, output) = usage.unwrap_or((0, 0));
        let duration = started.elapsed();
        let gen_ms = duration.as_millis() as u64;
        Some(RunStats {
            ttft_ms: ttft.map(|t| t.as_millis() as u64).unwrap_or(0),
            duration_ms: gen_ms,
            input_tokens: input,
            output_tokens: output,
            tokens_per_sec: if gen_ms > 0 { output * 1000 / gen_ms } else { 0 },
        })
    };

    // 系统提示由 loop 统一注入（身份 + 工具策略 + write-goal + 焦点 goal），调用方不重复造。
    let system_owned = !matches!(messages.first(), Some(m) if m.role == crate::llm::types::Role::System);
    let mut last_involved: Vec<std::path::PathBuf> = Vec::new();
    if system_owned {
        let involved = ctx.tracker.files();
        last_involved = involved.clone();
        messages.insert(0, Message::system(crate::agent::prompt::system_prompt(&ctx.workdir, &involved)));
    }

    'outer: loop {
        turns += 1;
        if turns > ctx.max_turns {
            (ctx.on_event)(AgentEvent::Error { message: format!("max turns ({}) reached", ctx.max_turns) });
            break;
        }
        if ctx.cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            aborted = true;
            break 'outer;
        }

        // 渐进披露 + 身份过滤：每轮重建（tool_search 挂载下轮可见；team 系工具按身份开关）
        let mut tools = base_tools.clone();
        tools.retain(|t| match t.function.name.as_str() {
            "team" => ctx.team.is_some() && ctx.team_identity.is_none(),
            "send_message" | "team_task" => ctx.team_identity.is_some(),
            _ => true,
        });
        if let Some(extras) = &ctx.extras {
            let enabled = crate::core::shared::lock(&extras.extra_tools);
            tools.extend(crate::agent::tools_spec::deferred_tools().into_iter().filter(|t| enabled.contains(&t.function.name)));
        }

        // mid-turn 刷新：涉及文件变化时重建系统提示（OKF globs 激活 / goal 状态 / 多层就近）
        if system_owned {
            let involved = ctx.tracker.files();
            if involved != last_involved {
                messages[0] = Message::system(crate::agent::prompt::system_prompt(&ctx.workdir, &involved));
                last_involved = involved;
            }
        }

        let mut acc = ToolCallAccumulator::default();
        let mut text = String::new();
        let mut stream = LlmClient::stream_with_tools(&ctx.model, &messages, &tools, &ctx.store);

        // stream 消费：cancel 即时打断（select 轮询 Delta 与取消令牌的等待）
        loop {
            let delta = match &ctx.cancel {
                Some(token) => tokio::select! {
                    d = stream.next() => d,
                    _ = token.wait() => { aborted = true; break; }
                },
                None => stream.next().await,
            };
            let Some(delta) = delta else { break };
            match delta {
                Delta::Text(t) => {
                    if ttft.is_none() {
                        ttft = Some(started.elapsed());
                    }
                    text.push_str(&t);
                    (ctx.on_event)(AgentEvent::Text { text: t });
                }
                Delta::Reasoning(r) => {
                    if ttft.is_none() {
                        ttft = Some(started.elapsed());
                    }
                    (ctx.on_event)(AgentEvent::Reasoning { text: r });
                }
                Delta::ToolFragments(fragments) => acc.push(&fragments),
                Delta::Usage { input, output } => usage = Some((input, output)),
                Delta::Done => break,
                Delta::Error(e) => {
                    (ctx.on_event)(AgentEvent::Error { message: e });
                    return AgentOutcome { final_text, turns, aborted, stats: stats(ttft, usage) };
                }
                Delta::ToolCall { .. } => {}
            }
        }
        if aborted {
            break 'outer;
        }

        let calls = acc.take();
        if calls.is_empty() {
            final_text = text;
            (ctx.on_event)(AgentEvent::Done { turns, stats: stats(ttft, usage) });
            break;
        }

        // assistant 消息带标准 tool_calls，结果用 Role::Tool 回传。
        // 同一 call 数据要进两条协议消息（assistant.tool_calls + tool_result），arguments 只克隆一次。
        let mut results = Vec::with_capacity(calls.len());
        let mut loop_stop: Option<crate::agent::loop_detect::LoopStop> = None;
        for call in &calls {
            (ctx.on_event)(AgentEvent::ToolCall { name: call.name.clone(), summary: summarize_args(&call.arguments) });
            // 工具执行段：cancel 打断即落 interrupted 终态（不等待执行完成，后续任务由 registry 收尾）
            let cancel = ctx.cancel.clone();
            let result = match &cancel {
                Some(token) => tokio::select! {
                    r = execute_tool(&call.name, &call.arguments, ctx) => r,
                    _ = token.wait() => Err("(interrupted)".to_string()),
                },
                None => execute_tool(&call.name, &call.arguments, ctx).await,
            };
            let interrupted = matches!(&result, Err(e) if e == "(interrupted)");
            if interrupted {
                (ctx.on_event)(AgentEvent::ToolResult { name: call.name.clone(), summary: "interrupted".into() });
                results.push(result);
                aborted = true;
                break;
            }
            (ctx.on_event)(AgentEvent::ToolResult { name: call.name.clone(), summary: result_summary(&call.name, &result) });
            if let crate::agent::loop_detect::LoopVerdict::Stop(stop) = ctx.loop_detector.record(&call.name, &call.arguments, &result_text(&result)) {
                loop_stop = Some(stop);
                results.push(result);
                break;
            }
            results.push(result);
        }
        let assistant_calls: Vec<crate::llm::types::AssistantToolCall> = calls
            .iter()
            .map(|c| crate::llm::types::AssistantToolCall::function(c.id.clone(), c.name.clone(), c.arguments.clone()))
            .collect();
        messages.push(Message::assistant_with_tools(text, assistant_calls));
        for (call, result) in calls.into_iter().zip(results) {
            messages.push(Message::tool_result(call.id, call.name, result_text(&result)));
        }
        if aborted {
            break 'outer;
        }
        if let Some(stop) = loop_stop {
            // 中断空转：硬停本轮，原因作为结果带出（事件已通知前端）
            let reason = stop.to_string();
            (ctx.on_event)(AgentEvent::Error { message: reason.clone() });
            final_text = reason;
            break;
        }
    }

    if aborted {
        (ctx.on_event)(AgentEvent::Aborted);
    }
    AgentOutcome { final_text, turns, aborted, stats: stats(ttft, usage) }
}

async fn execute_tool(name: &str, arguments: &str, ctx: &mut AgentContext) -> Result<String, String> {
    let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    let cwd = ctx.workdir.to_string_lossy().to_string();

    // hooks：pre_tool_use 任一失败即阻断；post_tool_use 仅记录
    if let Some(hooks) = &ctx.hooks {
        hooks.run_pre(name, &json!({ "tool": name, "arguments": args })).await?;
    }
    let result = dispatch_tool(name, &args, &cwd, ctx).await;
    if let Some(hooks) = &ctx.hooks {
        let preview = match &result {
            Ok(text) => text.chars().take(400).collect::<String>(),
            Err(e) => format!("ERROR: {}", e.chars().take(400).collect::<String>()),
        };
        hooks.run_post(name, &json!({ "tool": name, "arguments": args, "result_preview": preview })).await;
    }
    result
}

fn dispatch_tool<'a>(name: &'a str, args: &'a Value, cwd: &'a str, ctx: &'a mut AgentContext) -> impl std::future::Future<Output = Result<String, String>> + 'a {
    async move {
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
        "task" => execute_task_tool(&args, ctx).await,
        "goal" => execute_goal_tool(&args).await,
        "glob" => {
            let base = resolve_path(args.get("path").and_then(Value::as_str).unwrap_or(&cwd), &ctx.workdir);
            let pattern = args.get("pattern").and_then(Value::as_str).ok_or("missing pattern")?;
            crate::tools::search::glob_files(pattern, &base).map(|hits| {
                if hits.is_empty() { "no matches".into() } else { hits.join("\n") }
            }).map_err(|e| e.to_string())
        }
        "grep" => {
            let base = resolve_path(args.get("path").and_then(Value::as_str).unwrap_or(&cwd), &ctx.workdir);
            let pattern = args.get("pattern").and_then(Value::as_str).ok_or("missing pattern")?;
            let filter = args.get("glob").and_then(Value::as_str);
            crate::tools::search::grep_files(pattern, &base, filter).map(|hits| {
                if hits.is_empty() { "no matches".into() } else { hits.join("\n") }
            }).map_err(|e| e.to_string())
        }
        "tool_search" => {
            let query = args.get("query").and_then(Value::as_str).ok_or("missing query")?.to_lowercase();
            let Some(extras) = &ctx.extras else {
                return Err("tool_search unavailable in this context".into());
            };
            let matches: Vec<_> = crate::agent::tools_spec::deferred_tools()
                .into_iter()
                .filter(|t| {
                    let hay = format!("{} {}", t.function.name, t.function.description).to_lowercase();
                    query.split_whitespace().any(|w| hay.contains(w))
                })
                .collect();
            if matches.is_empty() {
                return Ok("no deferred tools match the query".into());
            }
            let mut enabled = crate::core::shared::lock(&extras.extra_tools);
            let mut names = Vec::with_capacity(matches.len());
            for tool in &matches {
                enabled.insert(tool.function.name.clone());
                names.push(tool.function.name.clone());
            }
            Ok(format!(
                "mounted for this session: {}\n{}",
                names.join(", "),
                serde_json::to_string_pretty(&matches).unwrap_or_default()
            ))
        }
        "todo" => {
            let Some(extras) = &ctx.extras else {
                return Err("todo unavailable in this context".into());
            };
            match args.get("action").and_then(Value::as_str).ok_or("missing action")? {
                "add" => {
                    let content = args.get("content").and_then(Value::as_str).ok_or("missing content")?;
                    let item = extras.todos.add(content.to_string());
                    Ok(format!("added #{} {}", item.id, item.content))
                }
                "list" => Ok(extras.todos.render()),
                "complete" => {
                    let id = args.get("id").and_then(Value::as_u64).ok_or("missing id")? as u32;
                    Ok(if extras.todos.complete(id) { format!("completed #{id}") } else { format!("todo not found: #{id}") })
                }
                "clear" => Ok(format!("cleared {} completed", extras.todos.clear_done())),
                other => Err(format!("unknown todo action: {other}")),
            }
        }
        "webfetch" => {
            let url = args.get("url").and_then(Value::as_str).ok_or("missing url")?;
            crate::tools::webfetch::fetch_text(url).await
        }
        "team" => {
            let Some(team) = &ctx.team else {
                return Err("team tool unavailable in this context".into());
            };
            let Some(sid) = &ctx.session_id else {
                return Err("team tool needs a session".into());
            };
            team.lead_action(sid, &args).await
        }
        "send_message" | "team_task" => {
            let Some(team) = &ctx.team else {
                return Err("team tools unavailable in this context".into());
            };
            let Some((sid, name)) = &ctx.team_identity else {
                return Err(format!("{name} is teammate-only"));
            };
            team.teammate_action(sid, name, &args).await
        }
        "skill" => {
            let name = args.get("name").and_then(Value::as_str).ok_or("missing name")?;
            let skill_args = args.get("args").and_then(Value::as_str).unwrap_or("");
            let Some(extras) = &ctx.extras else {
                return Err("skill unavailable in this context".into());
            };
            // 递归深度 cap 3（skill -> skill 链）
            let depth = extras.skill_depth.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let result = (|| {
                if depth > crate::agent::skills::SKILL_RECURSION_CAP {
                    return Err(format!("skill recursion cap ({}) reached", crate::agent::skills::SKILL_RECURSION_CAP));
                }
                let Some(skill) = crate::agent::skills::find(&ctx.workdir, name) else {
                    return Err(format!("skill not found: {name}"));
                };
                if skill.disable_model_invocation {
                    return Err(format!("skill {name} is user-invocable only (disable-model-invocation)"));
                }
                // 同 args 禁止重调
                let key = format!("{name}\x1f{skill_args}");
                if !crate::core::shared::lock(&extras.loaded_skills).insert(key) {
                    return Err(format!("skill {name} already loaded with identical args - reuse the block in this session"));
                }
                Ok(crate::agent::skills::render_loaded(&skill, skill_args, "model"))
            })();
            extras.skill_depth.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            result
        }
        "agent" => {
            let role = args.get("role").and_then(Value::as_str).ok_or("missing role")?.to_string();
            let prompt = args.get("prompt").and_then(Value::as_str).ok_or("missing prompt")?.to_string();
            let Some(mut deps) = crate::agent::subagent::SubagentDeps::from_context(ctx) else {
                return Err("agent tool unavailable: mrm not configured".into());
            };
            // worktree 隔离：该次派发在独立树执行，主树零接触
            let mut note = String::new();
            if let Some(wt) = args.get("worktree").and_then(Value::as_str) {
                let info = crate::tools::worktree::create(&ctx.workdir, wt).await?;
                note = format!("\n[worktree: {} (branch {})]", info.path.display(), info.branch);
                deps.workdir = Arc::from(info.path.as_path());
            }
            let result = Box::pin(crate::agent::subagent::dispatch(&role, prompt, &deps)).await?;
            Ok(format!("{result}{note}"))
        }
        "worktree" => {
            let repo = ctx.workdir.clone();
            match args.get("action").and_then(Value::as_str).ok_or("missing action")? {
                "create" => {
                    let name = args.get("name").and_then(Value::as_str).ok_or("missing name")?;
                    let info = crate::tools::worktree::create(&repo, name).await?;
                    Ok(format!("worktree {} at {} (branch {})", info.name, info.path.display(), info.branch))
                }
                "remove" => {
                    let name = args.get("name").and_then(Value::as_str).ok_or("missing name")?;
                    let delete_branch = args.get("delete_branch").and_then(Value::as_bool).unwrap_or(false);
                    crate::tools::worktree::remove(&repo, name, delete_branch).await?;
                    Ok(format!("removed worktree {name}{}", if delete_branch { " (branch deleted)" } else { " (branch kept)" }))
                }
                "list" => {
                    let list = crate::tools::worktree::list(&repo).await?;
                    Ok(if list.is_empty() {
                        "no kxen worktrees".into()
                    } else {
                        list.iter().map(|i| format!("{} -> {} ({})", i.name, i.path.display(), i.branch)).collect::<Vec<_>>().join("\n")
                    })
                }
                "diff" => {
                    let name = args.get("name").and_then(Value::as_str).ok_or("missing name")?;
                    let stat = crate::tools::worktree::diff_stat(&repo, name).await?;
                    Ok(if stat.trim().is_empty() { "no changes on branch".into() } else { stat })
                }
                other => Err(format!("unknown worktree action: {other}")),
            }
        }
        "workflow" => {
            let script = args.get("script").and_then(Value::as_str).ok_or("missing script")?;
            let Some(deps) = crate::agent::subagent::SubagentDeps::from_context(ctx) else {
                return Err("workflow unavailable: mrm not configured".into());
            };
            Box::pin(crate::agent::workflow::run_tool(script, deps, ctx)).await
        }
        other => Err(format!("unknown tool: {other}")),
    }
    }
}

/// task 工具：后台任务统一管理（dev server 是带 ready 门的 start）。
async fn execute_task_tool(args: &Value, ctx: &mut AgentContext) -> Result<String, String> {
    let action = args.get("action").and_then(Value::as_str).ok_or("missing action")?;
    let cwd = ctx.workdir.to_string_lossy().to_string();
    match action {
        "start" => {
            let params = DevServerParams {
                command: args.get("command").and_then(Value::as_str).ok_or("missing command")?.to_string(),
                workdir: resolve_path(args.get("workdir").and_then(Value::as_str).unwrap_or(&cwd), &ctx.workdir).to_string_lossy().into_owned(),
                ready: args.get("ready").map(|r| ReadySpec {
                    pattern: r.get("pattern").and_then(Value::as_str).map(String::from),
                    port: r.get("port").and_then(Value::as_u64).map(|p| p as u16),
                    timeout_ms: r.get("timeout_ms").and_then(Value::as_u64),
                }),
                shell: args.get("shell").and_then(Value::as_str).map(parse_shell).transpose()?,
            };
            dev_server(params, &ctx.registry)
                .await
                .map(|s| format!("ready: {} (task {})", s.url.unwrap_or_else(|| "(no url)".into()), s.task_id))
                .map_err(|e| e.to_string())
        }
        "output" => {
            let id = args.get("task_id").and_then(Value::as_str).ok_or("missing task_id")?;
            ctx.registry
                .output(id)
                .map(|(output, truncated, status)| format!("status: {status:?}{}\n{output}", if truncated { " (truncated)" } else { "" }))
                .ok_or_else(|| format!("task not found: {id}"))
        }
        "kill" => {
            let id = args.get("task_id").and_then(Value::as_str).ok_or("missing task_id")?;
            Ok(if ctx.registry.kill(id).await { format!("killed {id}") } else { format!("task not found: {id}") })
        }
        "list" => {
            let list = ctx.registry.list();
            Ok(if list.is_empty() { "no tasks".into() } else { serde_json::to_string_pretty(&list).unwrap_or_default() })
        }
        "restart" => {
            let id = args.get("task_id").and_then(Value::as_str).ok_or("missing task_id")?;
            restart_task(id, &ctx.registry).await.map(|new_id| format!("restarted as {new_id}")).map_err(|e| e.to_string())
        }
        other => Err(format!("unknown task action: {other}")),
    }
}

async fn execute_goal_tool(args: &Value) -> Result<String, String> {
    let action = args.get("action").and_then(Value::as_str).ok_or("missing action")?;
    let dir = crate::core::paths::goals_dir();
    let show = |g: &crate::core::goal::Goal| {
        format!(
            "goal {} [{}] {}\ncriteria: {}\nturns: {} tokens: {} blocks: {}{}",
            g.id,
            format!("{:?}", g.status).to_lowercase(),
            g.contract.objective,
            g.contract.completion_criteria,
            g.turns_used,
            g.tokens_used,
            g.consecutive_blocks,
            g.block_reason.as_deref().map(|r| format!("\nblocked: {r}")).unwrap_or_default()
        )
    };
    match action {
        "list" => {
            let goals = crate::core::goal::Goal::list(&dir);
            Ok(if goals.is_empty() { "no goals".into() } else { goals.iter().map(|g| show(g)).collect::<Vec<_>>().join("\n---\n") })
        }
        "create" => {
            let contract = crate::core::goal::GoalContract {
                objective: args.get("objective").and_then(Value::as_str).ok_or("missing objective")?.to_string(),
                completion_criteria: args.get("completion_criteria").and_then(Value::as_str).ok_or("missing completion_criteria")?.to_string(),
                constraints: args.get("constraints").and_then(Value::as_str).map(String::from),
                budget: crate::core::goal::GoalBudget {
                    tokens: args.pointer("/budget/tokens").and_then(Value::as_u64),
                    turns: args.pointer("/budget/turns").and_then(Value::as_u64).map(|n| n as u32),
                    wall_clock_ms: args.pointer("/budget/wall_clock_ms").and_then(Value::as_u64),
                },
            };
            let id = format!("goal_{}_{:06x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0), std::process::id());
            let goal = crate::core::goal::Goal::create(contract, id).map_err(|e| e.to_string())?;
            goal.save(&dir).map_err(|e| e.to_string())?;
            Ok(show(&goal))
        }
        other => {
            let id = args.get("id").and_then(Value::as_str).ok_or("missing id")?;
            let mut goal = crate::core::goal::Goal::load(&dir, id).map_err(|e| e.to_string())?;
            match other {
                "get" => {}
                "activate" => goal.activate().map_err(|e| e.to_string())?,
                "pause" => goal.pause().map_err(|e| e.to_string())?,
                "resume" => goal.resume().map_err(|e| e.to_string())?,
                "cancel" => goal.cancel().map_err(|e| e.to_string())?,
                "complete" => {
                    let evidence = args.get("evidence").and_then(Value::as_str).ok_or("missing evidence")?;
                    goal.complete(evidence).map_err(|e| e.to_string())?;
                }
                unknown => return Err(format!("unknown goal action: {unknown}")),
            }
            goal.save(&dir).map_err(|e| e.to_string())?;
            Ok(show(&goal))
        }
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
