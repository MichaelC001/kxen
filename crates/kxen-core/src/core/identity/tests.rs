use super::*;

#[test]
fn resource_id_rejects_paths_during_deserialization() {
    assert!(serde_json::from_str::<ResourceId>(r#""../escape""#).is_err());
    assert_eq!(serde_json::from_str::<ResourceId>(r#""bot_valid""#).unwrap().as_str(), "bot_valid");
}

#[test]
fn actor_roundtrip_preserves_typed_bot_identity() {
    let actor = ActorRef::Bot { id: ResourceId::parse("bot_research").unwrap() };
    let encoded = serde_json::to_string(&actor).unwrap();
    assert_eq!(serde_json::from_str::<ActorRef>(&encoded).unwrap(), actor);
    assert!(encoded.contains(r#""kind":"bot""#));
}

#[test]
fn content_hash_is_canonical_and_validated() {
    let hash = ContentHash::from_bytes(b"deterministic");
    assert_eq!(hash.as_str().len(), "sha256:".len() + 64);
    assert_eq!(serde_json::from_str::<ContentHash>(&serde_json::to_string(&hash).unwrap()).unwrap(), hash);
    assert!(ContentHash::parse("sha256:ABC").is_err());
}

#[test]
fn schema_version_zero_is_rejected() {
    assert!(SchemaVersion::new(0).is_err());
    assert!(serde_json::from_str::<SchemaVersion>("0").is_err());
    assert_eq!(SchemaVersion::new(1).unwrap().get(), 1);
}
