use std::future::Future;
use std::pin::Pin;

use crate::llm::tool::ToolDefinition;

pub type DomainToolFuture<'a> = Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;

pub trait DomainToolRouter: Send + Sync {
    fn definitions(&self) -> Vec<ToolDefinition>;
    fn handles(&self, name: &str) -> bool;
    fn execute<'a>(&'a self, name: &'a str, args: &'a serde_json::Value) -> DomainToolFuture<'a>;
}
