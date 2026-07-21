//! 后台任务注册表（任务三件套的后端 + dev_server 健康检查）。

use kxen_core::shared::{lock, SharedStr};
use serde::Serialize;
use std::collections::HashMap;
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
    pub port: Option<u16>,
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
                port: t.port,
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

    pub async fn kill(&self, id: &str) -> bool {
        let Some(task) = self.get(id) else { return false };
        let taken = lock(&task.child).take();
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
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn task_id() -> String {
    format!("task_{}_{:06x}", now_ms(), rand_u32())
}

fn rand_u32() -> u32 {
    use std::time::SystemTime;
    let nanos = SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    nanos ^ (std::process::id() << 16)
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
}
