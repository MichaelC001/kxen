//! workflow 沙箱的通用工具桥：脚本内 `await tool(name, args)` 在一次模型往返内编排多次工具调用。
//! 与 engine（super::workflow）分离：350 行门禁 + 桥的安全边界集中一处可审。
//!
//! 安全边界（不设旁路）：
//! - 每次调用走 `execute_tool` 完整路径：执行侧 permits 复验、hooks、ApprovalBroker、
//!   MCP 门控全部继承调用方 AgentContext；桥本身不做任何额外放行。
//! - ctx 快照在 workflow 线程外构建（字段皆 Arc/owned，rquickjs !Send 隔离不变）；
//!   notify/persist_turn/tool_journal 显式置 None——子调用的 durable 边界是 workflow_journal
//!   （本文件逐次 intent/record），外层 workflow 工具调用本身才是会话/DCP journal 的边界。
//! - 编排类工具递归（workflow）桥内直接拒绝；沙箱快照的 code_orchestration 恒 false 双保险。
//!
//! 治理：64 次调用封顶（参照 32 次 agent 派发先例）；单次输出按字符 cap 截断；
//! journal 逐次 resume_gate/record（role 维度命名空间 "tool:<name>"），崩溃恢复可逐条 replay。

use rquickjs::prelude::{Async, Func};
use rquickjs::{CatchResultExt, Ctx, Value};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::agent::agent_loop::AgentContext;

/// 脚本内工具调用总次数上限（与 MAX_AGENTS_PER_WORKFLOW=32 同风格：防失控扇出）。
const MAX_TOOL_CALLS_PER_WORKFLOW: u32 = 64;
/// 单次桥调用输出 cap（字符），与 mcp OUTPUT_CAP 同量级：结果要回流进脚本字符串并最终进转录。
const TOOL_OUTPUT_CAP: usize = 50_000;

/// 调用方 AgentContext 的桥用快照：克隆全部 Arc/owned 字段，run 级状态显式重置。
/// 字段新增时编译器强制在此点名——快照边界不接受隐式继承。
pub struct ToolBridge {
    ctx: AgentContext,
}

impl ToolBridge {
    pub fn new(ctx: &AgentContext, cancel: crate::agent::cancel::CancelToken) -> Self {
        Self {
            ctx: AgentContext {
                registry: ctx.registry.clone(),
                // 与主会话共享新鲜度视图：桥内 read 过的文件主会话同样视为已读
                tracker: ctx.tracker.clone(),
                workdir: ctx.workdir.clone(),
                child_env: ctx.child_env.clone(),
                path_grants: ctx.path_grants.clone(),
                path_scope: ctx.path_scope.clone(),
                model: ctx.model.clone(),
                store: ctx.store.clone(),
                max_turns: ctx.max_turns,
                max_pure_retries: ctx.max_pure_retries,
                mrm: ctx.mrm.clone(),
                allowed_tools: ctx.allowed_tools.clone(),
                extras: ctx.extras.clone(),
                hooks: ctx.hooks.clone(),
                loop_detector: crate::agent::loop_detect::LoopDetector::new(),
                cancel: Some(cancel),
                team: ctx.team.clone(),
                team_identity: ctx.team_identity.clone(),
                session_id: ctx.session_id.clone(),
                exec_scope: ctx.exec_scope.clone(),
                bound_goal_id: ctx.bound_goal_id.clone(),
                goal_binding_frozen: ctx.goal_binding_frozen,
                agents: ctx.agents.clone(),
                bus: ctx.bus.clone(),
                approvals: ctx.approvals.clone(),
                kanban_auto: ctx.kanban_auto.clone(),
                mcp: ctx.mcp.clone(),
                mcp_approval_prechecked: ctx.mcp_approval_prechecked,
                lsp: ctx.lsp.clone(),
                // 子调用不开通知路由（background 回执只在主会话承诺）、不落会话转录、
                // 不进 DCP tool journal：这三个边界都由外层 workflow 工具调用承载
                notify: None,
                persist_compaction: None,
                persist_turn: None,
                tool_journal: None,
                domain_tools: ctx.domain_tools.clone(),
                auxiliary_usage: ctx.auxiliary_usage.clone(),
                usage_reporter: ctx.usage_reporter.clone(),
                on_event: ctx.on_event.clone(),
                stream_override: ctx.stream_override.clone(),
                // 快照内永不再开桥：workflow 递归在桥内按名拒绝，这里兜底
                code_orchestration: false,
            },
        }
    }
}

