use super::*;
use serde_json::{Value, json};

#[test]
fn rejects_missing_wrong_and_invalid_params_with_data() {
    assert_eq!(validate_rpc("not.real", json!({})).unwrap_err().code, -32601);
    let missing = validate_rpc("approval.respond", json!({ "id": "a" })).unwrap_err();
    assert_eq!(missing.code, -32602);
    assert_eq!(missing.data.unwrap()["field"], "allow");
    assert_eq!(validate_rpc("send_message", json!({ "session_id": "s", "images": "bad" })).unwrap_err().code, -32602);
    assert_eq!(validate_rpc("config.set_send_policy", json!({ "policy": "drop" })).unwrap_err().code, -32602);
    assert_eq!(
        validate_rpc("goal.create", json!({ "objective": "o", "completion_criteria": "c", "budget": { "turns": -1 } })).unwrap_err().code,
        -32602
    );
}

#[test]
fn accepts_registered_contracts_and_rejects_every_value_constraint() {
    for (method, params) in [
        ("task.restart", json!({ "id": "task_one", "session_id": "ses_one" })),
        ("send_message", json!({ "session_id": "ses_one", "text": "", "context": [], "images": [] })),
        (
            "provider.add_custom",
            json!({
                "name": "local",
                "base_url": "https://example.test/v1",
                "models": ["model-one"],
                "api_key": "secret",
                "capabilities": ["text"]
            }),
        ),
        (
            "goal.create",
            json!({
                "objective": "ship",
                "completion_criteria": "verified",
                "budget": { "tokens": 10, "turns": 2, "wall_clock_ms": 1000 }
            }),
        ),
        (
            "config.set_limits",
            json!({
                "provider": "xai",
                "daily_token_budget": 10,
                "input_usd_per_million": 1.5,
                "output_usd_per_million": 2,
                "daily_cost_budget_usd": 3.5,
                "circuit_failure_threshold": 2,
                "circuit_cooldown_seconds": 30
            }),
        ),
        (
            "composer.suggest.remote",
            json!({ "draft": "fix auth", "session_id": "ses_one", "request_id": "suggest_one", "candidate_ids": [] }),
        ),
        ("config.set_composer_suggestions", json!({ "key": "semantic", "enabled": true })),
        ("config.set_embedding", json!({ "provider": "ollama", "model": "nomic-embed-text", "base_url": "" })),
        ("knowledge.move", json!({ "scope": "project", "slug": "note", "to": "personal" })),
        ("knowledge.consolidation_acknowledge_unknown", json!({ "session_id": "ses_one", "confirm_unknown": true })),
    ] {
        assert!(validate_rpc(method, params).is_ok(), "{method} rejected");
    }

    for (method, params, field) in [
        ("session.delete", json!({ "id": "" }), "id"),
        ("approval.respond", json!({ "id": "approval_one", "allow": "yes" }), "allow"),
        ("send_message", json!({ "session_id": "ses_one", "context": {} }), "context"),
        (
            "provider.add_custom",
            json!({ "name": "local", "base_url": "https://example.test", "models": [""], "api_key": "secret" }),
            "models",
        ),
        ("session.update_meta", json!({ "id": "ses_one", "sort_order": -1 }), "sort_order"),
        ("config.set_limits", json!({ "input_usd_per_million": -0.1 }), "input_usd_per_million"),
        ("goal.create", json!({ "objective": "ship", "completion_criteria": "verified", "budget": [] }), "budget"),
    ] {
        let error = validate_rpc(method, params).expect_err(method);
        assert_eq!(error.code, -32602);
        assert_eq!(error.data.as_ref().and_then(|data| data.get("field")).and_then(Value::as_str), Some(field));
    }

    for (method, params) in [
        ("config.set_experimental", json!({ "key": "unknown", "enabled": true })),
        ("config.set_composer_suggestions", json!({ "key": "unknown", "enabled": true })),
        ("config.set_embedding", json!({ "provider": "unknown" })),
        ("session.set_model", json!({ "id": "ses_one", "provider": "xai" })),
        ("provider.add_custom", json!({ "name": "local", "base_url": "https://example.test", "models": [], "api_key": "secret" })),
        ("knowledge.remove", json!({ "scope": "bogus", "slug": "note" })),
        ("goal.create", json!({ "objective": "ship", "completion_criteria": "verified", "budget": { "turns": u64::from(u32::MAX) + 1 } })),
        ("config.set_limits", json!({ "daily_cost_budget_usd": 1.0 })),
        ("knowledge.consolidation_acknowledge_unknown", json!({ "session_id": "ses_one", "confirm_unknown": false })),
    ] {
        assert_eq!(validate_rpc(method, params).expect_err(method).code, -32602);
    }

    assert!(validate_rpc("session.update_meta", json!({ "id": "ses_one", "title": null })).is_ok());
}
