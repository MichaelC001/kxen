//! Durable direct and moderated multi-Bot collaboration substrate.

mod command;
mod decision;
mod error;
mod events;
mod projection;
mod repository;
mod routing;
mod types;

pub use command::ConversationCommand;
pub use error::ConversationError;
pub use repository::{ConversationRepository, ConversationWrite};
pub use types::{
    BotParticipant, CollaborationTask, ConversationKind, ConversationLifecycle, ConversationState, Message, MessageDelivery, MessageKind,
    MessagePart, NewTask, TaskStatus,
};

pub fn direct_conversation_id(
    left: &crate::core::identity::ResourceId,
    right: &crate::core::identity::ResourceId,
) -> Result<crate::core::identity::ResourceId, String> {
    let (left, right) = if left <= right { (left, right) } else { (right, left) };
    crate::bot::ids::deterministic_id("bconv", &["owner", left.as_str(), right.as_str()])
}

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod tests;
