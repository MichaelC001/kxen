use std::path::PathBuf;

use super::agent_journal::DcpRunToolJournal;
use super::{DcpToolPhase, ToolBoundaryAction, ToolBoundaryJournal};

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("kxen-dcp-journal-{}", uuid::Uuid::new_v4()))
}

#[test]
fn completed_outcome_replays_only_for_the_same_call_id_until_turn_is_durable() {
    let dir = temp_dir();
    let journal = DcpRunToolJournal::open(&dir).unwrap();
    assert_eq!(journal.before("call_a", "write", "{}", 1).unwrap(), ToolBoundaryAction::Execute);
    journal.after("call_a", "write", "{}", "ok", false, 2).unwrap();
    assert_eq!(journal.before("call_a", "write", "{}", 3).unwrap(), ToolBoundaryAction::Replay { output: "ok".into(), is_error: false });
    assert_eq!(journal.before("call_b", "write", "{}", 3).unwrap(), ToolBoundaryAction::Execute);
    journal.after("call_b", "write", "{}", "ok again", false, 4).unwrap();
    journal
        .settle_parts(&[crate::core::session::Part::ToolCall {
            name: "write".into(),
            input: serde_json::Value::Null,
            output: "ok".into(),
            args: Some(serde_json::json!({})),
            id: Some("call_a".into()),
            started_at: None,
            finished_at: None,
        }])
        .unwrap();
    assert_eq!(journal.snapshot().operations[0].phase, DcpToolPhase::Settled);
    assert_eq!(journal.snapshot().operations[1].phase, DcpToolPhase::OutcomeKnown);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn started_operation_becomes_unknown_after_restart() {
    let dir = temp_dir();
    {
        let journal = DcpRunToolJournal::open(&dir).unwrap();
        journal.before("call_a", "exec", "{}", 1).unwrap();
    }
    let journal = DcpRunToolJournal::open(&dir).unwrap();
    let unknown = journal.reconcile(&[]).unwrap();
    assert_eq!(unknown.len(), 1);
    assert!(journal.should_pause());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn unknown_outcome_resolution_replays_and_settles_exactly_once() {
    let dir = temp_dir();
    let journal = DcpRunToolJournal::open(&dir).unwrap();
    assert!(DcpRunToolJournal::open(&dir).err().unwrap().contains("already active"));
    assert_eq!(journal.before("call_a", "write", "{}", 1).unwrap(), ToolBoundaryAction::Execute);
    assert!(journal.before("call_a", "exec", "{}", 2).unwrap_err().contains("collision"));
    journal.mark_unknown("call_a", "transport ended", 3).unwrap();
    let operation_id = journal.snapshot().operations[0].operation_id.clone();
    assert!(journal.should_pause());
    assert!(journal.before("call_a", "write", "{}", 4).unwrap_err().contains("UNKNOWN"));
    assert!(journal.after("call_a", "write", "{}", "late", false, 4).unwrap_err().contains("invalid DCP tool outcome"));

    let resolved = journal.resolve_unknown(&operation_id, "verified", true).unwrap();
    assert_eq!(resolved.phase, DcpToolPhase::OutcomeKnown);
    assert!(journal.mark_unknown("call_a", "late uncertainty", 4).unwrap_err().contains("invalid DCP tool UNKNOWN transition"));
    assert_eq!(journal.unrecorded_outcomes().as_slice(), std::slice::from_ref(&resolved));
    assert_eq!(
        journal.before("call_a", "write", "{}", 5).unwrap(),
        ToolBoundaryAction::Replay { output: "verified".into(), is_error: true }
    );
    assert!(journal.resolve_unknown(&operation_id, "again", false).unwrap_err().contains("not UNKNOWN"));
    assert!(journal.resolve_unknown("op_missing", "none", false).unwrap_err().contains("not found"));
    assert!(journal.mark_unknown("call_missing", "none", 6).unwrap_err().contains("assignment is missing"));
    assert!(journal.after("call_missing", "write", "{}", "none", false, 6).unwrap_err().contains("assignment is missing"));

    journal.settle_operations(std::slice::from_ref(&operation_id)).unwrap();
    assert_eq!(journal.snapshot().operations[0].phase, DcpToolPhase::Settled);
    journal.after("call_a", "write", "{}", "verified", true, 7).unwrap();
    assert!(journal.after("call_a", "write", "{}", "different", true, 8).unwrap_err().contains("invalid DCP tool outcome"));
    drop(journal);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn corrupt_or_unsupported_tool_journal_is_rejected() {
    let corrupt_dir = temp_dir();
    std::fs::create_dir_all(&corrupt_dir).unwrap();
    std::fs::write(corrupt_dir.join("tools.json"), "not json").unwrap();
    assert!(DcpRunToolJournal::open(&corrupt_dir).err().unwrap().contains("parse DCP tool journal"));

    let unsupported_dir = temp_dir();
    std::fs::create_dir_all(&unsupported_dir).unwrap();
    std::fs::write(unsupported_dir.join("tools.json"), r#"{"schemaVersion":99,"operations":[]}"#).unwrap();
    assert!(DcpRunToolJournal::open(&unsupported_dir).err().unwrap().contains("unsupported DCP tool journal schema 99"));
    std::fs::remove_dir_all(corrupt_dir).ok();
    std::fs::remove_dir_all(unsupported_dir).ok();
}
