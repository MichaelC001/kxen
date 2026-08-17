//! workflow journal：agent 派发按 run_id 落盘，同 run_id 重跑自动跳过已完成项（resume）。
//! 文件：data_dir/workflow-journals/<run_id>.jsonl（每行 {key, phase: started|done, result?, ts}；
//! phase 缺省按 done 解析，兼容 intent 引入前的旧条目）。
//! dispatch 前先落 started intent，成功后落 done 把 intent 转为完成：dispatch 与 record 之间崩溃，
//! 重开时 intent 在、result 无 = Unknown，fail closed 报副作用不可知，绝不静默重复派发。
//! 命名空间隔离：ns = sha256(run_id, sha256(script))，
//! key = sha256(ns, role, prompt, label, occurrence)。同输入的多次独立调用不会互相冒充；
//! 同 run_id 换了脚本语义全变，旧条目必须 miss；脚本哈希进 ns 让缓存自动失效。

use sha2::Digest;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// 条目 TTL 7 天：resume 场景是「崩溃/取消后接着跑」，跨度按天计；
/// 超期条目命中率趋零却无限涨盘，且脚本产物时效性丧失，留之无益。
const ENTRY_TTL_SECS: u64 = 7 * 24 * 3600;
const JOURNAL_SCHEMA: u64 = 2;

fn journal_file(run_id: &str) -> PathBuf {
    crate::core::paths::KxenPaths::user().workflow_journal(run_id)
}

/// 测试断言/清理用：open_scoped 的 session 派生命名空间对应的 journal 文件路径。
#[cfg(test)]
pub(crate) fn scoped_journal_file(session_id: Option<&str>, run_id: &str) -> PathBuf {
    journal_file(&stable_hash(&[session_id.unwrap_or("no-session"), run_id]))
}

/// 多段稳定哈希：段间写 0 分隔符（hex 输出无 0 字节，拼接防 ("ab","c") 与 ("a","bc") 撞车）。
fn stable_hash(segments: &[&str]) -> String {
    let mut h = sha2::Sha256::new();
    for seg in segments {
        h.update(seg.as_bytes());
        h.update([0u8]);
    }
    crate::core::shared::hex_lower(&h.finalize())
}

pub struct Journal {
    ns: String,
    done: HashMap<String, String>,
    /// 已落盘但未完成的 durable intent（started 无 done）：resume 时按 Unknown fail closed。
    pending: HashSet<String>,
    file: PathBuf,
    /// 同一 scoped run_id 同时只能有一个执行者；File lock 跨进程，Drop 自动释放。
    _lock: File,
}

