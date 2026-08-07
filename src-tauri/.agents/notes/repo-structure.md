---
note-type: convention
description: Repo structure
date: 2026-07-24
---

The root pnpm workspace contains only the `kxen-ui` desktop frontend package. The product website is an independent `website` pnpm package outside that workspace, and `src-tauri` is a Cargo workspace: the `kxen-gui` shell crate (Tauri desktop bin, lib `kxen_gui`) plus `crates/kxen` (headless server bin). Install and verification commands therefore run separately in the root and `website` directories, and cargo gates need `--workspace` to cover both crates.
