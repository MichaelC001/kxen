---
okf_version: "0.2"
title: Kxen project knowledge
description: Progressive disclosure map for the Kxen OKF bundle.
---

# Kxen project knowledge

This directory is an OKF v0.2 bundle. Directory names organize related concepts. Each non-reserved Markdown concept declares its semantic `type` in YAML frontmatter.

## Start here

- [Architecture](references/architecture.md): product boundaries and runtime structure.
- [Rust style](rules/rust-style.md): Rust implementation rules applied by the Rule handler.
- [Repository structure](notes/repo-structure.md): workspace and package layout.
- [Safety model](notes/safety-model.md): execution-layer safety invariants.
- [Goal lifecycle](notes/goal-lifecycle.md): durable goal state and budget rules.
- [MRM routing](notes/all-llm-traffic-must-go-through-mrm.md): Provider admission and accounting contract.

## Concept semantics

Known runtime types are `rule`, `skill`, `command`, `note`, `memory`, `reference`, and `history`. Other values such as `code`, `refactor`, or `test` remain valid generic concepts: they are searchable and linkable but do not acquire executable behavior.