impl Journal {
    /// run_id 会拼进 journal 文件路径：非法 id（路径穿越）直接返回 None（放弃 resume）。
    /// 打开即清理超 TTL 条目；损坏或旧格式行 fail closed 并保留原文件，避免把已完成派发误判为未执行。
    pub fn open(run_id: &str, script: &str) -> Result<Self, String> {
        crate::core::ids::validate_id(run_id)?;
        let ns = stable_hash(&[run_id, &stable_hash(&[script])]);
        let file = journal_file(run_id);
        let lock = lock_journal(&file)?;
        let now = now_secs();
        let mut done = HashMap::new();
        let mut pending = HashSet::new();
        let mut kept_lines: Vec<String> = Vec::new();
        let mut dropped = false;
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(format!("read workflow journal {}: {error}", file.display())),
        };
        for (index, line) in text.lines().enumerate() {
            let entry: serde_json::Value = serde_json::from_str(line)
                .map_err(|error| format!("parse workflow journal {} line {}: {error}", file.display(), index + 1))?;
            let schema = entry
                .get("schema")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| format!("workflow journal {} line {} uses an unsupported legacy schema", file.display(), index + 1))?;
            if schema != JOURNAL_SCHEMA {
                return Err(format!("workflow journal {} line {} has unsupported schema {schema}", file.display(), index + 1));
            }
            entry
                .get("occurrence")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| format!("workflow journal {} line {} has no occurrence", file.display(), index + 1))?;
            let key = entry
                .get("key")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("workflow journal {} line {} has no key", file.display(), index + 1))?;
            // phase 缺省 = done：intent 引入前的旧条目只有完成记录，无需迁移即可继续 resume
            let phase = entry.get("phase").and_then(serde_json::Value::as_str).unwrap_or("done");
            if phase != "done" && phase != "started" {
                return Err(format!("workflow journal {} line {} has unsupported phase {phase}", file.display(), index + 1));
            }
            let ts = entry
                .get("ts")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| format!("workflow journal {} line {} has no timestamp", file.display(), index + 1))?;
            if now.saturating_sub(ts) <= ENTRY_TTL_SECS {
                if phase == "done" {
                    let result = entry
                        .get("result")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| format!("workflow journal {} line {} has no result", file.display(), index + 1))?;
                    pending.remove(key);
                    done.insert(key.to_string(), result.to_string());
                } else {
                    pending.insert(key.to_string());
                }
                kept_lines.push(line.to_string());
            } else {
                dropped = true;
            }
        }
        if dropped {
            rewrite_journal(&file, &kept_lines)?;
        }
        Ok(Self { ns, done, pending, file, _lock: lock })
    }

    /// 宿主命名空间版 open：模型传入的
    /// run_id 只作当前会话内的 resume 令牌，真实 journal id = sha256(session, run_id)——
    /// 跨会话/历史同 run_id 不再命中旧 journal 跳过真实派发；同会话同脚本重跑仍可断点续跑。
    pub fn open_scoped(session_id: Option<&str>, run_id: &str, script: &str) -> Result<Self, String> {
        let scoped = stable_hash(&[session_id.unwrap_or("no-session"), run_id]);
        Self::open(&scoped, script)
    }

    /// 已完成的派发结果（resume 命中则免重跑）。
    pub fn cached(&self, role: &str, prompt: &str, label: Option<&str>, occurrence: u32) -> Option<&String> {
        self.done.get(&self.key(role, prompt, label, occurrence))
    }

    /// 单步派发的 durable 状态（resume 判定用）。
    pub fn state(&self, role: &str, prompt: &str, label: Option<&str>, occurrence: u32) -> DispatchState<'_> {
        let key = self.key(role, prompt, label, occurrence);
        if let Some(result) = self.done.get(&key) {
            return DispatchState::Done(result);
        }
        if self.pending.contains(&key) {
            return DispatchState::Unknown;
        }
        DispatchState::Miss
    }

    /// resume 闸门（判定与 intent 落盘在同一调用内完成）：Done 回缓存；Unknown fail closed，
    /// 报错指明该步副作用不可知、换新 run_id 显式重跑；Miss 先落 durable intent 再放行——
    /// dispatch 与 record 之间的崩溃由此转为可判定的 Unknown，而不是静默重复派发。
    pub fn resume_gate(&mut self, role: &str, prompt: &str, label: Option<&str>, occurrence: u32) -> Result<Option<String>, String> {
        match self.state(role, prompt, label, occurrence) {
            DispatchState::Done(result) => Ok(Some(result.clone())),
            DispatchState::Unknown => Err(format!(
                "workflow dispatch outcome unknown for role {role}: intent persisted without result \
                 (previous run crashed between dispatch and record); refusing silent re-dispatch, retry with a fresh run_id"
            )),
            DispatchState::Miss => self.begin(role, prompt, label, occurrence).map(|_| None),
        }
    }

    /// dispatch 前的 durable intent（立即落盘）：崩溃后重开见 intent 无 result = Unknown。
    /// 落盘失败必须拦住 dispatch——副作用先于持久化就违背了 intent 存在的意义。
    pub fn begin(&mut self, role: &str, prompt: &str, label: Option<&str>, occurrence: u32) -> Result<(), String> {
        let key = self.key(role, prompt, label, occurrence);
        let line = serde_json::json!({
            "schema": JOURNAL_SCHEMA,
            "key": key,
            "occurrence": occurrence,
            "phase": "started",
            "ts": now_secs(),
        });
        self.append_entry(&line)?;
        self.pending.insert(key);
        Ok(())
    }

    /// 追加一条完成记录（立即落盘，崩溃可续），把同 key 的 intent 转为 done。
    pub fn record(&mut self, role: &str, prompt: &str, label: Option<&str>, occurrence: u32, result: &str) -> Result<(), String> {
        let key = self.key(role, prompt, label, occurrence);
        let line = serde_json::json!({
            "schema": JOURNAL_SCHEMA,
            "key": key,
            "occurrence": occurrence,
            "phase": "done",
            "result": result,
            "ts": now_secs(),
        });
        let outcome = self.append_entry(&line);
        // 目录 sync 失败只上抛错误：行已 append，内存照常登记（重开仍可见，语义与引入 intent 前一致）
        self.pending.remove(&key);
        self.done.insert(key, result.to_string());
        outcome
    }

    pub fn completed(&self) -> usize {
        self.done.len()
    }

    fn key(&self, role: &str, prompt: &str, label: Option<&str>, occurrence: u32) -> String {
        let occurrence = occurrence.to_string();
        stable_hash(&[&self.ns, role, prompt, label.unwrap_or(""), &occurrence])
    }

    fn append_entry(&self, line: &serde_json::Value) -> Result<(), String> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)
            .map_err(|error| format!("open {}: {error}", self.file.display()))?;
        writeln!(file, "{line}").map_err(|error| format!("append {}: {error}", self.file.display()))?;
        file.sync_data().map_err(|error| format!("sync {}: {error}", self.file.display()))?;
        let parent = self.file.parent().ok_or_else(|| format!("journal path has no parent: {}", self.file.display()))?;
        sync_journal_directory(parent).map_err(|error| format!("sync {}: {error}", parent.display()))
    }
}

