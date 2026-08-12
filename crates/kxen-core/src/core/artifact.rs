//! Immutable content-addressed artifacts with typed manifests and explicit sharing.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::core::identity::{ActorRef, AggregateKind, AggregateRef, ContentHash, ResourceId};

pub const MAX_ARTIFACT_BYTES: usize = 25 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactManifest {
    pub artifact_id: ResourceId,
    pub owner: AggregateRef,
    pub display_name: String,
    pub media_type: String,
    pub content_hash: ContentHash,
    pub size_bytes: u64,
    pub shared_with_conversations: BTreeSet<ResourceId>,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug)]
pub struct ArtifactAccess {
    pub actor: ActorRef,
    pub conversation_id: Option<ResourceId>,
}

pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn commit(&self, request: CommitArtifact<'_>) -> Result<ArtifactManifest, ArtifactError> {
        validate_metadata(request.display_name, request.media_type, request.content)?;
        let content_hash = ContentHash::from_bytes(request.content);
        let size_bytes = u64::try_from(request.content.len()).map_err(|_| ArtifactError::Invalid("artifact size overflow".into()))?;
        let manifest = ArtifactManifest {
            artifact_id: request.artifact_id.clone(),
            owner: request.owner,
            display_name: request.display_name.to_string(),
            media_type: request.media_type.to_string(),
            content_hash,
            size_bytes,
            shared_with_conversations: request.shared_with_conversations,
            created_at_ms: request.created_at_ms,
        };
        let directory = self.directory(request.artifact_id);
        if self.trashed_directory(request.artifact_id).exists() {
            return Err(ArtifactError::Collision(format!("{} is trashed", request.artifact_id)));
        }
        if directory.exists() {
            let existing = self.load(request.artifact_id)?;
            if existing != manifest
                || self.read_verified(request.artifact_id, &ArtifactAccess { actor: ActorRef::Owner, conversation_id: None })?
                    != request.content
            {
                return Err(ArtifactError::Collision(request.artifact_id.to_string()));
            }
            return Ok(existing);
        }
        crate::core::durability::atomic_replace(&directory.join("content"), request.content)?;
        if let Err(error) = crate::core::durability::write_json_atomic(&directory.join("manifest.json"), &manifest) {
            if !error.committed() {
                std::fs::remove_file(directory.join("content")).ok();
            }
            return Err(error.into());
        }
        Ok(manifest)
    }

    pub fn load(&self, artifact_id: &ResourceId) -> Result<ArtifactManifest, ArtifactError> {
        let path = self.directory(artifact_id).join("manifest.json");
        let bytes = std::fs::read(&path).map_err(|error| not_found(artifact_id, error))?;
        let manifest: ArtifactManifest = serde_json::from_slice(&bytes).map_err(|error| ArtifactError::Integrity(error.to_string()))?;
        if &manifest.artifact_id != artifact_id {
            return Err(ArtifactError::Integrity("artifact manifest id mismatch".into()));
        }
        let content =
            std::fs::read(self.directory(artifact_id).join("content")).map_err(|error| ArtifactError::Integrity(error.to_string()))?;
        if manifest.content_hash != ContentHash::from_bytes(&content) || manifest.size_bytes != content.len() as u64 {
            return Err(ArtifactError::Integrity("artifact content hash or size mismatch".into()));
        }
        Ok(manifest)
    }

    pub fn read_verified(&self, artifact_id: &ResourceId, access: &ArtifactAccess) -> Result<Vec<u8>, ArtifactError> {
        let manifest = self.load(artifact_id)?;
        if !allowed(&manifest, access) {
            return Err(ArtifactError::Denied);
        }
        std::fs::read(self.directory(artifact_id).join("content")).map_err(ArtifactError::Io)
    }

    pub fn trash(&self, artifact_id: &ResourceId) -> Result<(), ArtifactError> {
        let trashed = self.trashed_directory(artifact_id);
        if !self.directory(artifact_id).exists() && trashed.exists() {
            return Ok(());
        }
        self.load(artifact_id)?;
        crate::core::durability::rename_durable(&self.directory(artifact_id), &trashed)?;
        Ok(())
    }

    pub fn restore(&self, artifact_id: &ResourceId) -> Result<ArtifactManifest, ArtifactError> {
        let active = self.directory(artifact_id);
        let from = self.trashed_directory(artifact_id);
        if active.exists() && from.exists() {
            return Err(ArtifactError::Integrity("artifact exists in active and trash stores".into()));
        }
        if active.exists() {
            return self.load(artifact_id);
        }
        crate::core::durability::rename_durable(&from, &active)?;
        self.load(artifact_id)
    }

    fn directory(&self, artifact_id: &ResourceId) -> PathBuf {
        self.root.join("artifacts").join(artifact_id.as_str())
    }

    fn trashed_directory(&self, artifact_id: &ResourceId) -> PathBuf {
        self.root.join("trash/artifacts").join(artifact_id.as_str())
    }
}

