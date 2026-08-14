---
type: note
note-type: convention
description: Prefer workflow for parallel orchestration
date: 2026-07-24
---

Use workflow + Promise.all + agent() for large-scale parallel sub-agent dispatch rather than manual sequential calls. This is the official and recommended pattern for complex tasks.
