//! Durable intent, side effect, outcome and settlement transition protocol.

mod types;

pub use types::{
    AttemptPhase, EvidenceRef, OperationCommand, OperationDecision, OperationError, OperationEvent, OperationOutcome, OperationProjection,
    SideEffectAttempt,
};

use serde::Serialize;

use crate::core::identity::ContentHash;

impl<I: Clone + Serialize, O: Clone + PartialEq> OperationProjection<I, O> {
    pub fn decide(&self, command: OperationCommand<I, O>) -> Result<OperationDecision<I, O>, OperationError> {
        match command {
            OperationCommand::Prepare { operation_id, generation, intent, at_ms } => {
                let intent_hash = intent_hash(&intent)?;
                if let Some(attempt) = &self.attempt {
                    if attempt.operation_id == operation_id && attempt.generation == generation && attempt.intent_hash == intent_hash {
                        return Ok(duplicate());
                    }
                    return Err(OperationError::Collision(operation_id.to_string()));
                }
                Ok(decision(OperationEvent::Prepared { operation_id, generation, intent, intent_hash, at_ms }))
            }
            OperationCommand::MarkStarted { generation, at_ms } => {
                let attempt = self.current(&generation)?;
                match attempt.phase {
                    AttemptPhase::Prepared => Ok(decision(OperationEvent::Started { generation, at_ms })),
                    AttemptPhase::Started => Ok(duplicate()),
                    phase => Err(invalid(phase, "started")),
                }
            }
            OperationCommand::RecordOutcome { generation, outcome, evidence } => {
                let attempt = self.current(&generation)?;
                match attempt.phase {
                    AttemptPhase::Started | AttemptPhase::OutcomeUnknown => {
                        Ok(decision(OperationEvent::OutcomeRecorded { generation, outcome, evidence }))
                    }
                    AttemptPhase::OutcomeKnown if attempt.outcome.as_ref() == Some(&outcome) && attempt.evidence == evidence => {
                        Ok(duplicate())
                    }
                    phase => Err(invalid(phase, "outcome_known")),
                }
            }
            OperationCommand::MarkOutcomeUnknown { generation, reason, evidence } => {
                let attempt = self.current(&generation)?;
                match attempt.phase {
                    AttemptPhase::Started => Ok(decision(OperationEvent::OutcomeMarkedUnknown { generation, reason, evidence })),
                    AttemptPhase::OutcomeUnknown if attempt.unknown_reason.as_deref() == Some(&reason) && attempt.evidence == evidence => {
                        Ok(duplicate())
                    }
                    phase => Err(invalid(phase, "outcome_unknown")),
                }
            }
            OperationCommand::Settle { generation, at_ms } => {
                let attempt = self.current(&generation)?;
                match attempt.phase {
                    AttemptPhase::OutcomeKnown => Ok(decision(OperationEvent::Settled { generation, at_ms })),
                    AttemptPhase::Settled => Ok(duplicate()),
                    phase => Err(invalid(phase, "settled")),
                }
            }
            OperationCommand::CancelBeforeStart { generation, at_ms } => {
                let attempt = self.current(&generation)?;
                match attempt.phase {
                    AttemptPhase::Prepared => Ok(decision(OperationEvent::CanceledBeforeStart { generation, at_ms })),
                    AttemptPhase::CanceledBeforeStart => Ok(duplicate()),
                    phase => Err(invalid(phase, "canceled_before_start")),
                }
            }
        }
    }

