//! Workflow engine: model-authored JavaScript orchestration on rquickjs (sandboxed, no OS access).
//!
//! Globals available to scripts:
//! - `agent(role, prompt)` -> Promise<string>   dispatch a subagent by role (routed + gated by MRM)
//! - `CONSTRAINTS`                              role bindings + provider availability snapshot
//! - `phase(name)`                              progress marker, streamed to the frontend live
//! - `log(msg)`                                 tracing
//! Everything else is plain JS: `Promise.all` for fan-out, for-loops for pipelines.

use crate::agent_loop::{AgentContext, AgentEvent};
use crate::subagent::{dispatch, SubagentDeps};
use rquickjs::prelude::{Async, Func};
use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Promise, Value};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const WORKFLOW_TIMEOUT_MS: u64 = 10 * 60 * 1000;
const MAX_AGENTS_PER_WORKFLOW: u32 = 32;
const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
const STACK_LIMIT: usize = 1024 * 1024;

/// workflow 工具入口：QuickJS 在专属线程 + current_thread runtime 跑（rquickjs !Send 全隔离），
/// 本任务侧只做 phase 转发 / 结果等待 / 超时取消（全部 Send）。
pub async fn run_tool(script: &str, deps: SubagentDeps, ctx: &mut AgentContext) -> Result<String, String> {
    let (phase_tx, mut phase_rx) = mpsc::unbounded_channel::<String>();
    let (result_tx, result_rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
    let cancel = Arc::new(AtomicBool::new(false));

    let script_owned = script.to_string();
    let cancel_thread = cancel.clone();
    std::thread::Builder::new()
        .name("kxen-workflow".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = result_tx.send(Err(format!("workflow runtime: {e}")));
                    return;
                }
            };
            let result = rt.block_on(run_script(&script_owned, deps, phase_tx, cancel_thread));
            let _ = result_tx.send(result);
        })
        .map_err(|e| format!("workflow thread: {e}"))?;

    // 超时/取消：置中断标志，QuickJS 在下一个字节码检查点中止，线程自行退出
    let cancel_on_drop = CancelGuard(cancel.clone());
    let on_event = ctx.on_event.clone();
    let body = async {
        tokio::pin!(result_rx);
        loop {
            tokio::select! {
                r = &mut result_rx => break r.unwrap_or_else(|_| Err("workflow thread died".into())),
                Some(name) = phase_rx.recv() => on_event(AgentEvent::Phase { name }),
            }
        }
    };

    let out = match tokio::time::timeout(Duration::from_millis(WORKFLOW_TIMEOUT_MS), body).await {
        Ok(result) => result,
        Err(_) => Err(format!("workflow timed out after {}s", WORKFLOW_TIMEOUT_MS / 1000)),
    };
    drop(cancel_on_drop);
    // 结果先到时排空已发送但未接收的 phase（发送先于 result，通道里必有）
    while let Ok(name) = phase_rx.try_recv() {
        on_event(AgentEvent::Phase { name });
    }
    out
}

/// 作用域结束即触发 JS 中断（覆盖超时与提前返回两条路径）。
struct CancelGuard(Arc<AtomicBool>);

impl Drop for CancelGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

async fn run_script(
    script: &str,
    deps: SubagentDeps,
    phase_tx: mpsc::UnboundedSender<String>,
    cancel: Arc<AtomicBool>,
) -> Result<String, String> {
    let constraints = build_constraints(&deps).await;

    let runtime = AsyncRuntime::new().map_err(|e| e.to_string())?;
    runtime.set_memory_limit(MEMORY_LIMIT).await;
    runtime.set_max_stack_size(STACK_LIMIT).await;
    runtime.set_interrupt_handler(Some(Box::new(move || cancel.load(Ordering::Relaxed)))).await;
    let context = AsyncContext::full(&runtime).await.map_err(|e| e.to_string())?;

    let script_owned = script.to_string();
    context
        .async_with(async move |ctx| {
            let globals = ctx.globals();

            // CONSTRAINTS：直接注入 JS 字面量，脚本免解析
            let inject = format!("globalThis.CONSTRAINTS = {};", serde_json::to_string(&constraints).unwrap_or_else(|_| "{}".into()));
            ctx.eval::<Value, _>(inject).catch(&ctx).map_err(|e| e.to_string())?;

            // agent(role, prompt)：每次调用克隆一份 deps；计数器硬性封顶。
            // 错误直接构造 Error::FromJs（不捕获 Ctx，避免生命周期问题），promise 照样 reject。
            let counter = Arc::new(AtomicU32::new(0));
            let agent_fn = Func::from(Async(move |role: String, prompt: String| {
                let deps = deps.clone();
                let counter = counter.clone();
                async move {
                    let n = counter.fetch_add(1, Ordering::Relaxed);
                    if n >= MAX_AGENTS_PER_WORKFLOW {
                        return Err(workflow_err(format!("workflow agent budget exhausted ({MAX_AGENTS_PER_WORKFLOW})")));
                    }
                    dispatch(&role, prompt, &deps).await.map_err(workflow_err)
                }
            }));
            globals.set("agent", agent_fn).catch(&ctx).map_err(|e| e.to_string())?;

            let phase_fn = Func::from(move |name: String| {
                let _ = phase_tx.send(name);
            });
            globals.set("phase", phase_fn).catch(&ctx).map_err(|e| e.to_string())?;

            globals
                .set("log", Func::from(|msg: String| tracing::info!(target: "workflow", "{msg}")))
                .catch(&ctx)
                .map_err(|e| e.to_string())?;

            // 脚本体包成 async 函数；返回值统一转字符串
            let wrapped = format!(
                "(async () => {{\n{script_owned}\n}})().then(v => typeof v === 'string' ? v : JSON.stringify(v ?? null))"
            );
            let promise = ctx.eval::<Promise, _>(wrapped).catch(&ctx).map_err(|e| e.to_string())?;
            let text: String = promise.into_future().await.catch(&ctx).map_err(|e| e.to_string())?;
            Ok::<String, String>(text)
        })
        .await
}

