//! cron 定时任务存储（进程内，随 app 存活；一次性/周期）。tick 由宿主循环驱动。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    /// 5 字段 cron（分 时 日 月 周）
    pub cron: String,
    pub prompt: String,
    pub session_id: String,
    /// 一次性：触发后即删
    pub once: bool,
    /// 下次触发（epoch ms，创建时算好，触发后重算）
    pub next_fire: u64,
}

static JOBS: std::sync::Mutex<Vec<CronJob>> = std::sync::Mutex::new(Vec::new());

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 解析 cron 并算下一次触发（本地时区）。cron crate 需秒位：5 字段标准 crontab 自动补 0 秒。
pub fn next_fire_of(expr: &str, after_ms: u64) -> Result<u64, String> {
    let normalized = match expr.split_whitespace().count() {
        5 => format!("0 {expr}"),
        _ => expr.to_string(),
    };
    let schedule = normalized.parse::<cron::Schedule>().map_err(|e| format!("cron 表达式无效: {e}"))?;
    let after = chrono_from_ms(after_ms);
    schedule
        .after(&after)
        .next()
        .map(|t| (t.timestamp_millis()) as u64)
        .ok_or_else(|| "cron 无可触发时间".to_string())
}

fn chrono_from_ms(ms: u64) -> chrono::DateTime<chrono::Local> {
    let secs = (ms / 1000) as i64;
    let utc = chrono::DateTime::from_timestamp(secs, ((ms % 1000) * 1_000_000) as u32).unwrap_or_default();
    utc.with_timezone(&chrono::Local)
}

pub fn add(cron: &str, prompt: &str, session_id: &str, once: bool) -> Result<CronJob, String> {
    let next_fire = next_fire_of(cron, now_ms())?;
    let job = CronJob {
        id: format!("cron-{}-{:04x}", now_ms(), std::process::id() & 0xffff),
        cron: cron.to_string(),
        prompt: prompt.to_string(),
        session_id: session_id.to_string(),
        once,
        next_fire,
    };
    crate::core::shared::lock(&JOBS).push(job.clone());
    Ok(job)
}

pub fn list() -> Vec<CronJob> {
    crate::core::shared::lock(&JOBS).clone()
}

pub fn remove(id: &str) -> bool {
    let mut jobs = crate::core::shared::lock(&JOBS);
    let before = jobs.len();
    jobs.retain(|j| j.id != id);
    jobs.len() != before
}

#[cfg(test)]
pub fn clear() {
    crate::core::shared::lock(&JOBS).clear();
}

/// 到期任务出列（once 删除；周期任务就地重算下次）。
pub fn drain_due(now: u64) -> Vec<CronJob> {
    let mut jobs = crate::core::shared::lock(&JOBS);
    let mut due = Vec::new();
    let mut i = 0;
    while i < jobs.len() {
        if jobs[i].next_fire <= now {
            let job = jobs[i].clone();
            due.push(job.clone());
            if job.once {
                jobs.remove(i);
                continue;
            }
            match next_fire_of(&job.cron, now) {
                Ok(nf) => jobs[i].next_fire = nf,
                Err(_) => {
                    jobs.remove(i);
                    continue;
                }
            }
        }
        i += 1;
    }
    due
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn cron_parse_and_next() {
        let nf = next_fire_of("*/1 * * * *", 0).unwrap();
        assert!(nf > 0);
        assert!(next_fire_of("not a cron", 0).is_err());
    }

    #[test]
    fn once_drains_and_removes() {
        let _g = crate::core::shared::lock(&TEST_LOCK);
        clear();
        let job = add("*/1 * * * *", "ping", "s1", true).unwrap();
        let due = drain_due(job.next_fire + 1);
        assert!(due.iter().any(|j| j.id == job.id));
        assert!(list().iter().all(|j| j.id != job.id), "once 应触发后删除");
    }

    #[test]
    fn recurring_reschedules() {
        let _g = crate::core::shared::lock(&TEST_LOCK);
        clear();
        let job = add("*/1 * * * *", "ping", "s2", false).unwrap();
        let due = drain_due(job.next_fire + 1);
        assert!(due.iter().any(|j| j.id == job.id));
        let after = list().into_iter().find(|j| j.id == job.id).unwrap();
        assert!(after.next_fire > job.next_fire);
        remove(&job.id);
    }
}
