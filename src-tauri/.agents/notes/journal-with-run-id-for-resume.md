---
note-type: convention
description: Journal with run_id for resume
date: 2026-07-24
---

Pass run_id to workflow tool to enable jsonl persistence in data_dir/workflow-journals. Completed agent() calls (keyed by hash(role+prompt)) are skipped on rerun for crash recovery and idempotency.