/// 一次脚本内工具调用的投影记录（前端 workflow 工具行的子调用列表数据源）。
pub struct Subcall {
    pub name: String,
    pub ok: bool,
    /// 真实执行耗时；journal 缓存回放为 None（未执行，不虚构耗时）
    pub ms: Option<u64>,
    pub cached: bool,
}

pub type Subcalls = Arc<Mutex<Vec<Subcall>>>;

type WfJournal = Arc<Mutex<Option<crate::agent::workflow_journal::Journal>>>;

/// 安装 __kxen_tool 桥与 tool() JS 门面。journal/occurrence 键复用 agent 派发同一文件：
/// role 维度固定 "tool:<name>"，与 agent 派发键同构哈希，互不冒充。
/// 递归防护（所有宿主一致）：workflow/tool_define/tool_undefine 一律拒绝——编排类工具不许在沙箱内递归，
/// 动态工具注册/卸载动作不许由沙箱内脚本发起（审批边界在宿主侧）。reject_dynamic（动态工具宿主）
/// 额外拒绝 dyn__*：动态工具嵌套动态工具每次都是新线程新运行时，自调用即无限递归。
pub(crate) fn install(
    qjs: &Ctx<'_>,
    bridge: ToolBridge,
    journal: WfJournal,
    subcalls: Subcalls,
    reject_dynamic: bool,
) -> Result<(), String> {
    let bridge = Arc::new(bridge);
    let counter = Arc::new(AtomicU32::new(0));
    let occurrences = Arc::new(Mutex::new(HashMap::<(String, String), u32>::new()));
    let tool_fn = Func::from(Async(move |name: String, args: Option<String>| {
        let bridge = bridge.clone();
        let counter = counter.clone();
        let occurrences = occurrences.clone();
        let journal = journal.clone();
        let subcalls = subcalls.clone();
        async move {
            let n = counter.fetch_add(1, Ordering::Relaxed);
            if n >= MAX_TOOL_CALLS_PER_WORKFLOW {
                return Err(super::workflow_err(format!("workflow tool call budget exhausted ({MAX_TOOL_CALLS_PER_WORKFLOW})")));
            }
            // 递归编排拒绝在 journal 之前：不留 intent，resume 语义不涉及
            if name == "workflow" || name == "tool_define" || name == "tool_undefine" {
                return Err(super::workflow_err(format!("recursive tool(\"{name}\") orchestration is not allowed")));
            }
            if reject_dynamic && name.starts_with(crate::agent::dynamic::NAME_PREFIX) {
                return Err(super::workflow_err(format!("nested dynamic tool call is not allowed: {name}")));
            }
            let args = args.unwrap_or_else(|| "{}".into());
            let occurrence = {
                let mut occurrences = crate::core::shared::lock(&occurrences);
                let entry = occurrences.entry((name.clone(), args.clone())).or_insert(0);
                let current = *entry;
                *entry = entry.saturating_add(1);
                current
            };
            let role = format!("tool:{name}");
            match crate::core::shared::lock(&journal).as_mut().map(|j| j.resume_gate(&role, &args, None, occurrence)) {
                // resume 命中：回缓存不真实执行（与 agent 派发同语义），耗时记 None
                Some(Ok(Some(cached))) => {
                    crate::core::shared::lock(&subcalls).push(Subcall { name, ok: true, ms: None, cached: true });
                    return Ok(cached);
                }
                // Unknown（intent 在、result 无）或 intent 落盘失败：fail closed，不静默重放副作用
                Some(Err(msg)) => return Err(super::workflow_err(msg)),
                None | Some(Ok(None)) => {}
            }
            let started = std::time::Instant::now();
            let result = crate::agent::agent_loop::execute_tool(&name, &args, &bridge.ctx).await;
            let ms = started.elapsed().as_millis() as u64;
            match result {
                Ok(text) => {
                    let text = cap_output(text);
                    if let Some(j) = crate::core::shared::lock(&journal).as_mut() {
                        j.record(&role, &args, None, occurrence, &text).map_err(super::workflow_err)?;
                    }
                    crate::core::shared::lock(&subcalls).push(Subcall { name, ok: true, ms: Some(ms), cached: false });
                    Ok(text)
                }
                Err(e) => {
                    crate::core::shared::lock(&subcalls).push(Subcall { name, ok: false, ms: Some(ms), cached: false });
                    Err(super::workflow_err(e))
                }
            }
        }
    }));
    qjs.globals().set("__kxen_tool", tool_fn).catch(qjs).map_err(|e| e.to_string())?;
    qjs.eval::<Value, _>(super::js::TOOL_JS).catch(qjs).map_err(|e| e.to_string())?;
    Ok(())
}

