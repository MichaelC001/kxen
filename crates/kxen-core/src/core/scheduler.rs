//! Target-neutral occurrence planning with IANA timezone and bounded misfire handling.

use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::core::identity::{ContentHash, ResourceId};

const MAX_SCAN_OCCURRENCES: usize = 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleExpression {
    Once { at_ms: u64 },
    Cron { expression: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MisfirePolicy {
    Skip,
    RunOnce,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleSpec {
    pub expression: ScheduleExpression,
    pub timezone: String,
    pub misfire: MisfirePolicy,
    pub max_lateness_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OccurrenceDecision {
    Run,
    Skip,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedOccurrence {
    pub occurrence_id: ResourceId,
    pub scheduled_at_ms: u64,
    pub observed_at_ms: u64,
    pub decision: OccurrenceDecision,
    pub missed_before: u32,
}

impl ScheduleSpec {
    pub fn validate(&self) -> Result<(), SchedulerError> {
        timezone(&self.timezone)?;
        match &self.expression {
            ScheduleExpression::Once { .. } => Ok(()),
            ScheduleExpression::Cron { expression } => parse_cron(expression).map(|_| ()),
        }
    }

    pub fn next_after(&self, after_ms: u64) -> Result<Option<u64>, SchedulerError> {
        let tz = timezone(&self.timezone)?;
        match &self.expression {
            ScheduleExpression::Once { at_ms } => Ok((*at_ms > after_ms).then_some(*at_ms)),
            ScheduleExpression::Cron { expression } => {
                let schedule = parse_cron(expression)?;
                let after = instant(after_ms)?.with_timezone(&tz);
                Ok(schedule.after(&after).next().map(|value| value.timestamp_millis() as u64))
            }
        }
    }

    pub fn plan(
        &self,
        schedule_id: &ResourceId,
        last_observed_at_ms: u64,
        observed_at_ms: u64,
    ) -> Result<Option<PlannedOccurrence>, SchedulerError> {
        self.validate()?;
        if observed_at_ms <= last_observed_at_ms {
            return Ok(None);
        }
        let due = self.due_between(last_observed_at_ms, observed_at_ms)?;
        let Some(&scheduled_at_ms) = due.last() else { return Ok(None) };
        let lateness = observed_at_ms.saturating_sub(scheduled_at_ms);
        let decision = if due.len() == 1 || (self.misfire == MisfirePolicy::RunOnce && lateness <= self.max_lateness_ms) {
            OccurrenceDecision::Run
        } else {
            OccurrenceDecision::Skip
        };
        let occurrence_id = occurrence_id(schedule_id, scheduled_at_ms)?;
        Ok(Some(PlannedOccurrence {
            occurrence_id,
            scheduled_at_ms,
            observed_at_ms,
            decision,
            missed_before: u32::try_from(due.len().saturating_sub(1)).unwrap_or(u32::MAX),
        }))
    }

    fn due_between(&self, after_ms: u64, through_ms: u64) -> Result<Vec<u64>, SchedulerError> {
        match &self.expression {
            ScheduleExpression::Once { at_ms } => Ok((after_ms < *at_ms && *at_ms <= through_ms).then_some(*at_ms).into_iter().collect()),
            ScheduleExpression::Cron { .. } => {
                let mut due = Vec::new();
                let mut cursor = after_ms;
                while let Some(next) = self.next_after(cursor)? {
                    if next > through_ms {
                        break;
                    }
                    due.push(next);
                    if due.len() >= MAX_SCAN_OCCURRENCES {
                        return Err(SchedulerError::ScanLimit);
                    }
                    cursor = next;
                }
                Ok(due)
            }
        }
    }
}

pub fn occurrence_id(schedule_id: &ResourceId, scheduled_at_ms: u64) -> Result<ResourceId, SchedulerError> {
    let hash = ContentHash::from_bytes(format!("{}:{scheduled_at_ms}", schedule_id.as_str()).as_bytes());
    ResourceId::parse(format!("occ_{}", &hash.as_str()["sha256:".len()..])).map_err(SchedulerError::Invalid)
}

fn timezone(value: &str) -> Result<Tz, SchedulerError> {
    value.parse().map_err(|_| SchedulerError::Invalid(format!("unknown IANA timezone: {value}")))
}

fn parse_cron(value: &str) -> Result<cron::Schedule, SchedulerError> {
    let normalized = if value.split_whitespace().count() == 5 { format!("0 {value}") } else { value.to_string() };
    normalized.parse().map_err(|error| SchedulerError::Invalid(format!("invalid cron expression: {error}")))
}

fn instant(ms: u64) -> Result<DateTime<Utc>, SchedulerError> {
    let seconds = i64::try_from(ms / 1000).map_err(|_| SchedulerError::Invalid("timestamp overflow".into()))?;
    Utc.timestamp_opt(seconds, ((ms % 1000) * 1_000_000) as u32).single().ok_or_else(|| SchedulerError::Invalid("invalid timestamp".into()))
}

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("schedule is invalid: {0}")]
    Invalid(String),
    #[error("schedule occurrence scan exceeded bounded limit")]
    ScanLimit,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: &str) -> u64 {
        DateTime::parse_from_rfc3339(value).unwrap().timestamp_millis() as u64
    }

    #[test]
    fn cron_uses_iana_timezone_across_dst() {
        let spec = ScheduleSpec {
            expression: ScheduleExpression::Cron { expression: "30 2 * * *".into() },
            timezone: "America/New_York".into(),
            misfire: MisfirePolicy::Skip,
            max_lateness_ms: 60_000,
        };
        let before = ms("2026-03-08T06:00:00Z");
        let next = spec.next_after(before).unwrap().unwrap();
        assert_eq!(next, ms("2026-03-09T06:30:00Z"), "nonexistent DST local time must not be fabricated");
    }

    #[test]
    fn duplicate_tick_and_run_once_misfire_are_bounded() {
        let spec = ScheduleSpec {
            expression: ScheduleExpression::Cron { expression: "* * * * *".into() },
            timezone: "UTC".into(),
            misfire: MisfirePolicy::RunOnce,
            max_lateness_ms: 120_000,
        };
        let routine_id = ResourceId::parse("routine_one").unwrap();
        let plan = spec.plan(&routine_id, 0, 180_000).unwrap().unwrap();
        assert_eq!(plan.decision, OccurrenceDecision::Run);
        assert_eq!(plan.missed_before, 2);
        assert!(spec.plan(&routine_id, 180_000, 180_000).unwrap().is_none());
    }

    #[test]
    fn skip_misfire_records_latest_without_unbounded_backfill() {
        let spec = ScheduleSpec {
            expression: ScheduleExpression::Cron { expression: "* * * * *".into() },
            timezone: "UTC".into(),
            misfire: MisfirePolicy::Skip,
            max_lateness_ms: 60_000,
        };
        let plan = spec.plan(&ResourceId::parse("routine_skip").unwrap(), 0, 300_000).unwrap().unwrap();
        assert_eq!(plan.decision, OccurrenceDecision::Skip);
        assert_eq!(plan.missed_before, 4);
    }
}
