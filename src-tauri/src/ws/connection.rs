//! 单连接多路复用。读取、事件接收与 RPC 执行彼此独立，慢 RPC 不阻塞心跳或事件。
//!
//! 核心是传输无关的：握手（token/Origin/Host）与协议帧适配在 `web` 模块完成，
//! 这里只消费/产出自有 `Frame`（Vec<u8> 载荷便于跨 channel 传递）。

use std::future::Future;
use std::sync::Arc;

use futures::{Sink, SinkExt, Stream, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use super::protocol::{CallError, Request, Response};
use super::request::SystemAction;
use super::{StreamSequences, SubBinding};
use crate::AppState;

const OUTBOUND_CAPACITY: usize = 256;

/// 传输无关的连接帧：web 模块的 axum adapter 负责 Message <-> Frame 双向映射。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Text(Vec<u8>),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
}

enum FrameOutcome {
    Reply(Response),
    Call(Request),
}

#[derive(Default)]
struct CallTasks {
    tasks: JoinSet<()>,
    disconnect: kxen_gui::agent::cancel::CancelToken,
}

impl CallTasks {
    fn spawn<F>(&mut self, id: Value, future: F, outbound: mpsc::Sender<Frame>)
    where
        F: Future<Output = Result<Value, CallError>> + Send + 'static,
    {
        let disconnect = self.disconnect.clone();
        self.tasks.spawn(async move {
            kxen_gui::agent::approval::scope_wait_cancellation(disconnect, response_task(id, future, outbound)).await;
        });
    }

    /// 断线只取消尚在等待的审批。RPC task 随后脱离连接继续收尾，避免在已经 Allow
    /// 或已进入文件、进程、事务提交段后被 JoinSet drop 强制 abort。
    fn disconnect(&mut self) {
        self.disconnect.cancel();
        self.tasks.detach_all();
    }
}

impl Drop for CallTasks {
    fn drop(&mut self) {
        // 防止后续新增 early return 绕过 handle 尾部。先 detach 再由 JoinSet 字段 drop，
        // 已进入提交段的 task 才不会被隐式 abort。
        self.disconnect();
    }
}

pub async fn handle<I, O>(source: I, sink: O, state: Arc<AppState>)
where
    I: Stream<Item = Frame> + Unpin,
    O: Sink<Frame, Error = std::io::Error> + Unpin + Send + 'static,
{
    let (mut sink, mut source) = (sink, source);
    let (outbound, mut outbound_rx) = mpsc::channel::<Frame>(OUTBOUND_CAPACITY);
    let mut writer = tokio::spawn(async move {
        while let Some(frame) = outbound_rx.recv().await {
            sink.send(frame).await?;
        }
        Ok::<(), std::io::Error>(())
    });
    let mut subscriptions = Vec::<SubBinding>::new();
    let mut sequences = StreamSequences::default();
    let mut bus = state.bus.subscribe();
    let mut calls = CallTasks::default();

    'connection: loop {
        tokio::select! {
            _ = &mut writer => break,
            result = calls.tasks.join_next(), if !calls.tasks.is_empty() => {
                if let Some(Err(error)) = result {
                    tracing::warn!(%error, "connection-owned RPC task failed");
                }
            }
            frame = source.next() => match frame {
                Some(Frame::Text(bytes)) => {
                    // 无效 UTF-8 视为协议违例直接断开（tungstenite 时代等价于握手后读错误）
                    let Ok(text) = String::from_utf8(bytes) else { break };
                    match route_frame(&text, &mut subscriptions, &mut sequences) {
                        FrameOutcome::Reply(response) => {
                            if send_json(&outbound, &response).await.is_err() { break; }
                        }
                        FrameOutcome::Call(request) => {
                            let id = request.id;
                            let method = request.method;
                            let params = request.params;
                            let rpc_state = state.clone();
                            let future = async move { super::rpc::rpc_call(&method, params, &rpc_state).await };
                            calls.spawn(id, future, outbound.clone());
                        }
                    }
                }
                Some(Frame::Ping(payload)) => {
                    if outbound.send(Frame::Pong(payload)).await.is_err() { break; }
                }
                Some(Frame::Binary(_)) | Some(Frame::Pong(_)) => {}
                Some(Frame::Close) | None => break,
            },
            event = bus.recv() => {
                use tokio::sync::broadcast::error::RecvError;
                let chunks = match event {
                    Ok(event) => super::stream::event_to_chunks(event, &subscriptions, &mut sequences),
                    Err(RecvError::Lagged(dropped)) => vec![super::resync_chunk(dropped, &mut sequences)],
                    Err(RecvError::Closed) => break,
                };
                for chunk in chunks {
                    if send_json(&outbound, &chunk).await.is_err() {
                        break 'connection;
                    }
                }
            }
        }
    }
    calls.disconnect();
    writer.abort();
}

