//! 工具执行入口与路由（goal 工具单独在 goal_tool.rs）。

use crate::tools::dev_server::{dev_server, restart_task, DevServerParams, ReadySpec};
use crate::tools::exec::{exec, ExecOutcome, ExecParams};
use crate::tools::fs_tool::{delete, edit, read, write, EditSpec};
use serde_json::{json, Value};
use std::sync::Arc;

use super::context::AgentContext;
use super::goal_tool::execute_goal_tool;
use super::helpers::{parse_shell, resolve_path};

pub async fn execute_tool(name: &str, arguments: &str, ctx: &mut AgentContext) -> Result<String, String> {
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

pub fn dispatch_tool<'a>(name: &'a str, args: &'a Value, cwd: &'a str, ctx: &'a mut AgentContext) -> impl std::future::Future<Output = Result<String, String>> + 'a {
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
        "knowledge" => {
            match args.get("action").and_then(Value::as_str).ok_or("missing action")? {
                "add" => {
                    let scope = args.get("scope").and_then(Value::as_str).unwrap_or("memory");
                    let slug = args.get("slug").and_then(Value::as_str);
                    let kind = args.get("type").and_then(Value::as_str).unwrap_or("note");
                    let description = args.get("description").and_then(Value::as_str).ok_or("missing description")?;
                    let content = args.get("content").and_then(Value::as_str).ok_or("missing content")?;
                    let path = crate::knowledge::add(scope, &ctx.workdir, slug, kind, description, content)?;
                    Ok(format!("knowledge saved ({scope}): {path}"))
                }
                "list" => Ok(serde_json::to_string_pretty(&crate::knowledge::list(&ctx.workdir)).unwrap_or_default()),
                "remove" => {
                    let scope = args.get("scope").and_then(Value::as_str).ok_or("missing scope")?;
                    let slug = args.get("slug").and_then(Value::as_str).ok_or("missing slug")?;
                    crate::knowledge::remove(scope, &ctx.workdir, slug)?;
                    Ok(format!("knowledge removed ({scope}/{slug})"))
                }
                other => Err(format!("unknown knowledge action: {other}")),
            }
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
            let result = Box::pin(crate::agent::subagent::dispatch(&role, prompt, &deps, crate::agent::activity::AgentKind::Subagent)).await?;
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
pub async fn execute_task_tool(args: &Value, ctx: &mut AgentContext) -> Result<String, String> {
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
