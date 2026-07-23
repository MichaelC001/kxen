//! LspManager：per-workspace 单 rust-analyzer，懒启动（首个 diagnostics 请求才拉起）。
//! 未安装/不可用 -> 友好降级文案，不阻塞 agent（mise shim 存在但不可用的场景按不可用处理）。

mod process;
mod protocol;
mod store;

use process::LspClient;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use store::Store;

pub struct LspManager {
    root: PathBuf,
    store: Arc<Store>,
    state: tokio::sync::Mutex<State>,
}

enum State {
    NotStarted,
    Running(Arc<LspClient>),
    Unavailable(String),
}

impl LspManager {
    pub fn new(root: PathBuf) -> Arc<Self> {
        Arc::new(Self { root, store: Arc::new(Store::default()), state: tokio::sync::Mutex::new(State::NotStarted) })
    }

    /// 懒启动：probe `--version` 先行（shim 存在但不可用时快速失败），再全握手。
    async fn ensure_client(&self) -> Result<Arc<LspClient>, String> {
        let mut state = self.state.lock().await;
        match &*state {
            State::Running(c) => Ok(c.clone()),
            State::Unavailable(msg) => Err(msg.clone()),
            State::NotStarted => {
                let probe = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    tokio::process::Command::new("rust-analyzer").arg("--version").output(),
                )
                .await;
                let usable = matches!(probe, Ok(Ok(out)) if out.status.success());
                if !usable {
                    let msg = "rust-analyzer unavailable: install it (rustup component add rust-analyzer) for compiler-level diagnostics".to_string();
                    *state = State::Unavailable(msg.clone());
                    return Err(msg);
                }
                match LspClient::start(&self.root, self.store.clone()).await {
                    Ok(client) => {
                        *state = State::Running(client.clone());
                        Ok(client)
                    }
                    Err(e) => {
                        *state = State::Unavailable(e.clone());
                        Err(e)
                    }
                }
            }
        }
    }

    /// 仅 peek：不触发启动（write/edit 后的 didChange 挂点用，未启动就跳过）。
    fn running_client(&self) -> Option<Arc<LspClient>> {
        match self.state.try_lock() {
            Ok(guard) => match &*guard {
                State::Running(c) => Some(c.clone()),
                _ => None,
            },
            Err(_) => None,
        }
    }

    /// 工作区切换时调用：杀 server，回到 NotStarted。
    pub async fn shutdown(&self) {
        let mut state = self.state.lock().await;
        if let State::Running(c) = &*state {
            c.kill().await;
        }
        *state = State::NotStarted;
    }
}

/// fs_tool write/edit 成功后的同步挂点：fire-and-forget，server 未起不动。
pub fn notify_change(lsp: Option<&Arc<LspManager>>, path: &Path) {
    let Some(lsp) = lsp else { return };
    let Some(client) = lsp.running_client() else { return };
    let path = path.to_path_buf();
    tokio::spawn(async move {
        if let Err(e) = client.sync_file(&path).await {
            tracing::warn!(error = %e, path = %path.display(), "lsp sync failed");
        }
    });
}

/// diagnostics 工具入口：path 给则同步该文件并等发布，无则同步会话内 .rs 文件集。
pub async fn diagnostics_tool(
    lsp: Option<&Arc<LspManager>>,
    path: Option<&str>,
    workdir: &Path,
    tracked: Vec<PathBuf>,
) -> Result<String, String> {
    let Some(lsp) = lsp else { return Err("lsp not configured".into()) };
    let client = match lsp.ensure_client().await {
        Ok(c) => c,
        Err(msg) => return Ok(msg), // 降级文案走正常结果，agent 可读继续干活
    };
    let target = path.map(|p| workdir.join(p));
    let files: Vec<PathBuf> = match &target {
        Some(p) => vec![p.clone()],
        None => tracked.into_iter().filter(|p| p.extension().is_some_and(|e| e == "rs")).take(50).collect(),
    };
    for f in &files {
        client.sync_file(f).await?;
    }
    // publishDiagnostics 是异步通知：冷启动有索引耗时，轮询等首波结果
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let arrived = match &target {
            Some(p) => client.store.has_entry(p),
            None => files.iter().any(|f| client.store.has_entry(f)),
        };
        if arrived || std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    Ok(client.store.snapshot(target.as_deref()))
}
