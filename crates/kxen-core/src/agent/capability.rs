//! Runtime capability catalog and closed capability sets.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::core::identity::ResourceId;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    Tool,
    Skill,
    Mcp,
    Connector,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAvailability {
    Available,
    Disabled,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDescriptor {
    pub id: ResourceId,
    pub kind: CapabilityKind,
    pub display_name: String,
    pub availability: CapabilityAvailability,
    pub requires_approval: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CapabilitySet(BTreeSet<ResourceId>);

impl CapabilitySet {
    pub fn new(ids: impl IntoIterator<Item = ResourceId>) -> Self {
        Self(ids.into_iter().collect())
    }

    pub fn allows(&self, id: &ResourceId) -> bool {
        self.0.contains(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ResourceId> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub struct CapabilityCatalog {
    entries: BTreeMap<ResourceId, CapabilityDescriptor>,
}

impl CapabilityCatalog {
    pub fn register(&mut self, descriptor: CapabilityDescriptor) -> Result<(), CapabilityError> {
        if descriptor.display_name.trim().is_empty() {
            return Err(CapabilityError::InvalidDescriptor(descriptor.id.to_string()));
        }
        if self.entries.insert(descriptor.id.clone(), descriptor).is_some() {
            return Err(CapabilityError::Duplicate);
        }
        Ok(())
    }

    pub fn get(&self, id: &ResourceId) -> Option<&CapabilityDescriptor> {
        self.entries.get(id)
    }

    pub fn resolve(&self, requested: &CapabilitySet) -> Result<Vec<CapabilityDescriptor>, CapabilityError> {
        requested
            .iter()
            .map(|id| {
                let descriptor = self.entries.get(id).ok_or_else(|| CapabilityError::Unknown(id.to_string()))?;
                if descriptor.availability != CapabilityAvailability::Available {
                    return Err(CapabilityError::Unavailable(id.to_string()));
                }
                Ok(descriptor.clone())
            })
            .collect()
    }

    pub fn available_set(&self) -> CapabilitySet {
        CapabilitySet::new(
            self.entries.values().filter(|entry| entry.availability == CapabilityAvailability::Available).map(|entry| entry.id.clone()),
        )
    }

    pub fn descriptors(&self) -> impl Iterator<Item = &CapabilityDescriptor> {
        self.entries.values()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    #[error("duplicate capability")]
    Duplicate,
    #[error("invalid capability descriptor: {0}")]
    InvalidDescriptor(String),
    #[error("unknown capability: {0}")]
    Unknown(String),
    #[error("capability is unavailable: {0}")]
    Unavailable(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(id: &str, availability: CapabilityAvailability) -> CapabilityDescriptor {
        CapabilityDescriptor {
            id: ResourceId::parse(id).unwrap(),
            kind: CapabilityKind::Tool,
            display_name: id.into(),
            availability,
            requires_approval: false,
        }
    }

    #[test]
    fn resolution_is_closed_and_fails_for_unavailable_items() {
        let mut catalog = CapabilityCatalog::default();
        catalog.register(descriptor("read", CapabilityAvailability::Available)).unwrap();
        catalog.register(descriptor("write", CapabilityAvailability::Disabled)).unwrap();
        let read = CapabilitySet::new([ResourceId::parse("read").unwrap()]);
        assert_eq!(catalog.resolve(&read).unwrap().len(), 1);
        let write = CapabilitySet::new([ResourceId::parse("write").unwrap()]);
        assert!(matches!(catalog.resolve(&write), Err(CapabilityError::Unavailable(_))));
        let invented = CapabilitySet::new([ResourceId::parse("invented").unwrap()]);
        assert!(matches!(catalog.resolve(&invented), Err(CapabilityError::Unknown(_))));
    }
}
