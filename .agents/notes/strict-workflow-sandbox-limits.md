---
note-type: pitfall
description: Strict workflow sandbox limits
date: 2026-07-24
---

Workflow JS is sandboxed to 64MB memory, 1MB stack, max 32 agent() calls, and 10-minute total timeout. Can be interrupted on timeout or user cancel.