    pub fn apply(&mut self, event: OperationEvent<I, O>) -> Result<(), OperationError> {
        match event {
            OperationEvent::Prepared { operation_id, generation, intent, intent_hash: stored_hash, at_ms } => {
                if self.attempt.is_some() {
                    return Err(OperationError::Collision(operation_id.to_string()));
                }
                if intent_hash(&intent)? != stored_hash {
                    return Err(OperationError::IntentHashMismatch);
                }
                self.attempt = Some(SideEffectAttempt {
                    operation_id,
                    generation,
                    intent,
                    intent_hash: stored_hash,
                    phase: AttemptPhase::Prepared,
                    prepared_at_ms: at_ms,
                    started_at_ms: None,
                    outcome: None,
                    evidence: Vec::new(),
                    unknown_reason: None,
                    settled_at_ms: None,
                });
            }
            OperationEvent::Started { generation, at_ms } => {
                let attempt = self.current_mut(&generation)?;
                require_phase(attempt.phase, AttemptPhase::Prepared, "started")?;
                attempt.phase = AttemptPhase::Started;
                attempt.started_at_ms = Some(at_ms);
            }
            OperationEvent::OutcomeRecorded { generation, outcome, evidence } => {
                let attempt = self.current_mut(&generation)?;
                if !matches!(attempt.phase, AttemptPhase::Started | AttemptPhase::OutcomeUnknown) {
                    return Err(invalid(attempt.phase, "outcome_known"));
                }
                attempt.phase = AttemptPhase::OutcomeKnown;
                attempt.outcome = Some(outcome);
                attempt.evidence = evidence;
                attempt.unknown_reason = None;
            }
            OperationEvent::OutcomeMarkedUnknown { generation, reason, evidence } => {
                let attempt = self.current_mut(&generation)?;
                require_phase(attempt.phase, AttemptPhase::Started, "outcome_unknown")?;
                attempt.phase = AttemptPhase::OutcomeUnknown;
                attempt.unknown_reason = Some(reason);
                attempt.evidence = evidence;
            }
            OperationEvent::Settled { generation, at_ms } => {
                let attempt = self.current_mut(&generation)?;
                require_phase(attempt.phase, AttemptPhase::OutcomeKnown, "settled")?;
                attempt.phase = AttemptPhase::Settled;
                attempt.settled_at_ms = Some(at_ms);
            }
            OperationEvent::CanceledBeforeStart { generation, at_ms } => {
                let attempt = self.current_mut(&generation)?;
                require_phase(attempt.phase, AttemptPhase::Prepared, "canceled_before_start")?;
                attempt.phase = AttemptPhase::CanceledBeforeStart;
                attempt.settled_at_ms = Some(at_ms);
            }
        }
        self.version = self.version.checked_add(1).ok_or(OperationError::VersionOverflow)?;
        Ok(())
    }

    fn current(&self, generation: &crate::core::identity::ResourceId) -> Result<&SideEffectAttempt<I, O>, OperationError> {
        let attempt = self.attempt.as_ref().ok_or(OperationError::Missing)?;
        if &attempt.generation != generation {
            return Err(OperationError::StaleGeneration);
        }
        Ok(attempt)
    }

    fn current_mut(&mut self, generation: &crate::core::identity::ResourceId) -> Result<&mut SideEffectAttempt<I, O>, OperationError> {
        let attempt = self.attempt.as_mut().ok_or(OperationError::Missing)?;
        if &attempt.generation != generation {
            return Err(OperationError::StaleGeneration);
        }
        Ok(attempt)
    }
}

fn intent_hash(intent: &impl Serialize) -> Result<ContentHash, OperationError> {
    serde_json::to_vec(intent).map(|bytes| ContentHash::from_bytes(&bytes)).map_err(|error| OperationError::Codec(error.to_string()))
}

fn require_phase(actual: AttemptPhase, expected: AttemptPhase, target: &'static str) -> Result<(), OperationError> {
    if actual == expected { Ok(()) } else { Err(invalid(actual, target)) }
}

fn invalid(from: AttemptPhase, to: &'static str) -> OperationError {
    OperationError::InvalidTransition { from, to }
}

fn decision<I, O>(event: OperationEvent<I, O>) -> OperationDecision<I, O> {
    OperationDecision { events: vec![event], duplicate: false }
}

fn duplicate<I, O>() -> OperationDecision<I, O> {
    OperationDecision { events: Vec::new(), duplicate: true }
}

#[cfg(test)]
mod tests;
