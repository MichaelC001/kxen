use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    value: String,
}

fn path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("kxen-journal-{name}-{}.jsonl", uuid::Uuid::new_v4()))
}

#[test]
fn strict_journal_appends_and_resumes_at_cursor() {
    let path = path("cursor");
    let journal = StrictJsonl::<Record>::new(&path);
    let first = journal.append(&Record { value: "one".into() }, 0).unwrap();
    journal.append(&Record { value: "two".into() }, 1).unwrap();
    let tail = journal.scan(first.end).unwrap();
    assert_eq!(tail.records, [Record { value: "two".into() }]);
    assert_eq!(tail.next.record_index, 2);
    std::fs::remove_file(path).ok();
}

#[test]
fn strict_journal_fails_closed_on_torn_or_bad_record() {
    let torn = path("torn");
    std::fs::write(&torn, b"{\"value\":\"one\"}").unwrap();
    assert!(matches!(StrictJsonl::<Record>::new(&torn).scan(JournalCursor::default()), Err(JournalError::Unterminated { .. })));
    let bad = path("bad");
    std::fs::write(&bad, b"{\"unknown\":1}\n").unwrap();
    assert!(matches!(StrictJsonl::<Record>::new(&bad).scan(JournalCursor::default()), Err(JournalError::Parse { .. })));
    std::fs::remove_file(torn).ok();
    std::fs::remove_file(bad).ok();
}

#[test]
fn best_effort_journal_reports_and_skips_bad_observations() {
    let path = path("best-effort");
    std::fs::write(&path, b"{\"value\":\"one\"}\nnot-json\n{\"value\":\"two\"}").unwrap();
    let scan = BestEffortJsonl::<Record>::new(&path).scan(JournalCursor::default()).unwrap();
    assert_eq!(scan.records.len(), 2);
    assert_eq!(scan.diagnostics.len(), 2);
    assert!(scan.diagnostics.iter().any(|item| item.message.contains("unterminated")));
    std::fs::remove_file(path).ok();
}

#[test]
fn cursor_must_land_after_record_terminator() {
    let path = path("boundary");
    std::fs::write(&path, b"{\"value\":\"one\"}\n").unwrap();
    let cursor = JournalCursor { byte_offset: 3, record_index: 0 };
    assert!(matches!(StrictJsonl::<Record>::new(&path).scan(cursor), Err(JournalError::InvalidCursor { .. })));
    std::fs::remove_file(path).ok();
}
