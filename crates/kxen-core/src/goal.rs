//! Goal 生命周期：状态机 + 预算 + 阻塞三次规则 + 持久化。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Draft,
    Queued,
    Active,
    Paused,
    Complete,
    Blocked,
    BudgetLimited,
    Canceled,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GoalBudget {
    pub tokens: Option<u64>,
    pub turns: Option<u32>,
    pub wall_clock_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalContract {
    pub objective: String,
    pub completion_criteria: String,
    #[serde(default)]
    pub constraints: Option<String>,
    #[serde(default)]
    pub budget: GoalBudget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub contract: GoalContract,
    pub status: GoalStatus,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub activated_at: Option<u64>,
    #[serde(default)]
    pub turns_used: u32,
    #[serde(default)]
    pub tokens_used: u64,
    #[serde(default)]
    pub last_block_reason: Option<String>,
    #[serde(default)]
    pub consecutive_blocks: u32,
    #[serde(default)]
    pub block_reason: Option<String>,
    #[serde(default)]
    pub verification_evidence: Option<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum GoalError {
    #[error("invalid transition: {from:?} -> {to:?}")]
    InvalidTransition { from: GoalStatus, to: GoalStatus },
    #[error("contract incomplete: {0}")]
    ContractIncomplete(&'static str),
    #[error("goal not found: {0}")]
    NotFound(String),
}

fn transitions(from: GoalStatus) -> &'static [GoalStatus] {
    use GoalStatus::*;
    match from {
        Draft => &[Queued, Active, Canceled],
        Queued => &[Active, Canceled],
        Active => &[Paused, Complete, Blocked, BudgetLimited, Canceled],
        Paused => &[Active, Canceled],
        Blocked => &[Active, Canceled],
        BudgetLimited => &[Active, Canceled],
        Complete | Canceled => &[],
    }
}

impl Goal {
    pub fn create(contract: GoalContract, id: String) -> Result<Self, GoalError> {
        if contract.objective.trim().is_empty() {
            return Err(GoalError::ContractIncomplete("objective is required"));
        }
        if contract.completion_criteria.trim().is_empty() {
            return Err(GoalError::ContractIncomplete("completion_criteria is required"));
        }
        let now = now_ms();
        Ok(Self {
            id,
            contract,
            status: GoalStatus::Draft,
            created_at: now,
            updated_at: now,
            activated_at: None,
            turns_used: 0,
            tokens_used: 0,
            last_block_reason: None,
            consecutive_blocks: 0,
            block_reason: None,
            verification_evidence: None,
        })
    }

    fn transit(&mut self, to: GoalStatus) -> Result<(), GoalError> {
        if !transitions(self.status).contains(&to) {
            return Err(GoalError::InvalidTransition { from: self.status, to });
        }
        if to == GoalStatus::Active && self.activated_at.is_none() {
            self.activated_at = Some(now_ms());
        }
        self.status = to;
        self.updated_at = now_ms();
        Ok(())
    }

    pub fn activate(&mut self) -> Result<(), GoalError> {
        self.transit(GoalStatus::Active)
    }

    pub fn pause(&mut self) -> Result<(), GoalError> {
        self.transit(GoalStatus::Paused)
    }

    pub fn resume(&mut self) -> Result<(), GoalError> {
        self.transit(GoalStatus::Active)
    }

    pub fn cancel(&mut self) -> Result<(), GoalError> {
        self.transit(GoalStatus::Canceled)
    }

    pub fn complete(&mut self, evidence: &str) -> Result<(), GoalError> {
        if evidence.trim().is_empty() {
            return Err(GoalError::ContractIncomplete("completion requires verification evidence"));
        }
        self.verification_evidence = Some(evidence.to_string());
        self.transit(GoalStatus::Complete)
    }

    /// 记录一轮推进；预算与阻塞三次规则在此。
    pub fn record_turn(&mut self, tokens: u64, blocked_reason: Option<&str>, terminal: bool) -> Result<(), GoalError> {
        if self.status != GoalStatus::Active {
            return Err(GoalError::InvalidTransition { from: self.status, to: GoalStatus::Active });
        }
        self.turns_used += 1;
        self.tokens_used += tokens;
        self.updated_at = now_ms();

        let b = &self.contract.budget;
        if b.turns.is_some_and(|t| self.turns_used >= t)
            || b.tokens.is_some_and(|t| self.tokens_used >= t)
            || (b.wall_clock_ms.is_some()
                && self.activated_at.is_some_and(|a| now_ms() - a >= b.wall_clock_ms.unwrap_or(u64::MAX)))
        {
            return self.transit(GoalStatus::BudgetLimited);
        }

        if let Some(reason) = blocked_reason {
            let same = self.last_block_reason.as_deref() == Some(reason);
            self.consecutive_blocks = if same { self.consecutive_blocks + 1 } else { 1 };
            self.last_block_reason = Some(reason.to_string());
            if terminal || self.consecutive_blocks >= 3 {
                self.block_reason = Some(reason.to_string());
                return self.transit(GoalStatus::Blocked);
            }
        } else {
            self.consecutive_blocks = 0;
            self.last_block_reason = None;
        }
        Ok(())
    }

    // --- 持久化 ---

    pub fn save(&self, dir: &std::path::Path) -> crate::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.json", self.id));
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn load(dir: &std::path::Path, id: &str) -> Result<Self, GoalError> {
        let path = dir.join(format!("{id}.json"));
        let text = std::fs::read_to_string(&path).map_err(|_| GoalError::NotFound(id.to_string()))?;
        serde_json::from_str(&text).map_err(|_| GoalError::ContractIncomplete("corrupt goal file"))
    }

    pub fn list(dir: &std::path::Path) -> Vec<Self> {
        let mut out: Vec<Self> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok())
            .collect();
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        out
    }
    /// 当前焦点 goal（active/paused/blocked/budget_limited 中最近更新的一个），用于状态注入与 GUI 焦点显示。
    pub fn focus(dir: &std::path::Path) -> Option<Self> {
        Self::list(dir)
            .into_iter()
            .find(|g| matches!(g.status, GoalStatus::Active | GoalStatus::Paused | GoalStatus::Blocked | GoalStatus::BudgetLimited))
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> GoalContract {
        GoalContract {
            objective: "迁移完成".into(),
            completion_criteria: "测试全绿".into(),
            constraints: None,
            budget: GoalBudget { tokens: Some(1000), turns: Some(5), wall_clock_ms: None },
        }
    }

    #[test]
    fn lifecycle() {
        let mut g = Goal::create(contract(), "g1".into()).unwrap();
        assert_eq!(g.status, GoalStatus::Draft);
        g.activate().unwrap();
        g.pause().unwrap();
        g.resume().unwrap();
        g.complete("1074 pass").unwrap();
        assert_eq!(g.status, GoalStatus::Complete);
    }

    #[test]
    fn blocked_after_three_same_reasons() {
        let mut g = Goal::create(contract(), "g2".into()).unwrap();
        g.activate().unwrap();
        for _ in 0..2 {
            g.record_turn(0, Some("网络不可达"), false).unwrap();
            assert_eq!(g.status, GoalStatus::Active);
        }
        g.record_turn(0, Some("网络不可达"), false).unwrap();
        assert_eq!(g.status, GoalStatus::Blocked);
    }

    #[test]
    fn budget_limited() {
        let mut g = Goal::create(contract(), "g3".into()).unwrap();
        g.activate().unwrap();
        for _ in 0..5 {
            g.record_turn(0, None, false).unwrap();
        }
        assert_eq!(g.status, GoalStatus::BudgetLimited);
    }

    #[test]
    fn persist_roundtrip() {
        let dir = std::env::temp_dir().join(format!("kxen-goal-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut g = Goal::create(contract(), "gx".into()).unwrap();
        g.activate().unwrap();
        g.save(&dir).unwrap();
        let loaded = Goal::load(&dir, "gx").unwrap();
        assert_eq!(loaded.status, GoalStatus::Active);
        assert!(Goal::list(&dir).iter().any(|x| x.id == "gx"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