pub struct CommitArtifact<'a> {
    pub artifact_id: &'a ResourceId,
    pub owner: AggregateRef,
    pub display_name: &'a str,
    pub media_type: &'a str,
    pub content: &'a [u8],
    pub shared_with_conversations: BTreeSet<ResourceId>,
    pub created_at_ms: u64,
}

fn validate_metadata(display_name: &str, media_type: &str, content: &[u8]) -> Result<(), ArtifactError> {
    if display_name.trim().is_empty()
        || display_name.contains('/')
        || display_name.contains('\\')
        || media_type.trim().is_empty()
        || content.len() > MAX_ARTIFACT_BYTES
    {
        return Err(ArtifactError::Invalid("artifact metadata or size is invalid".into()));
    }
    Ok(())
}

fn allowed(manifest: &ArtifactManifest, access: &ArtifactAccess) -> bool {
    match &access.actor {
        ActorRef::Owner => true,
        ActorRef::Bot { id } => {
            (manifest.owner.kind == AggregateKind::Bot && &manifest.owner.id == id)
                || access.conversation_id.as_ref().is_some_and(|conversation| manifest.shared_with_conversations.contains(conversation))
        }
        _ => false,
    }
}

fn not_found(id: &ResourceId, error: std::io::Error) -> ArtifactError {
    if error.kind() == std::io::ErrorKind::NotFound { ArtifactError::NotFound(id.to_string()) } else { ArtifactError::Io(error) }
}

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error(transparent)]
    Commit(#[from] crate::core::durability::CommitError),
    #[error("artifact IO: {0}")]
    Io(std::io::Error),
    #[error("artifact not found: {0}")]
    NotFound(String),
    #[error("artifact access denied")]
    Denied,
    #[error("artifact id collision: {0}")]
    Collision(String),
    #[error("artifact invalid: {0}")]
    Invalid(String),
    #[error("artifact integrity: {0}")]
    Integrity(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immutable_content_acl_integrity_and_recoverable_trash() {
        let root = std::env::temp_dir().join(format!("kxen-artifact-{}", uuid::Uuid::new_v4()));
        let store = ArtifactStore::new(&root);
        let artifact_id = ResourceId::parse("artifact_one").unwrap();
        let bot_id = ResourceId::parse("bot_owner").unwrap();
        let conversation_id = ResourceId::parse("bconv_shared").unwrap();
        let manifest = store
            .commit(CommitArtifact {
                artifact_id: &artifact_id,
                owner: AggregateRef { kind: AggregateKind::Bot, id: bot_id.clone() },
                display_name: "report.md",
                media_type: "text/markdown",
                content: b"verified",
                shared_with_conversations: [conversation_id.clone()].into_iter().collect(),
                created_at_ms: 1,
            })
            .unwrap();
        assert_eq!(manifest.content_hash, ContentHash::from_bytes(b"verified"));
        assert_eq!(
            store
                .read_verified(
                    &artifact_id,
                    &ArtifactAccess {
                        actor: ActorRef::Bot { id: ResourceId::parse("bot_peer").unwrap() },
                        conversation_id: Some(conversation_id)
                    }
                )
                .unwrap(),
            b"verified"
        );
        assert!(matches!(
            store.read_verified(
                &artifact_id,
                &ArtifactAccess { actor: ActorRef::Bot { id: ResourceId::parse("bot_peer").unwrap() }, conversation_id: None }
            ),
            Err(ArtifactError::Denied)
        ));
        store.trash(&artifact_id).unwrap();
        store.trash(&artifact_id).unwrap();
        assert!(matches!(store.load(&artifact_id), Err(ArtifactError::NotFound(_))));
        assert_eq!(store.restore(&artifact_id).unwrap(), manifest);
        assert_eq!(store.restore(&artifact_id).unwrap(), manifest);
        std::fs::write(root.join("artifacts/artifact_one/content"), b"tampered").unwrap();
        assert!(matches!(store.load(&artifact_id), Err(ArtifactError::Integrity(_))));
        std::fs::remove_dir_all(root).ok();
    }
}
