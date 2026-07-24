---
note-type: pitfall
description: Frontend state is unreliable
date: 2026-07-24
---

AppState locks (active_runs, pending_messages, etc.) are the only source of truth. Never trust UI state for control decisions or resumption.
