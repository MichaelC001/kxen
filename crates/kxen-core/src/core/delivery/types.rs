use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

use crate::core::identity::{ActorRef, ContentHash, ResourceId};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryEnvelope<P> {
    pub delivery_id: ResourceId,
    pub recipient: ActorRef,
    pub created_at_ms: u64,
    pub payload_fingerprint: ContentHash,
    pub payload: P,
}

impl<P: Serialize> DeliveryEnvelope<P> {
    pub fn new(delivery_id: ResourceId, recipient: ActorRef, created_at_ms: u64, payload: P) -> Result<Self, DeliveryError> {
        let payload_fingerprint = fingerprint(&recipient, &payload)?;
        Ok(Self { delivery_id, recipient, created_at_ms, payload_fingerprint, payload })
    }

    pub(super) fn verify(&self) -> Result<(), DeliveryError> {
        if fingerprint(&self.recipient, &self.payload)? != self.payload_fingerprint {
            return Err(DeliveryError::FingerprintMismatch(self.delivery_id.to_string()));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Queued,
    InFlight,
    Acked,
    Rejected,
    Blocked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuePosition {
    Front,
    Back,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimMode {
    One,
    Batch { limit: usize },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimToken {
    pub generation: ResourceId,
    pub delivery_ids: Vec<ResourceId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryRecord<P> {
    pub envelope: DeliveryEnvelope<P>,
    pub status: DeliveryStatus,
    pub blocked_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryTombstone {
    pub delivery_id: ResourceId,
    pub payload_fingerprint: ContentHash,
    pub status: DeliveryStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryProjection<P> {
    pub version: u64,
    pub records: BTreeMap<ResourceId, DeliveryRecord<P>>,
    pub queued: VecDeque<ResourceId>,
    pub in_flight: Option<ClaimToken>,
    pub tombstones: VecDeque<DeliveryTombstone>,
    pub tombstone_limit: usize,
}

impl<P> DeliveryProjection<P> {
    pub fn new(tombstone_limit: usize) -> Self {
        Self {
            version: 0,
            records: BTreeMap::new(),
            queued: VecDeque::new(),
            in_flight: None,
            tombstones: VecDeque::new(),
            tombstone_limit,
        }
    }
}

#[derive(Clone, Debug)]
pub enum DeliveryCommand<P> {
    Enqueue { envelope: DeliveryEnvelope<P>, position: QueuePosition },
    Claim { mode: ClaimMode, generation: ResourceId },
    Acknowledge { token: ClaimToken },
    Release { token: ClaimToken },
    Reject { delivery_id: ResourceId, generation: Option<ResourceId>, reason: String },
    Block { delivery_id: ResourceId, reason: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeliveryEvent<P> {
    Enqueued { envelope: DeliveryEnvelope<P>, position: QueuePosition },
    Claimed { token: ClaimToken },
    Acknowledged { token: ClaimToken },
    Released { token: ClaimToken },
    Rejected { delivery_id: ResourceId, generation: Option<ResourceId>, reason: String },
    Blocked { delivery_id: ResourceId, reason: String },
}

#[derive(Clone, Debug)]
pub struct DeliveryDecision<P> {
    pub events: Vec<DeliveryEvent<P>>,
    pub claim: Option<ClaimToken>,
    pub duplicate: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    #[error("delivery codec: {0}")]
    Codec(String),
    #[error("delivery fingerprint mismatch: {0}")]
    FingerprintMismatch(String),
    #[error("delivery id collision: {0}")]
    IdCollision(String),
    #[error("delivery not found: {0}")]
    NotFound(String),
    #[error("delivery claim limit must be greater than zero")]
    EmptyClaim,
    #[error("delivery claim token is stale")]
    StaleClaim,
    #[error("invalid delivery transition: {0}")]
    InvalidTransition(String),
}

#[derive(Serialize)]
struct FingerprintBody<'a, P> {
    recipient: &'a ActorRef,
    payload: &'a P,
}

fn fingerprint<P: Serialize>(recipient: &ActorRef, payload: &P) -> Result<ContentHash, DeliveryError> {
    serde_json::to_vec(&FingerprintBody { recipient, payload })
        .map(|bytes| ContentHash::from_bytes(&bytes))
        .map_err(|error| DeliveryError::Codec(error.to_string()))
}
