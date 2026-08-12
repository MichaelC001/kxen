use serde::{Deserialize, Serialize};

use crate::core::delivery::DeliveryEvent;
use crate::core::identity::ResourceId;

use super::types::{BotParticipant, CollaborationTask, ConversationKind, Message, MessageDelivery, TaskStatus};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationEvent {
    Created {
        conversation_id: ResourceId,
        kind: ConversationKind,
        members: Vec<BotParticipant>,
        moderator_bot_id: Option<ResourceId>,
        at_ms: u64,
    },
    ParticipantAdded {
        participant: BotParticipant,
        at_ms: u64,
    },
    ParticipantRemoved {
        bot_id: ResourceId,
        at_ms: u64,
    },
    ModeratorChanged {
        bot_id: ResourceId,
        at_ms: u64,
    },
    MessageAppended {
        message: Message,
        at_ms: u64,
    },
    Delivery {
        event: DeliveryEvent<MessageDelivery>,
        at_ms: u64,
    },
    DeliveryRunLinked {
        delivery_id: ResourceId,
        run_id: ResourceId,
        at_ms: u64,
    },
    TaskCreated {
        task: CollaborationTask,
        at_ms: u64,
    },
    TaskStatusChanged {
        task_id: ResourceId,
        status: TaskStatus,
        result: Vec<super::types::MessagePart>,
        at_ms: u64,
    },
    TaskReassigned {
        task_id: ResourceId,
        owner_bot_id: ResourceId,
        at_ms: u64,
    },
    Paused {
        at_ms: u64,
    },
    Resumed {
        at_ms: u64,
    },
    Archived {
        at_ms: u64,
    },
    Blocked {
        reason: String,
        at_ms: u64,
    },
}

impl ConversationEvent {
    pub fn at_ms(&self) -> u64 {
        match self {
            Self::Created { at_ms, .. }
            | Self::ParticipantAdded { at_ms, .. }
            | Self::ParticipantRemoved { at_ms, .. }
            | Self::ModeratorChanged { at_ms, .. }
            | Self::MessageAppended { at_ms, .. }
            | Self::Delivery { at_ms, .. }
            | Self::DeliveryRunLinked { at_ms, .. }
            | Self::TaskCreated { at_ms, .. }
            | Self::TaskStatusChanged { at_ms, .. }
            | Self::TaskReassigned { at_ms, .. }
            | Self::Paused { at_ms }
            | Self::Resumed { at_ms }
            | Self::Archived { at_ms }
            | Self::Blocked { at_ms, .. } => *at_ms,
        }
    }
}
