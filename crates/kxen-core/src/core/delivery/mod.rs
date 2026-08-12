//! Repository-neutral durable delivery transition model.

mod types;

pub use types::{
    ClaimMode, ClaimToken, DeliveryCommand, DeliveryDecision, DeliveryEnvelope, DeliveryError, DeliveryEvent, DeliveryProjection,
    DeliveryRecord, DeliveryStatus, DeliveryTombstone, QueuePosition,
};

use serde::Serialize;

impl<P: Clone + Serialize> DeliveryProjection<P> {
    pub fn decide(&self, command: DeliveryCommand<P>) -> Result<DeliveryDecision<P>, DeliveryError> {
        match command {
            DeliveryCommand::Enqueue { envelope, position } => self.decide_enqueue(envelope, position),
            DeliveryCommand::Claim { mode, generation } => self.decide_claim(mode, generation),
            DeliveryCommand::Acknowledge { token } => {
                self.verify_claim(&token)?;
                Ok(decision(DeliveryEvent::Acknowledged { token }, None))
            }
            DeliveryCommand::Release { token } => {
                self.verify_claim(&token)?;
                Ok(decision(DeliveryEvent::Released { token }, None))
            }
            DeliveryCommand::Reject { delivery_id, generation, reason } => {
                self.verify_terminal_target(&delivery_id, generation.as_ref())?;
                Ok(decision(DeliveryEvent::Rejected { delivery_id, generation, reason }, None))
            }
            DeliveryCommand::Block { delivery_id, reason } => {
                if !self.records.contains_key(&delivery_id) {
                    return Err(DeliveryError::NotFound(delivery_id.to_string()));
                }
                Ok(decision(DeliveryEvent::Blocked { delivery_id, reason }, None))
            }
        }
    }

    pub fn apply(&mut self, event: DeliveryEvent<P>) -> Result<(), DeliveryError> {
        match event {
            DeliveryEvent::Enqueued { envelope, position } => self.apply_enqueued(envelope, position)?,
            DeliveryEvent::Claimed { token } => self.apply_claimed(token)?,
            DeliveryEvent::Acknowledged { token } => self.apply_acknowledged(token)?,
            DeliveryEvent::Released { token } => self.apply_released(token)?,
            DeliveryEvent::Rejected { delivery_id, generation, reason: _ } => {
                self.apply_rejected(delivery_id, generation.as_ref())?;
            }
            DeliveryEvent::Blocked { delivery_id, reason } => self.apply_blocked(delivery_id, reason)?,
        }
        self.version = self.version.checked_add(1).ok_or_else(|| DeliveryError::InvalidTransition("version overflow".into()))?;
        Ok(())
    }

    fn decide_enqueue(&self, envelope: DeliveryEnvelope<P>, position: QueuePosition) -> Result<DeliveryDecision<P>, DeliveryError> {
        envelope.verify()?;
        if let Some(existing) = self.records.get(&envelope.delivery_id) {
            return duplicate_or_collision(&envelope, &existing.envelope.payload_fingerprint);
        }
        if let Some(existing) = self.tombstones.iter().find(|item| item.delivery_id == envelope.delivery_id) {
            return duplicate_or_collision(&envelope, &existing.payload_fingerprint);
        }
        Ok(decision(DeliveryEvent::Enqueued { envelope, position }, None))
    }

    fn decide_claim(&self, mode: ClaimMode, generation: crate::core::identity::ResourceId) -> Result<DeliveryDecision<P>, DeliveryError> {
        if let Some(existing) = &self.in_flight {
            return Ok(DeliveryDecision { events: Vec::new(), claim: Some(existing.clone()), duplicate: true });
        }
        let limit = match mode {
            ClaimMode::One => 1,
            ClaimMode::Batch { limit: 0 } => return Err(DeliveryError::EmptyClaim),
            ClaimMode::Batch { limit } => limit,
        };
        let delivery_ids = self.queued.iter().take(limit).cloned().collect::<Vec<_>>();
        if delivery_ids.is_empty() {
            return Ok(DeliveryDecision { events: Vec::new(), claim: None, duplicate: false });
        }
        let token = ClaimToken { generation, delivery_ids };
        Ok(decision(DeliveryEvent::Claimed { token: token.clone() }, Some(token)))
    }

    fn verify_claim(&self, token: &ClaimToken) -> Result<(), DeliveryError> {
        if self.in_flight.as_ref() != Some(token) {
            return Err(DeliveryError::StaleClaim);
        }
        Ok(())
    }

    fn verify_terminal_target(
        &self,
        delivery_id: &crate::core::identity::ResourceId,
        generation: Option<&crate::core::identity::ResourceId>,
    ) -> Result<(), DeliveryError> {
        let record = self.records.get(delivery_id).ok_or_else(|| DeliveryError::NotFound(delivery_id.to_string()))?;
        if record.status == DeliveryStatus::InFlight && self.in_flight.as_ref().map(|claim| &claim.generation) != generation {
            return Err(DeliveryError::StaleClaim);
        }
        Ok(())
    }

