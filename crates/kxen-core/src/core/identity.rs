//! 跨领域 identity value objects，避免 aggregate、actor、run 和 hash 在接口中退化成裸字符串。

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ResourceId(String);

impl ResourceId {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        crate::core::ids::validate_id(&value)?;
        Ok(Self(value))
    }

    pub fn new(prefix: &str) -> Result<Self, String> {
        Self::parse(crate::core::ids::new_id(prefix))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ResourceId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl AsRef<str> for ResourceId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateKind {
    Session,
    Team,
    KanbanBoard,
    Goal,
    Bot,
    BotRevision,
    BotRun,
    Conversation,
    CollaborationTask,
    Routine,
    BotMemory,
    Artifact,
    BuilderSession,
    Recovery,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AggregateRef {
    pub kind: AggregateKind,
    pub id: ResourceId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemActor {
    Runtime,
    Scheduler,
    Recovery,
    Builder,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActorRef {
    Owner,
    Bot { id: ResourceId },
    Agent { scope: AggregateRef, id: ResourceId },
    System { actor: SystemActor },
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionKey {
    pub owner: AggregateRef,
    pub run_id: ResourceId,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TraceContext {
    pub causation_id: Option<ResourceId>,
    pub correlation_id: Option<ResourceId>,
    pub parent_operation_id: Option<ResourceId>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ContentHash(String);

impl ContentHash {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        use sha2::Digest as _;
        let digest = sha2::Sha256::digest(bytes);
        Self(format!("sha256:{}", crate::core::shared::hex_lower(&digest)))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("sha256:") else { return Err("content hash must use sha256 prefix".into()) };
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
            return Err("content hash must contain 64 lowercase hexadecimal characters".into());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct IdempotencyKey(ResourceId);

impl IdempotencyKey {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        ResourceId::parse(value).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl<'de> Deserialize<'de> for IdempotencyKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Sequence(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SchemaVersion(u32);

impl SchemaVersion {
    pub fn new(value: u32) -> Result<Self, String> {
        (value > 0).then_some(Self(value)).ok_or_else(|| "schema version must be greater than zero".into())
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "identity/tests.rs"]
mod tests;
