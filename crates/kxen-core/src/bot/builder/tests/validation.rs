use super::*;
use crate::agent::capability::CapabilityCatalog;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn deterministic_validation_requires_exact_grant_and_test_evidence() {
    let definition = definition("Reporter");
    let draft_hash = definition.content_hash().unwrap();
    let permission_hash = permission_hash(&definition).unwrap();
    let catalog = CapabilityCatalog::default();
    let roles = [id("execution")].into_iter().collect::<BTreeSet<_>>();
    let grant = PermissionGrant {
        grant_id: id("grant_one"),
        draft_hash: draft_hash.clone(),
        permission_hash,
        reason: "Reviewed exact scope".into(),
        granted_at_ms: 1,
    };
    let without_test = validate(
        id("report_one"),
        &definition,
        ValidationContext { catalog: &catalog, mrm_roles: &roles, connectors: &Default::default(), grant: Some(&grant), tests: &[] },
        2,
    )
    .unwrap();
    assert!(!without_test.publish_eligible);
    assert!(without_test.findings.iter().any(|finding| finding.status == ValidationStatus::Unknown));
    let evidence = TestEvidence {
        run_id: id("brun_test"),
        draft_hash,
        passed: true,
        criteria: BTreeMap::from([("Report totals are verified".into(), true)]),
        summary: "Output contract verified".into(),
        recorded_at_ms: 3,
    };
    let eligible = validate(
        id("report_two"),
        &definition,
        ValidationContext {
            catalog: &catalog,
            mrm_roles: &roles,
            connectors: &Default::default(),
            grant: Some(&grant),
            tests: &[evidence],
        },
        4,
    )
    .unwrap();
    assert!(eligible.publish_eligible);
    assert!(eligible.findings.iter().all(|finding| finding.status == ValidationStatus::Pass));
}

#[test]
fn deterministic_validation_rejects_unconfigured_connectors() {
    let mut definition = definition("Connector Reporter");
    definition.resources.connectors.insert(id("missing_connector"));
    let report = validate(
        id("report_missing_connector"),
        &definition,
        ValidationContext {
            catalog: &CapabilityCatalog::default(),
            mrm_roles: &[id("execution")].into_iter().collect(),
            connectors: &Default::default(),
            grant: None,
            tests: &[],
        },
        1,
    )
    .unwrap();
    assert!(report.findings.iter().any(|finding| finding.code == "connectors" && finding.status == ValidationStatus::Fail));
    assert!(!report.publish_eligible);
}
