//! 动态工具沙箱宿主：tool_define 注册的 QuickJS 实现在专属线程跑（与 workflow 引擎同构隔离）。
//!
//! 与 workflow 的差异：无 agent() 派发、无 journal、无 phase/meta——契约只有
//! `args`（深冻结入参）、`await tool(name, args)`（execute_tool 全路径桥）、
//! `parallel`/`log`，顶层 return 一个字符串。
//! 递归防护与 C 同口径：桥内拒绝 workflow/tool_define，且本宿主安装桥时拒绝 dyn__*
//! （动态工具不许嵌套调用动态工具，阻断自调用无限递归——每次嵌套都是新线程新运行时）。

use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Promise, Value};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::oneshot;

use super::STACK_LIMIT;
use super::cancel::{CancelGuard, cascade_parent};
use super::tools::{Subcalls, ToolBridge};
use crate::agent::agent_loop::AgentContext;
use crate::agent::dynamic::DynamicToolDef;

pub async fn run(def: &DynamicToolDef, args: &serde_json::Value, ctx: &AgentContext) -> Result<String, String> {
    let (result_tx, result_rx) = oneshot::channel::<Result<String, String>>();
    let interrupt = Arc::new(AtomicBool::new(false));
    // 父 run abort 级联进沙箱令牌（桥内子工具调用的取消源），作用域结束 CancelGuard 统一收尾
    let cancel = crate::agent::cancel::CancelToken::new();
    let parent_cascade = cascade_parent(ctx.cancel.clone(), &cancel);
    let bridge = ToolBridge::new(ctx, cancel.clone());
    let script = super::js::wrap_dynamic_script(&def.implementation, args);
    let interrupt_thread = interrupt.clone();
    std::thread::Builder::new()
        .name("kxen-dyn-tool".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = result_tx.send(Err(format!("dynamic tool runtime: {e}")));
                    return;
                }
            };
            let result = rt.block_on(run_script(&script, bridge, interrupt_thread));
            let _ = result_tx.send(result);
        })
        .map_err(|e| format!("dynamic tool thread: {e}"))?;

    let cancel_on_drop = CancelGuard(interrupt, cancel);
    let timeout = crate::core::config::sandbox_config().dynamic_tool_timeout();
    let out = match tokio::time::timeout(timeout, result_rx).await {
        Ok(result) => result.unwrap_or_else(|_| Err("dynamic tool thread died".into())),
        Err(_) => Err(format!("dynamic tool {} timed out after {}s", def.name, timeout.as_secs())),
    };
    drop(cancel_on_drop);
    drop(parent_cascade);
    out
}

/// 引擎部分（拆出供测试）：无超时/线程包装，桥恒装（reject_dynamic=true）、journal 恒 None。
async fn run_script(script: &str, bridge: ToolBridge, interrupt: Arc<AtomicBool>) -> Result<String, String> {
    let runtime = AsyncRuntime::new().map_err(|e| e.to_string())?;
    runtime.set_memory_limit(crate::core::config::sandbox_config().memory_limit()).await;
    runtime.set_max_stack_size(STACK_LIMIT).await;
    runtime.set_interrupt_handler(Some(Box::new(move || interrupt.load(Ordering::Relaxed)))).await;
    let context = AsyncContext::full(&runtime).await.map_err(|e| e.to_string())?;
    context
        .async_with(async move |ctx| {
            // 与 workflow 同序：深冻结工具（args 注入在 wrap_dynamic_script 内用它冻结）-> 结果格式化 -> parallel
            ctx.eval::<Value, _>(super::js::DEEP_FREEZE_JS).catch(&ctx).map_err(|e| e.to_string())?;
            ctx.eval::<Value, _>(super::js::FORMAT_RESULT_JS).catch(&ctx).map_err(|e| e.to_string())?;
            ctx.eval::<Value, _>(super::js::PARALLEL_JS).catch(&ctx).map_err(|e| e.to_string())?;
            let subcalls: Subcalls = Arc::new(std::sync::Mutex::new(Vec::new()));
            super::tools::install(&ctx, bridge, Arc::new(std::sync::Mutex::new(None)), subcalls.clone(), true)?;
            ctx.globals()
                .set("log", rquickjs::prelude::Func::from(|msg: String| tracing::info!(target: "dynamic_tool", "{msg}")))
                .catch(&ctx)
                .map_err(|e| e.to_string())?;
            let promise = ctx.eval::<Promise, _>(script.to_string()).catch(&ctx).map_err(|e| e.to_string())?;
            let mut text: String = promise.into_future().await.catch(&ctx).map_err(|e| e.to_string())?;
            // 子调用结构化块（同 workflow 口径）：模型与前端都能感知桥内调了哪些工具
            text.push_str(&super::tools::render_block(&crate::core::shared::lock(&subcalls)));
            Ok::<String, String>(text)
        })
        .await
}