    fn apply_enqueued(&mut self, envelope: DeliveryEnvelope<P>, position: QueuePosition) -> Result<(), DeliveryError> {
        envelope.verify()?;
        if self.records.contains_key(&envelope.delivery_id) || self.tombstones.iter().any(|item| item.delivery_id == envelope.delivery_id) {
            return Err(DeliveryError::IdCollision(envelope.delivery_id.to_string()));
        }
        let id = envelope.delivery_id.clone();
        self.records.insert(id.clone(), DeliveryRecord { envelope, status: DeliveryStatus::Queued, blocked_reason: None });
        match position {
            QueuePosition::Front => self.queued.push_front(id),
            QueuePosition::Back => self.queued.push_back(id),
        }
        Ok(())
    }

    fn apply_claimed(&mut self, token: ClaimToken) -> Result<(), DeliveryError> {
        if self.in_flight.is_some() || token.delivery_ids.is_empty() {
            return Err(DeliveryError::InvalidTransition("claim requires no existing in-flight delivery".into()));
        }
        let expected = self.queued.iter().take(token.delivery_ids.len()).collect::<Vec<_>>();
        if expected != token.delivery_ids.iter().collect::<Vec<_>>() {
            return Err(DeliveryError::InvalidTransition("claim must match the queue head".into()));
        }
        for id in &token.delivery_ids {
            self.queued.pop_front();
            self.records.get_mut(id).ok_or_else(|| DeliveryError::NotFound(id.to_string()))?.status = DeliveryStatus::InFlight;
        }
        self.in_flight = Some(token);
        Ok(())
    }

    fn apply_acknowledged(&mut self, token: ClaimToken) -> Result<(), DeliveryError> {
        self.verify_claim(&token)?;
        self.in_flight = None;
        for id in token.delivery_ids {
            let record = self.records.remove(&id).ok_or_else(|| DeliveryError::NotFound(id.to_string()))?;
            self.push_tombstone(record, DeliveryStatus::Acked);
        }
        Ok(())
    }

    fn apply_released(&mut self, token: ClaimToken) -> Result<(), DeliveryError> {
        self.verify_claim(&token)?;
        self.in_flight = None;
        for id in token.delivery_ids.iter().rev() {
            self.records.get_mut(id).ok_or_else(|| DeliveryError::NotFound(id.to_string()))?.status = DeliveryStatus::Queued;
            self.queued.push_front(id.clone());
        }
        Ok(())
    }

    fn apply_rejected(
        &mut self,
        delivery_id: crate::core::identity::ResourceId,
        generation: Option<&crate::core::identity::ResourceId>,
    ) -> Result<(), DeliveryError> {
        self.verify_terminal_target(&delivery_id, generation)?;
        self.remove_from_active(&delivery_id);
        let record = self.records.remove(&delivery_id).ok_or_else(|| DeliveryError::NotFound(delivery_id.to_string()))?;
        self.push_tombstone(record, DeliveryStatus::Rejected);
        Ok(())
    }

    fn apply_blocked(&mut self, delivery_id: crate::core::identity::ResourceId, reason: String) -> Result<(), DeliveryError> {
        self.remove_from_active(&delivery_id);
        let record = self.records.get_mut(&delivery_id).ok_or_else(|| DeliveryError::NotFound(delivery_id.to_string()))?;
        record.status = DeliveryStatus::Blocked;
        record.blocked_reason = Some(reason);
        Ok(())
    }

    fn remove_from_active(&mut self, delivery_id: &crate::core::identity::ResourceId) {
        self.queued.retain(|id| id != delivery_id);
        if let Some(claim) = &mut self.in_flight {
            claim.delivery_ids.retain(|id| id != delivery_id);
            if claim.delivery_ids.is_empty() {
                self.in_flight = None;
            }
        }
    }

    fn push_tombstone(&mut self, record: DeliveryRecord<P>, status: DeliveryStatus) {
        if self.tombstone_limit == 0 {
            return;
        }
        self.tombstones.push_back(DeliveryTombstone {
            delivery_id: record.envelope.delivery_id,
            payload_fingerprint: record.envelope.payload_fingerprint,
            status,
        });
        while self.tombstones.len() > self.tombstone_limit {
            self.tombstones.pop_front();
        }
    }
}

fn decision<P>(event: DeliveryEvent<P>, claim: Option<ClaimToken>) -> DeliveryDecision<P> {
    DeliveryDecision { events: vec![event], claim, duplicate: false }
}

fn duplicate_or_collision<P>(
    envelope: &DeliveryEnvelope<P>,
    existing: &crate::core::identity::ContentHash,
) -> Result<DeliveryDecision<P>, DeliveryError> {
    if &envelope.payload_fingerprint == existing {
        Ok(DeliveryDecision { events: Vec::new(), claim: None, duplicate: true })
    } else {
        Err(DeliveryError::IdCollision(envelope.delivery_id.to_string()))
    }
}

#[cfg(test)]
mod tests;
