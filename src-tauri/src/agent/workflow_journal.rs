//! workflow journal：agent 派发结果按 run_id 落盘，同 run_id 重跑自动跳过已完成项（resume）。
//! 文件：data_dir/workflow-journals/<run_id>.jsonl（每行 {key, result}，key = role+prompt 哈希）。

use std::collections::HashMap;
use std::path::PathBuf;

fn journal_file(run_id: &str) -> PathBuf {
    crate::core::paths::data_dir().join("workflow-journals").join(format!("{run_id}.jsonl"))
}

fn key_of(role: &str, prompt: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    role.hash(&mut h);
    prompt.hash(&mut h);
    format!("{:x}", h.finish())
}

pub struct Journal {
    done: HashMap<String, String>,
    file: PathBuf,
}

impl Journal {
    pub fn open(run_id: &str) -> Self {
        let file = journal_file(run_id);
        let done = std::fs::read_to_string(&file)
            .ok()
            .map(|text| {
                text.lines()
                    .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                    .filter_map(|v| Some((v.get("key")?.as_str()?.to_string(), v.get("result")?.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        Self { done, file }
    }

    /// 已完成的派发结果（resume 命中则免重跑）。
    pub fn cached(&self, role: &str, prompt: &str) -> Option<&String> {
        self.done.get(&key_of(role, prompt))
    }

    /// 追加一条完成记录（立即落盘，崩溃可续）。
    pub fn record(&mut self, role: &str, prompt: &str, result: &str) {
        use std::io::Write;
        let key = key_of(role, prompt);
        if let Some(parent) = self.file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.file) {
            let line = serde_json::json!({ "key": key, "result": result });
            let _ = writeln!(f, "{line}");
        }
        self.done.insert(key, result.to_string());
    }

    pub fn completed(&self) -> usize {
        self.done.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_resume_hit() {
        let run_id = format!("test-{}", std::process::id());
        let file = journal_file(&run_id);
        let _ = std::fs::remove_file(&file);
        {
            let mut j = Journal::open(&run_id);
            assert_eq!(j.completed(), 0);
            j.record("execution", "do A", "result A");
        }
        // 重新打开（模拟崩溃后 resume）：命中缓存
        let j2 = Journal::open(&run_id);
        assert_eq!(j2.completed(), 1);
        assert_eq!(j2.cached("execution", "do A").map(String::as_str), Some("result A"));
        assert_eq!(j2.cached("execution", "do B"), None);
        let _ = std::fs::remove_file(&file);
    }
}
