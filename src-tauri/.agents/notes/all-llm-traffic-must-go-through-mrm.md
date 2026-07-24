---
note-type: convention
description: All LLM traffic must go through MRM
date: 2026-07-24
---

ModelResourceManager is the single choke point for concurrency, RPM limits, role routing, degradation chains, and account rotation. acquire() returns RAII Slot that must be used for every call.