/// 输出截断：按字符计（与 mcp cap_output 同口径），超限追加显式标记，不静默吞尾巴。
fn cap_output(text: String) -> String {
    let total = text.chars().count();
    if total <= TOOL_OUTPUT_CAP {
        return text;
    }
    let kept: String = text.chars().take(TOOL_OUTPUT_CAP).collect();
    format!("{kept}\n...[truncated by workflow tool bridge, {total} chars total]")
}

/// 子调用结构化块：追加在 workflow 结果尾部，前端按标记解析渲染（模型同文可见，一行一条 JSONL）。
/// 块体上限即调用上限（64 行短 JSON），远低于任何转录 cap，无需二次截断。
pub(crate) fn render_block(subcalls: &[Subcall]) -> String {
    if subcalls.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n\n[kxen:tool-calls]");
    for call in subcalls {
        // 手工拼 JSON 定 key 序（serde_json 默认 BTreeMap 按字母序）：name 在前便于人读与前端调试
        let name = serde_json::to_string(&call.name).expect("string serialization cannot fail");
        let status = if call.ok { "ok" } else { "error" };
        write!(out, "\n{{\"name\":{name},\"status\":\"{status}\"").expect("writing to String cannot fail");
        if let Some(ms) = call.ms {
            write!(out, ",\"ms\":{ms}").expect("writing to String cannot fail");
        }
        if call.cached {
            out.push_str(",\"cached\":true");
        }
        out.push('}');
    }
    out.push_str("\n[/kxen:tool-calls]");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_output_truncates_with_marker() {
        let short = "abc".to_string();
        assert_eq!(cap_output(short.clone()), short);
        let long = "汉".repeat(TOOL_OUTPUT_CAP + 10);
        let capped = cap_output(long);
        assert!(capped.contains("truncated by workflow tool bridge"), "{capped}");
        assert!(capped.chars().count() > TOOL_OUTPUT_CAP, "标记本身在 cap 之外");
    }

    #[test]
    fn render_block_shape() {
        assert_eq!(render_block(&[]), "");
        let block = render_block(&[
            Subcall { name: "read".into(), ok: true, ms: Some(12), cached: false },
            Subcall { name: "grep".into(), ok: false, ms: Some(3), cached: false },
            Subcall { name: "read".into(), ok: true, ms: None, cached: true },
        ]);
        assert!(block.starts_with("\n\n[kxen:tool-calls]"), "{block}");
        assert!(block.ends_with("[/kxen:tool-calls]"), "{block}");
        assert!(block.contains(r#"{"name":"read","status":"ok","ms":12}"#), "{block}");
        assert!(block.contains(r#"{"name":"grep","status":"error","ms":3}"#), "{block}");
        // 缓存回放不虚构耗时
        assert!(block.contains(r#"{"name":"read","status":"ok","cached":true}"#), "{block}");
    }
}
