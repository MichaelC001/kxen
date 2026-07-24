---
note-type: convention
description: Runs never end silently
date: 2026-07-24
---

Every agent run path must emit a final message or error. pending_messages queue, multiple fallbacks, and explicit completion handling enforce this.
