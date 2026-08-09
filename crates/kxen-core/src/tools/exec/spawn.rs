//! 后台进程 spawn 漏斗：exec background / task start / dev_server / restart 全部经此收口。
//! DCP 落盘点：注册成功先落 start 行（intent），退出收割先落 exit 行再公开 exit_code。

use crate::core::shared::{SharedStr, lock};
use crate::tools::exec::ExecError;
use crate::tools::task::{TaskHandle, TaskOwner, TaskRegistry, append_capped};
use crate::tools::task_journal::{self, TaskLine};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

const OUTPUT_CAP: usize = 64 * 1024;

/// id 由调用方生成：restart 要沿用原 id 重新注册（同配置重启 id 不变）。
pub async fn spawn_task(
    id: &str,
    argv: Vec<String>,
    display_command: &str,
    workdir: &str,
    registry: &Arc<TaskRegistry>,
    owner: &TaskOwner,
    port: Option<u16>,
) -> Result<Arc<TaskHandle>, ExecError> {
    spawn_task_inner(id, argv, display_command, workdir, registry, owner, SpawnRegistration { port, expected_generation: None }).await
}

pub(crate) struct RespawnOptions {
    pub port: Option<u16>,
    pub expected_generation: u64,
}

/// restart 专用：发布新进程前以旧 generation 做 CAS，失败时立即回收新进程。
pub(crate) async fn respawn_task(
    id: &str,
    argv: Vec<String>,
    display_command: &str,
    workdir: &str,
    registry: &Arc<TaskRegistry>,
    owner: &TaskOwner,
    options: RespawnOptions,
) -> Result<Arc<TaskHandle>, ExecError> {
    spawn_task_inner(
        id,
        argv,
        display_command,
        workdir,
        registry,
        owner,
        SpawnRegistration { port: options.port, expected_generation: Some(options.expected_generation) },
    )
    .await
}

struct SpawnRegistration {
    port: Option<u16>,
    expected_generation: Option<u64>,
}

async fn spawn_task_inner(
    id: &str,
    argv: Vec<String>,
    display_command: &str,
    workdir: &str,
    registry: &Arc<TaskRegistry>,
    owner: &TaskOwner,
    registration: SpawnRegistration,
) -> Result<Arc<TaskHandle>, ExecError> {
    let (bin, args) = argv.split_first().ok_or_else(|| ExecError::Spawn("empty argv".into()))?;
    let generation = registry.allocate_generation().map_err(ExecError::Spawn)?;
    let mut cmd = Command::new(bin);
    cmd.args(args).current_dir(workdir).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    // 独立进程组组长:kill 走 killpg 才能覆盖 shell 的孙进程（dev server 子进程不泄漏）。
    // 仅 unix 有进程组语义与外部 kill 命令，支持范围即 unix
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn().map_err(|e| ExecError::Spawn(format!("{bin}: {e}")))?;

    let output = Arc::new(Mutex::new(String::new()));
    let truncated = Arc::new(Mutex::new(false));
    let exit_code = Arc::new(Mutex::new(None));
    let pid = child.id();

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let handle = Arc::new(TaskHandle {
        id: id.to_string(),
        owner: owner.clone(),
        generation,
        command: SharedStr::from(display_command),
        workdir: SharedStr::from(workdir),
        output: output.clone(),
        truncated: truncated.clone(),
        started_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
        pid,
        exit_code: exit_code.clone(),
        port: Arc::new(Mutex::new(registration.port)),
        killed: AtomicBool::new(false),
        health_failed: AtomicBool::new(false),
        restart: Mutex::new(None),
    });
    let registered = match registration.expected_generation {
        Some(expected) => registry.replace_current(expected, handle.clone()),
        None => registry.register_new(handle.clone()),
    };
    if !registered {
        // 注册失败立即回收新进程。不落 killed 行：start 行尚未落盘（注册成功才落），
        // 且同 id 可能另有在役 generation，一条不属于它的终态行会把它误判成已收口。
        registry.terminate_unjournaled(handle).await;
        let _ = child.kill().await;
        let _ = child.wait().await;
        return Err(ExecError::Spawn(format!("task changed while starting: {id}")));
    }

    // DCP intent：拿到 pid 且注册成功即落 start 行；恢复侧凭「最后一行 = start」判定中断。
    let sessions_dir = registry.sessions_dir().map(std::path::PathBuf::from);
    task_journal::append(sessions_dir.as_deref(), owner.session_id(), &TaskLine::start(id, display_command, workdir, pid));

    // 输出泵（合并 stdout/stderr 按到达顺序）
    if let Some(mut out) = stdout {
        let (output, truncated) = (output.clone(), truncated.clone());
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            while let Ok(n) = out.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                append_capped(&output, &truncated, &String::from_utf8_lossy(&buf[..n]), OUTPUT_CAP);
            }
        });
    }
    if let Some(mut err) = stderr {
        let (output, truncated) = (output.clone(), truncated.clone());
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            while let Ok(n) = err.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                append_capped(&output, &truncated, &String::from_utf8_lossy(&buf[..n]), OUTPUT_CAP);
            }
        });
    }

    // 退出收割
    let reaper = handle.clone();
    let owner_session = owner.session_id().to_string();
    tokio::spawn(async move {
        let status = child.wait().await;
        let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
        // 先落 exit 行再公开 exit_code：通知 watcher 看到退出码时终态已 durable
        // （persist-before-deliver，与 NotifyRouter 同模式）；且覆盖无 watcher 的合法
        // session 任务（子代理上下文无通知路由），否则它们的 start 行永远悬空、恢复误报。
        // killed 置位的退出由 terminate 落 killed 行，这里不重复。
        if !reaper.killed.load(Ordering::Relaxed) {
            task_journal::append(sessions_dir.as_deref(), &owner_session, &TaskLine::exit(&reaper.id, code));
        }
        *lock(&exit_code) = Some(code);
    });

    Ok(handle)
}