/// 脚本侧可见的错误（promise rejection 的 message）。
fn workflow_err(msg: String) -> rquickjs::Error {
    rquickjs::Error::FromJs { from: "workflow agent", to: "promise", message: Some(msg) }
}

/// constraints 快照：角色绑定 + provider 实时可用性 + mrm 文字描述。
async fn build_constraints(deps: &SubagentDeps) -> serde_json::Value {
    let mut roles = serde_json::Map::new();
    for role in ["thinking", "planning", "execution", "review", "research"] {
        if let Some(binding) = deps.mrm.role(role) {
            roles.insert(
                role.to_string(),
                serde_json::json!({
                    "provider": binding.provider,
                    "model": binding.model,
                    "available": deps.mrm.available(&binding.provider).await,
                }),
            );
        }
    }
    serde_json::json!({
        "roles": roles,
        "mrm": deps.mrm.describe().await,
        "max_agents": MAX_AGENTS_PER_WORKFLOW,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kxen_core::config::{Config, Limits, ProviderLimit, RoleBinding};
    use kxen_llm::mrm::ModelResourceManager;
    use std::collections::HashMap;

    /// 不触网的 deps：agent 闭包会真实走 dispatch -> mrm -> 凭证缺失报错，
    /// 但纯 JS 能力（算数 / Promise.all / phase / CONSTRAINTS）不需要网络。
    fn test_deps() -> SubagentDeps {
        let mut roles = HashMap::new();
        roles.insert("thinking".into(), RoleBinding { provider: "anthropic".into(), model: "claude".into() });
        roles.insert("execution".into(), RoleBinding { provider: "xai".into(), model: "grok".into() });
        let config = Config {
            roles,
            limits: Limits { global_concurrent: 4, providers: HashMap::<String, ProviderLimit>::new() },
        };
        SubagentDeps {
            registry: Arc::new(kxen_tools::task::TaskRegistry::new()),
            workdir: Arc::from(std::path::Path::new("/tmp")),
            store: kxen_auth::credential::AuthStore::default(),
            mrm: Arc::new(ModelResourceManager::new(config)),
        }
    }

    async fn run_ok(script: &str) -> String {
        let (tx, _rx) = mpsc::unbounded_channel();
        run_script(script, test_deps(), tx, Arc::new(AtomicBool::new(false))).await.expect("script should succeed")
    }

    #[tokio::test]
    async fn plain_js_arithmetic() {
        assert_eq!(run_ok("return 1 + 2").await, "3");
    }

    #[tokio::test]
    async fn promise_all_fanout() {
        let out = run_ok("const r = await Promise.all([1,2,3].map(async x => x * 2)); return r.join(',')").await;
        assert_eq!(out, "2,4,6");
    }

    #[tokio::test]
    async fn constraints_are_visible() {
        let out = run_ok("return CONSTRAINTS.roles.thinking.provider + '/' + CONSTRAINTS.roles.execution.model").await;
        assert_eq!(out, "anthropic/grok");
    }

    #[tokio::test]
    async fn phases_are_streamed() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let fut = run_script("phase('scan'); phase('fix'); return 'done'", test_deps(), tx, Arc::new(AtomicBool::new(false)));
        tokio::pin!(fut);
        let mut phases = Vec::new();
        let result = loop {
            tokio::select! {
                r = &mut fut => break r,
                Some(name) = rx.recv() => phases.push(name),
            }
        };
        // 与 run_tool 相同的竞态处理：结果先到则排空残留 phase
        while let Ok(name) = rx.try_recv() {
            phases.push(name);
        }
        assert_eq!(result.unwrap(), "done");
        assert_eq!(phases, ["scan", "fix"]);
    }

    #[tokio::test]
    async fn js_exception_surfaces_message() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = run_script("throw new Error('boom')", test_deps(), tx, Arc::new(AtomicBool::new(false))).await.unwrap_err();
        assert!(err.contains("boom"), "unexpected: {err}");
    }

    #[tokio::test]
    async fn object_result_is_json() {
        assert_eq!(run_ok("return { a: 1 }").await, "{\"a\":1}");
    }
}
