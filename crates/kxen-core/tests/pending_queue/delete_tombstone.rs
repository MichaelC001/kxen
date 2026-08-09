use super::*;

#[test]
fn delete_tombstone_rejects_new_queue_items_but_allows_recovery_replay() {
    let dir = tmp_dir("deleting");
    let queue = PendingQueues::new(dir.clone());
    let guard = kxen_core::core::session_recovery::begin_deletion(&dir, "ses_one").unwrap();
    let error = queue.enqueue("ses_one", "late".into(), vec![], vec![]).unwrap_err();
    assert!(error.contains("deletion in progress"));
    assert!(!queue.has_queued("ses_one"));

    queue
        .enqueue_existing(
            "ses_one",
            kxen_core::core::pending_queue::QueuedMessage {
                id: "queue-recovery".into(),
                created_at: 1,
                text: "preserved".into(),
                context: vec![],
                images: vec![],
                schedule_job_id: None,
            },
        )
        .unwrap();
    assert_eq!(queue.texts("ses_one"), vec!["preserved"]);
    guard.finish().unwrap();
    std::fs::remove_dir_all(dir).ok();
}
