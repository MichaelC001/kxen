---
type: note
note-type: pitfall
description: rquickjs requires dedicated thread
date: 2026-07-24
---

rquickjs context is not Send, so workflow execution must run in a separate thread using current_thread tokio runtime. Cannot share runtime with main agent loop.
