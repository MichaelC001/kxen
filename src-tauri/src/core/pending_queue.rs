//! pending queue 持久化（P1-13）：排队消息落 `<sessions_dir>/<id>.queue.json`，崩溃重启可恢复续跑。
//! 内存 map 是运行时真相，每次变更把该 session 队列整写到盘（tmp + rename 原子替换）：
//! 队列短小，O(n) 重写换来单一口径——恢复逻辑只需读文件，不需合并磁盘与内存两路状态。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedMessage {
    pub text: String,
    #[serde(default)]
    pub context: Vec<crate::agent::context::ContextItem>,
    #[serde(default)]
    pub images: Vec<crate::llm::types::ImagePart>,
}

pub struct PendingQueues {
    dir: PathBuf,
    map: std::sync::Mutex<HashMap<String, VecDeque<QueuedMessage>>>,
}

/// queue 文件路径（session.rs remove 随 meta/jsonl 一并清理，属同一会话生命周期）。
pub fn file_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.queue.json"))
}

impl PendingQueues {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            map: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// 内存状态整写到盘；空队列删文件（残留空文件会被 restore 当有效队列）。
    fn persist(&self, id: &str) {
        let snapshot: Vec<QueuedMessage> = crate::core::shared::lock(&self.map)
            .get(id)
            .map(|q| q.iter().cloned().collect())
            .unwrap_or_default();
        let path = file_path(&self.dir, id);
        if snapshot.is_empty() {
            let _ = std::fs::remove_file(&path);
            return;
        }
        let tmp = path.with_extension("json.tmp");
        let Ok(text) = serde_json::to_string_pretty(&snapshot) else {
            return;
        };
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    /// 入队并落盘，返回该 session 排队总数（通知文案用）。id 不合法直接拒（防路径穿越）。
    pub fn enqueue(
        &self,
        id: &str,
        text: String,
        context: Vec<crate::agent::context::ContextItem>,
        images: Vec<crate::llm::types::ImagePart>,
    ) -> usize {
        if crate::core::ids::validate_id(id).is_err() {
            return 0;
        }
        let n = {
            let mut map = crate::core::shared::lock(&self.map);
            let q = map.entry(id.to_string()).or_default();
            q.push_back(QueuedMessage {
                text,
                context,
                images,
            });
            q.len()
        };
        self.persist(id);
        n
    }

    /// 弹出队首（消费后重写磁盘）。pop 即删盘上行条目：崩溃窗口内丢一条与旧纯内存行为等价，不引入重复消费。
    pub fn pop(&self, id: &str) -> Option<QueuedMessage> {
        let item = crate::core::shared::lock(&self.map)
            .get_mut(id)?
            .pop_front();
        if item.is_some() {
            self.persist(id);
        }
        item
    }

    /// 清空该 session 队列（abort/delete 用），返回清掉条数。
    pub fn clear(&self, id: &str) -> usize {
        let n = crate::core::shared::lock(&self.map)
            .remove(id)
            .map(|q| q.len())
            .unwrap_or(0);
        let _ = std::fs::remove_file(file_path(&self.dir, id));
        n
    }

    pub fn texts(&self, id: &str) -> Vec<String> {
        crate::core::shared::lock(&self.map)
            .get(id)
            .map(|q| q.iter().map(|m| m.text.clone()).collect())
            .unwrap_or_default()
    }

    pub fn has_queued(&self, id: &str) -> bool {
        crate::core::shared::lock(&self.map)
            .get(id)
            .is_some_and(|q| !q.is_empty())
    }

    /// 启动恢复：读全部 queue 文件进内存，返回有待跑消息的 session id（调用方据此续跑）。
    /// 坏文件跳过不删：可能是另一版本写的新格式，留给人工处置比静默丢消息安全。
    pub fn restore(&self) -> Vec<String> {
        let mut ready = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return ready;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(id) = name.strip_suffix(".queue.json") else {
                continue;
            };
            if crate::core::ids::validate_id(id).is_err() {
                continue;
            }
            let items: Vec<QueuedMessage> = std::fs::read_to_string(entry.path())
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
            if items.is_empty() {
                continue;
            }
            crate::core::shared::lock(&self.map)
                .insert(id.to_string(), items.into_iter().collect());
            ready.push(id.to_string());
        }
        ready
    }
}