/// 单步派发的 durable 状态。
pub enum DispatchState<'a> {
    /// result 已落盘：resume 直接回缓存，不重派。
    Done(&'a String),
    /// intent 在、result 无：上次 dispatch 与 record 之间崩溃（或 dispatch 失败），副作用不可知。
    Unknown,
    Miss,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn lock_journal(path: &std::path::Path) -> Result<File, String> {
    let parent = path.parent().ok_or_else(|| format!("journal path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    let lock_path = path.with_extension("jsonl.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|error| format!("open workflow lock {}: {error}", lock_path.display()))?;
    lock.try_lock().map_err(|error| format!("workflow run already active for {}: {error}", path.display()))?;
    Ok(lock)
}

fn rewrite_journal(path: &std::path::Path, lines: &[String]) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| format!("journal path has no parent: {}", path.display()))?;
    let tmp = path.with_extension("jsonl.tmp");
    let text = lines.join("\n") + if lines.is_empty() { "" } else { "\n" };
    let mut output = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)
        .map_err(|error| format!("open {}: {error}", tmp.display()))?;
    output.write_all(text.as_bytes()).map_err(|error| format!("write {}: {error}", tmp.display()))?;
    output.sync_all().map_err(|error| format!("sync {}: {error}", tmp.display()))?;
    drop(output);
    std::fs::rename(&tmp, path).map_err(|error| {
        std::fs::remove_file(&tmp).ok();
        format!("replace {}: {error}", path.display())
    })?;
    sync_journal_directory(parent).map_err(|error| format!("sync {}: {error}", parent.display()))?;
    Ok(())
}

#[cfg(unix)]
fn sync_journal_directory(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(test)]
    if FAIL_NEXT_JOURNAL_DIRECTORY_SYNC.with(|flag| flag.replace(false)) {
        return Err(std::io::Error::other("injected workflow journal directory sync failure"));
    }
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_journal_directory(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_JOURNAL_DIRECTORY_SYNC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
mod tests;
