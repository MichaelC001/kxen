use std::collections::BTreeMap;

use crate::core::delivery::ClaimToken;
use crate::core::identity::ResourceId;

use super::types::{BotParticipant, ConversationKind, Message, MessagePart, NewTask, TaskStatus};

#[derive(Clone, Debug)]
pub enum ConversationCommand {
    Create {
        conversation_id: ResourceId,
        kind: ConversationKind,
        members: Vec<BotParticipant>,
        moderator_bot_id: Option<ResourceId>,
        at_ms: u64,
    },
    Post {
        message: Box<Message>,
        task: Option<NewTask>,
        at_ms: u64,
    },
    ClaimDelivery {
        generation: ResourceId,
        at_ms: u64,
    },
    AcknowledgeDelivery {
        token: ClaimToken,
        run_ids: BTreeMap<ResourceId, ResourceId>,
        at_ms: u64,
    },
    ReleaseDelivery {
        token: ClaimToken,
        at_ms: u64,
    },
    RejectDelivery {
        delivery_id: ResourceId,
        generation: Option<ResourceId>,
        reason: String,
        at_ms: u64,
    },
    ChangeTask {
        task_id: ResourceId,
        status: TaskStatus,
        result: Vec<MessagePart>,
        at_ms: u64,
    },
    ReassignTask {
        task_id: ResourceId,
        owner_bot_id: ResourceId,
        at_ms: u64,
    },
    AddMember {
        participant: BotParticipant,
        at_ms: u64,
    },
    RemoveMember {
        bot_id: ResourceId,
        at_ms: u64,
    },
    SetModerator {
        bot_id: ResourceId,
        at_ms: u64,
    },
    Pause {
        at_ms: u64,
    },
    Resume {
        at_ms: u64,
    },
    Archive {
        at_ms: u64,
    },
    Reopen {
        at_ms: u64,
    },
    Block {
        reason: String,
        at_ms: u64,
    },
}
