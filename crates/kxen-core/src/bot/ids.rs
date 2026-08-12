use crate::core::identity::{ContentHash, ResourceId};

pub(crate) fn deterministic_id(prefix: &str, parts: &[&str]) -> Result<ResourceId, String> {
    let mut bytes = Vec::new();
    for part in parts {
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part.as_bytes());
    }
    let hash = ContentHash::from_bytes(&bytes);
    ResourceId::parse(format!("{prefix}_{}", &hash.as_str()["sha256:".len()..]))
}