fn route_frame(text: &str, subscriptions: &mut Vec<SubBinding>, sequences: &mut StreamSequences) -> FrameOutcome {
    let request = match super::request::parse(text) {
        Ok(request) => request,
        Err(response) => return FrameOutcome::Reply(*response),
    };
    let stream_ids = subscriptions.iter().map(|binding| binding.stream_id.clone()).collect::<Vec<_>>();
    let action = match super::request::validate_system(&request, &stream_ids) {
        Ok(action) => action,
        Err(response) => return FrameOutcome::Reply(*response),
    };
    match action {
        Some(SystemAction::Heartbeat) => FrameOutcome::Reply(Response::ok(request.id, json!({ "alive": true }))),
        Some(SystemAction::Subscribe(topics)) => {
            let stream_id = super::protocol::stream_id("sub");
            subscriptions.push(SubBinding { stream_id: stream_id.clone(), topics });
            FrameOutcome::Reply(Response::ok(request.id, json!({ "stream_id": stream_id })))
        }
        Some(SystemAction::Unsubscribe(stream_id)) => {
            subscriptions.retain(|binding| binding.stream_id != stream_id);
            sequences.remove(&stream_id);
            FrameOutcome::Reply(Response::ok(request.id, json!(true)))
        }
        None => FrameOutcome::Call(request),
    }
}

#[cfg(test)]
fn spawn_response<F>(id: Value, future: F, outbound: mpsc::Sender<Frame>)
where
    F: Future<Output = Result<Value, CallError>> + Send + 'static,
{
    tokio::spawn(response_task(id, future, outbound));
}

async fn response_task<F>(id: Value, future: F, outbound: mpsc::Sender<Frame>)
where
    F: Future<Output = Result<Value, CallError>> + Send + 'static,
{
    let response = match future.await {
        Ok(value) => Response::ok(id, value),
        Err(error) => match error.data {
            Some(data) => Response::err_with_data(id, error.code, error.message, data),
            None => Response::err(id, error.code, error.message),
        },
    };
    let _ = send_json(&outbound, &response).await;
}

