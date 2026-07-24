//! 后台任务注册表（任务三件套的后端 + dev_server 健康检查）。

use crate::core::shared::{SharedStr, lock};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::process::Child;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Running,
    Exited,
    Killed,
    Failed,
}

pub struct TaskHandle {
    pub id: String,
    pub command: SharedStr,
    pub workdir: SharedStr,
    pub output: Arc<Mutex<String>>,
    pub truncated: Arc<Mutex<bool>>,
    pub started_at: u64,
    pub pid: Option<u32>,
    pub exit_code: Arc<Mutex<Option<i32>>>,
    pub child: Arc<Mutex<Option<Child>>>,
    /// readiness 解析出的 port 会后写（spawn 时没有）：共享槽，list/health 读现值
    pub port: Arc<Mutex<Option<u16>>>,
    /// kill() 终止标记：kill 的退出码（-1/143）与自身失败同形，没有它 status 会把 Killed 误报成 Failed
    pub killed: AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskInfo {
    pub id: String,
    #[serde(serialize_with = "serialize_shared")]
    pub command: SharedStr,
    pub status: TaskStatus,
    pub uptime_ms: u64,
    pub port: Option<u16>,
    pub tail: String,
}

#[derive(Default)]
pub struct TaskRegistry {
    tasks: Mutex<HashMap<String, Arc<TaskHandle>>>,
}

impl TaskHandle {
    pub fn status(&self) -> TaskStatus {
        match *self.exit_code.lock().expect("exit") {
            // kill 的退出码（-1/143）与自身失败同形，须靠 killed 标记区分
            Some(_) if self.killed.load(Ordering::Relaxed) => TaskStatus::Killed,
            Some(0) => TaskStatus::Exited,
            Some(_) => TaskStatus::Failed,
            None => TaskStatus::Running,
        }
    }
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, handle: Arc<TaskHandle>) {
        lock(&self.tasks).insert(handle.id.clone(), handle);
    }

    pub fn get(&self, id: &str) -> Option<Arc<TaskHandle>> {
        lock(&self.tasks).get(id).cloned()
    }

    pub fn list(&self) -> Vec<TaskInfo> {
        let now = now_ms();
        lock(&self.tasks)
            .values()
            .map(|t| TaskInfo {
                id: t.id.clone(),
                command: t.command.clone(),
                status: t.status(),
                uptime_ms: now.saturating_sub(t.started_at),
                port: *lock(&t.port),
                tail: tail_of(&t.output.lock().expect("output"), 400),
            })
            .collect()
    }

    pub fn output(&self, id: &str) -> Option<(String, bool, TaskStatus)> {
        let task = self.get(id)?;
        let output = lock(&task.output).clone();
        let truncated = *lock(&task.truncated);
        Some((output, truncated, task.status()))
    }

    /// 进程组终止：SIGTERM -> 800ms 宽限 -> SIGKILL 升级（spawn 时 process_group(0) 组长，组覆盖孙进程）。
    pub async fn kill(&self, id: &str) -> bool {
        let Some(task) = self.get(id) else { return false };
        // 只给仍在运行的任务打标记：已自行退出的保持 Exited/Failed 原判定
        if task.exit_code.lock().expect("exit").is_none() {
            task.killed.store(true, Ordering::Relaxed);
        }
        if let Some(pid) = task.pid {
            let _ = std::process::Command::new("kill").args(["-TERM", &format!("-{pid}")]).status();
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(800);
            loop {
                let alive =
                    std::process::Command::new("kill").args(["-0", &pid.to_string()]).status().map(|s| s.success()).unwrap_or(false);
                if !alive || std::time::Instant::now() >= deadline {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(60)).await;
            }
        }
        let taken = lock(&task.child).take();
        if let Some(pid) = task.pid {
            let _ = std::process::Command::new("kill").args(["-KILL", &format!("-{pid}")]).status();
        }
        if let Some(mut child) = taken {
            let _ = child.kill().await;
        }
        true
    }
}

pub fn tail_of(output: &str, max: usize) -> String {
    if output.len() <= max {
        return output.to_string();
    }
    output[output.floor_char_boundary(output.len() - max)..].to_string()
}

pub fn append_capped(output: &Arc<Mutex<String>>, truncated: &Arc<Mutex<bool>>, chunk: &str, cap: usize) {
    let mut out = output.lock().expect("output");
    out.push_str(chunk);
    if out.len() > cap {
        let cut = out.floor_char_boundary(out.len() - cap / 2);
        *out = out[cut..].to_string();
        *truncated.lock().expect("truncated") = true;
    }
}

fn serialize_shared<S: serde::Serializer>(value: &SharedStr, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(value)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn task_id() -> String {
    crate::core::ids::new_id("task")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_crops() {
        assert_eq!(tail_of("abcdef", 3), "def");
        assert_eq!(tail_of("abc", 10), "abc");
    }

    #[test]
    fn append_caps() {
        let out = Arc::new(Mutex::new(String::new()));
        let trunc = Arc::new(Mutex::new(false));
        append_capped(&out, &trunc, &"x".repeat(100), 60);
        assert!(lock(&out).len() <= 60);
        assert!(*lock(&trunc));
    }

    #[tokio::test]
    async fn killed_task_reports_killed_not_failed() {
        let registry = Arc::new(TaskRegistry::new());
        let id =
            crate::tools::exec::spawn_task(vec!["sleep".into(), "30".into()], "sleep 30", "/tmp", &registry, None).await.expect("spawn");
        assert!(registry.kill(&id).await);
        let task = registry.get(&id).expect("task");
        for _ in 0..100 {
            if task.status() != TaskStatus::Running {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(task.status(), TaskStatus::Killed, "被 kill 的任务不得误报 Failed");
    }

    #[tokio::test]
    async fn self_exit_failure_stays_failed() {
        let registry = Arc::new(TaskRegistry::new());
        let id = crate::tools::exec::spawn_task(vec!["false".into()], "false", "/tmp", &registry, None).await.expect("spawn");
        let task = registry.get(&id).expect("task");
        for _ in 0..100 {
            if task.status() != TaskStatus::Running {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(task.status(), TaskStatus::Failed, "自行非零退出保持 Failed，不得误报 Killed");
    }
}
