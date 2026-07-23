//! rust-analyzer 子进程 client：spawn + initialize 握手 + didOpen/didChange + publishDiagnostics 入 store。

use super::protocol::{encode, FrameDecoder};
use super::store::Store;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin};

const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

pub struct LspClient {
    child: tokio::sync::Mutex<Child>,
    stdin: tokio::sync::Mutex<ChildStdin>,
    pending: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>>,
    next_id: AtomicU64,
    /// 已 didOpen 的文件 -> 当前版本号（didChange 用全文同步递增）。
    opened: Mutex<HashMap<PathBuf, u64>>,
    pub store: Arc<Store>,
}

impl LspClient {
    /// spawn + initialize（rootUri=workspace）+ initialized。
    pub async fn start(root: &Path, store: Arc<Store>) -> Result<Arc<Self>, String> {
        let mut child = tokio::process::Command::new("rust-analyzer")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("rust-analyzer spawn failed: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let mut stdout = child.stdout.take().ok_or("no stdout")?;
        let pending: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_rx = pending.clone();
        let store_rx = store.clone();
        tokio::spawn(async move {
            let mut decoder = FrameDecoder::default();
            let mut chunk = [0u8; 8192];
            loop {
                let Ok(n) = stdout.read(&mut chunk).await else { break };
                if n == 0 {
                    break;
                }
                for frame in decoder.feed(&chunk[..n]) {
                    let Ok(v) = serde_json::from_str::<Value>(&frame) else { continue };
                    if let Some(id) = v.get("id").and_then(Value::as_u64) {
                        if let Some(tx) = pending_rx.lock().expect("lsp pending").remove(&id) {
                            let _ = tx.send(v);
                        }
                    } else if v.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics") {
                        if let Some(params) = v.get("params") {
                            store_rx.update_from_publish(params);
                        }
                    }
                }
            }
            pending_rx.lock().expect("lsp pending").clear();
        });
        let client = Arc::new(Self {
            child: tokio::sync::Mutex::new(child),
            stdin: tokio::sync::Mutex::new(stdin),
            pending,
            next_id: AtomicU64::new(1),
            opened: Mutex::new(HashMap::new()),
            store,
        });
        let root_uri = format!("file://{}", root.display());
        let init = client
            .request(
                "initialize",
                json!({
                    "processId": std::process::id(),
                    "rootUri": root_uri,
                    "capabilities": { "textDocument": { "publishDiagnostics": {} } },
                }),
            )
            .await?;
        if init.get("error").is_some() {
            client.kill().await;
            return Err(format!("rust-analyzer initialize rejected: {}", init["error"]));
        }
        client.notify("initialized", json!({})).await?;
        Ok(client)
    }

    /// 同步文件到 server：首次 didOpen（全文），之后 didChange（全文同步）。
    pub async fn sync_file(&self, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let uri = format!("file://{}", path.display());
        // guard 不跨 await：块内定 method/params，落锁后再发
        let (method, params) = {
            let mut opened = self.opened.lock().expect("lsp opened");
            match opened.get_mut(path) {
                Some(version) => {
                    *version += 1;
                    (
                        "textDocument/didChange",
                        json!({
                            "textDocument": { "uri": uri, "version": *version },
                            "contentChanges": [ { "text": text } ],
                        }),
                    )
                }
                None => {
                    opened.insert(path.to_path_buf(), 1);
                    (
                        "textDocument/didOpen",
                        json!({
                            "textDocument": { "uri": uri, "languageId": "rust", "version": 1, "text": text },
                        }),
                    )
                }
            }
        };
        self.notify(method, params).await
    }

    pub async fn kill(&self) {
        let _ = self.child.lock().await.kill().await;
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().expect("lsp pending").insert(id, tx);
        let frame = encode(&serde_json::to_string(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).map_err(|e| e.to_string())?);
        self.stdin.lock().await.write_all(&frame).await.map_err(|e| format!("lsp write: {e}"))?;
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(_)) => Err("rust-analyzer died".into()),
            Err(_) => Err(format!("lsp request {method} timed out")),
        }
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let frame = encode(&serde_json::to_string(&json!({ "jsonrpc": "2.0", "method": method, "params": params })).map_err(|e| e.to_string())?);
        self.stdin.lock().await.write_all(&frame).await.map_err(|e| format!("lsp write: {e}"))
    }
}