async fn send_json(outbound: &mpsc::Sender<Frame>, value: &impl serde::Serialize) -> Result<(), ()> {
    let text = serde_json::to_string(value).map_err(|_| ())?;
    outbound.send(Frame::Text(text.into_bytes())).await.map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn slow_rpc_does_not_block_immediate_frames() {
        let (outbound, mut received) = mpsc::channel(2);
        let (_release, wait) = tokio::sync::oneshot::channel::<()>();
        spawn_response(
            json!(1),
            async move {
                let _ = wait.await;
                Ok(json!("slow"))
            },
            outbound.clone(),
        );
        outbound.send(Frame::Text(b"heartbeat".to_vec())).await.unwrap();
        let next = tokio::time::timeout(std::time::Duration::from_millis(100), received.recv()).await.unwrap();
        assert_eq!(next, Some(Frame::Text(b"heartbeat".to_vec())));
    }

    #[tokio::test]
    async fn disconnect_aborts_approval_rpc_and_broker_fails_closed() {
        let broker = Arc::new(kxen_gui::agent::approval::ApprovalBroker::with_timeout(std::time::Duration::from_secs(30)));
        let executed = Arc::new(AtomicBool::new(false));
        let (registered_tx, registered_rx) = tokio::sync::oneshot::channel();
        let (outbound, _received) = mpsc::channel(2);
        let mut calls = CallTasks::default();
        let rpc_broker = broker.clone();
        let rpc_executed = executed.clone();
        calls.spawn(
            json!(1),
            async move {
                let (id, rx) = rpc_broker.register("", "delete worktree", "destructive operation");
                registered_tx.send(id.clone()).ok();
                if rpc_broker.wait(&id, rx, None).await == kxen_gui::agent::approval::ApprovalOutcome::Allow {
                    rpc_executed.store(true, Ordering::SeqCst);
                }
                Ok(json!(true))
            },
            outbound,
        );
        let id = tokio::time::timeout(std::time::Duration::from_secs(1), registered_rx).await.unwrap().unwrap();
        assert_eq!(broker.list_pending(None).len(), 1);

        drop(calls);

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !broker.list_pending(None).is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(!broker.respond(&id, true), "a disconnected approval RPC must not remain allow-able");
        assert!(!executed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn disconnect_keeps_in_progress_commit_alive() {
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let (outbound, _received) = mpsc::channel(2);
        let mut calls = CallTasks::default();
        calls.spawn(
            json!(1),
            async move {
                started_tx.send(()).ok();
                let _ = release_rx.await;
                done_tx.send(()).ok();
                Ok(json!(true))
            },
            outbound,
        );
        tokio::time::timeout(std::time::Duration::from_secs(1), started_rx).await.unwrap().unwrap();

        calls.disconnect();
        release_tx.send(()).unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(1), done_rx).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn allow_transitions_rpc_from_wait_to_durable_commit() {
        let broker = Arc::new(kxen_gui::agent::approval::ApprovalBroker::with_timeout(std::time::Duration::from_secs(30)));
        let (registered_tx, registered_rx) = tokio::sync::oneshot::channel();
        let (commit_tx, commit_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let (outbound, _received) = mpsc::channel(2);
        let mut calls = CallTasks::default();
        let rpc_broker = broker.clone();
        calls.spawn(
            json!(1),
            async move {
                let (id, rx) = rpc_broker.register("", "delete worktree", "destructive operation");
                registered_tx.send(id.clone()).ok();
                if rpc_broker.wait(&id, rx, None).await != kxen_gui::agent::approval::ApprovalOutcome::Allow {
                    return Ok(json!(false));
                }
                commit_tx.send(()).ok();
                let _ = release_rx.await;
                done_tx.send(()).ok();
                Ok(json!(true))
            },
            outbound,
        );
        let id = tokio::time::timeout(std::time::Duration::from_secs(1), registered_rx).await.unwrap().unwrap();
        assert!(broker.respond(&id, true));
        tokio::time::timeout(std::time::Duration::from_secs(1), commit_rx).await.unwrap().unwrap();

        calls.disconnect();
        release_tx.send(()).unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(1), done_rx).await.unwrap().unwrap();
    }

    #[test]
    fn unknown_unsubscribe_returns_structured_stream_error() {
        let mut subscriptions = Vec::new();
        let mut sequences = StreamSequences::default();
        let frame = r#"{"jsonrpc":"3.0","id":9,"method":"rpc.unsubscribe","params":{"stream_id":"sub-missing"}}"#;
        let FrameOutcome::Reply(response) = route_frame(frame, &mut subscriptions, &mut sequences) else {
            panic!("unsubscribe must reply")
        };
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["error"]["code"], super::super::protocol::STREAM_NOT_FOUND);
        assert_eq!(value["error"]["data"]["stream_id"], "sub-missing");
    }
}
