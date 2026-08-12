mod artifact;
mod definitions;
mod helpers;
mod memory;
mod message;
mod task;

use std::sync::Arc;

use crate::agent::domain_tool::{DomainToolFuture, DomainToolRouter};
use crate::core::identity::ResourceId;

pub struct BotToolRouter {
    system: Arc<crate::bot::system::BotSystem>,
    run_id: ResourceId,
}

impl BotToolRouter {
    pub fn new(system: Arc<crate::bot::system::BotSystem>, run_id: ResourceId) -> Self {
        Self { system, run_id }
    }
}

impl DomainToolRouter for BotToolRouter {
    fn definitions(&self) -> Vec<crate::llm::tool::ToolDefinition> {
        definitions::all()
    }

    fn handles(&self, name: &str) -> bool {
        matches!(name, "bot_message" | "bot_task" | "bot_memory" | "bot_artifact")
    }

    fn execute<'a>(&'a self, name: &'a str, args: &'a serde_json::Value) -> DomainToolFuture<'a> {
        Box::pin(async move {
            match name {
                "bot_message" => message::execute(&self.system, &self.run_id, args),
                "bot_task" => task::execute(&self.system, &self.run_id, args),
                "bot_memory" => memory::execute(&self.system, &self.run_id, args),
                "bot_artifact" => artifact::execute(&self.system, &self.run_id, args),
                _ => Err(format!("unknown Bot domain tool: {name}")),
            }
        })
    }
}

pub fn definitions() -> Vec<crate::llm::tool::ToolDefinition> {
    definitions::all()
}

#[cfg(test)]
mod tests;
