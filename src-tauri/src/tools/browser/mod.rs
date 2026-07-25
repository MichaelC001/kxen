//! browser 工具：CDP 驱动系统 Chrome headless（deferred 工具，经 tool_search 挂载）。
//! per-session 懒启动单实例（SessionExtras 键控）；同 session 复用同一页面，导航累加历史。
//! 已知边界：SSRF 守卫只钉初始 URL（net_guard 与 webfetch 同口径），页内跳转/资源加载不经守卫——
//! 逐跳拦截需要 CDP request interception，超出 v1 范围。

pub mod ax;
pub mod chrome;
pub mod driver;
pub mod fake;

use driver::{BrowserDriver, NavOutcome};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// evaluate 结果上限（10KB，JSON.stringify 后截断）。
const MAX_EVAL_CHARS: usize = 10 * 1024;

/// 一次浏览器会话：driver + ref 分配状态。
/// ref 单调递增（跨 snapshot 不复用），epoch 在导航/点击后递增——旧 epoch 的 ref 即失效，
/// 防止模型拿着上一页的 ref 点本页元素。
struct Instance {
    driver: Box<dyn BrowserDriver>,
    epoch: u64,
    ref_epoch: u64,
    refs: HashMap<u32, ax::RefTarget>,
    next_ref: u32,
    shot_seq: u32,
}

impl Instance {
    fn new(driver: Box<dyn BrowserDriver>) -> Self {
        Self { driver, epoch: 0, ref_epoch: 0, refs: HashMap::new(), next_ref: 1, shot_seq: 0 }
    }

    fn resolve_ref(&self, r: u32) -> Result<&ax::RefTarget, String> {
        let Some(target) = self.refs.get(&r) else {
            return Err(format!("unknown ref {r}: not assigned by the latest snapshot - run browser snapshot to get current refs"));
        };
        if self.ref_epoch != self.epoch {
            return Err(format!("stale ref {r}: the page changed since that snapshot - run browser snapshot again"));
        }
        Ok(target)
    }
}

/// SessionExtras 里的浏览器槽位：tokio Mutex（操作全 async），同 session 并发 tool call 串行化。
#[derive(Default)]
pub struct BrowserSlot {
    inner: tokio::sync::Mutex<Option<Instance>>,
}

impl BrowserSlot {
    /// 显式释放（session_delete 清理链 / browser close 共用）。
    pub async fn close(&self) -> bool {
        if let Some(mut instance) = self.inner.lock().await.take() {
            let _ = instance.driver.close().await;
            return true;
        }
        false
    }

    /// 测试播种：预置 fake 实例，跳过真实 Chrome 懒启动。
    #[doc(hidden)]
    pub async fn seed(&self, driver: Box<dyn BrowserDriver>) {
        *self.inner.lock().await = Some(Instance::new(driver));
    }
}

