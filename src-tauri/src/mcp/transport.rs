//! stdio transport：MCP 标准形态——子进程 stdin/stdout 按行分隔的 JSON-RPC 2.0。
//! 读循环把响应按 id 路由到挂起的 oneshot；进程死亡则全体挂起请求失败。

use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};

pub struct StdioTransport {
    // child/stdin 只在 async 调用点持有，用 tokio Mutex；pending 在读循环里同步锁，保持 std Mutex
    child: tokio::sync::Mutex<Child>,
    stdin: tokio::sync::Mutex<ChildStdin>,
    pending: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>>,
    next_id: AtomicU64,
}

impl StdioTransport {
    pub fn spawn(command: &str, args: &[String], env: &HashMap<String, String>) -> Result<Arc<Self>, String> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let mut child = cmd.spawn().map_err(|e| format!("mcp spawn {command}: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let pending: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_rx = pending.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                if let Some(id) = v.get("id").and_then(|i| i.as_u64()) {
                    if let Some(tx) = pending_rx.lock().expect("mcp pending").remove(&id) {
                        let _ = tx.send(v);
                    }
                }
            }
            // EOF：全部挂起请求按失败结束（调用方走 lazy restart）
            pending_rx.lock().expect("mcp pending").clear();
        });
        Ok(Arc::new(Self {
            child: tokio::sync::Mutex::new(child),
            stdin: tokio::sync::Mutex::new(stdin),
            pending,
            next_id: AtomicU64::new(1),
        }))
    }

    /// 发送请求并等待响应（行分隔 JSON-RPC）。
    pub async fn request(&self, method: &str, params: Value, timeout: std::time::Duration) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().expect("mcp pending").insert(id, tx);
        let frame = serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let line = format!("{}\n", serde_json::to_string(&frame).map_err(|e| e.to_string())?);
        self.stdin.lock().await.write_all(line.as_bytes()).await.map_err(|e| format!("mcp write: {e}"))?;
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(_)) => Err("mcp server died".into()),
            Err(_) => Err(format!("mcp request {method} timed out")),
        }
    }

    /// 发通知（无 id，不等响应）。
    pub async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let frame = serde_json::json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let line = format!("{}\n", serde_json::to_string(&frame).map_err(|e| e.to_string())?);
        self.stdin.lock().await.write_all(line.as_bytes()).await.map_err(|e| format!("mcp write: {e}"))
    }

    pub async fn kill(&self) {
        let _ = self.child.lock().await.kill().await;
    }
}