pub async fn dispatch(args: &Value, slot: Option<&BrowserSlot>, session_id: Option<&str>) -> Result<String, String> {
    let slot = slot.ok_or("browser unavailable in this context")?;
    let action = args.get("action").and_then(Value::as_str).ok_or("missing action")?;
    let mut guard = slot.inner.lock().await;
    match action {
        "open" | "navigate" => {
            let url = args.get("url").and_then(Value::as_str).ok_or("missing url")?;
            crate::tools::net_guard::check_url(url).await?;
            let instance = ensure(&mut guard).await?;
            let outcome = instance.driver.navigate(url).await?;
            instance.epoch += 1;
            Ok(format!("{}\nhint: run browser snapshot to read the page", nav_line("navigated to", &outcome)))
        }
        "snapshot" => {
            let instance = opened(&mut guard)?;
            let nodes = instance.driver.ax_tree().await?;
            let snap = ax::render(&nodes, instance.next_ref);
            instance.next_ref += snap.refs.len() as u32;
            instance.refs = snap.refs.into_iter().map(|t| (t.id, t)).collect();
            instance.ref_epoch = instance.epoch;
            Ok(if snap.text.is_empty() { "page has no accessible content".to_string() } else { snap.text })
        }
        "click" => {
            let r = parse_ref(args)?;
            let instance = opened(&mut guard)?;
            let target = instance.resolve_ref(r)?.clone();
            instance.driver.click(target.backend).await?;
            instance.epoch += 1;
            Ok(format!("clicked [{r}] {}\nhint: the page may have changed - run browser snapshot before the next action", target.label))
        }
        "fill" => {
            let r = parse_ref(args)?;
            let text = args.get("text").and_then(Value::as_str).ok_or("missing text")?;
            let instance = opened(&mut guard)?;
            let target = instance.resolve_ref(r)?.clone();
            instance.driver.fill(target.backend, text).await?;
            // fill 不 bump epoch：表单填多项再提交是同一份 snapshot 的主流程
            Ok(format!("filled [{r}] {} with {} chars", target.label, text.chars().count()))
        }
        "evaluate" => {
            let expr = args.get("expr").and_then(Value::as_str).ok_or("missing expr")?;
            let instance = opened(&mut guard)?;
            let out = instance.driver.evaluate(expr).await?;
            Ok(cap(&out, MAX_EVAL_CHARS))
        }
        "screenshot" => {
            let dir = screenshot_dir(session_id)?;
            let instance = opened(&mut guard)?;
            let bytes = instance.driver.screenshot().await?;
            instance.shot_seq += 1;
            std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
            let path = dir.join(format!("shot-{:013}-{}.png", crate::core::session::now_ms(), instance.shot_seq));
            std::fs::write(&path, &bytes).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
            Ok(format!("screenshot saved: {} ({} bytes)", path.display(), bytes.len()))
        }
        "back" => {
            let instance = opened(&mut guard)?;
            let outcome = instance.driver.back().await?;
            instance.epoch += 1;
            Ok(nav_line("back to", &outcome))
        }
        "close" => {
            if let Some(mut instance) = guard.take() {
                let _ = instance.driver.close().await;
                return Ok("browser closed".into());
            }
            Ok("browser was not running".into())
        }
        other => Err(format!("unknown browser action: {other}")),
    }
}

/// 懒启动：首个导航动作时才拉起 Chrome（snapshot/click 等只读已有实例，不隐式启动）。
async fn ensure(slot: &mut Option<Instance>) -> Result<&mut Instance, String> {
    if slot.is_none() {
        let driver = chrome::ChromeDriver::launch().await?;
        *slot = Some(Instance::new(Box::new(driver)));
    }
    Ok(slot.as_mut().expect("instance just ensured"))
}

fn opened(slot: &mut Option<Instance>) -> Result<&mut Instance, String> {
    slot.as_mut().ok_or_else(|| "no page open yet - use browser open {url} first".to_string())
}

fn parse_ref(args: &Value) -> Result<u32, String> {
    args.get("ref").and_then(Value::as_u64).map(|n| n as u32).ok_or_else(|| "missing ref (integer from browser snapshot)".to_string())
}

fn nav_line(prefix: &str, outcome: &NavOutcome) -> String {
    format!("{prefix} {}\ntitle: {}", outcome.url, outcome.title)
}

/// session 目录下的 browser/ 子目录；session id 先过 ids 校验（路径拼接前的防穿越闸，与 session.rs 同闸）。
fn screenshot_dir(session_id: Option<&str>) -> Result<PathBuf, String> {
    let sid = session_id.ok_or("browser screenshot needs a session context")?;
    crate::core::ids::validate_id(sid).map_err(|e| format!("invalid session id: {e}"))?;
    Ok(crate::core::paths::sessions_dir().join(sid).join("browser"))
}

fn cap(text: &str, max: usize) -> String {
    if text.len() <= max { text.to_string() } else { format!("{}...(truncated)", &text[..text.floor_char_boundary(max)]) }
}
