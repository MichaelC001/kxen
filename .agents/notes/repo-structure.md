---
type: note
note-type: convention
description: Repo structure
date: 2026-07-24
---

The root pnpm workspace contains only the `kxen-ui` desktop frontend package. The product website is an independent `website` pnpm package outside that workspace. The Cargo workspace sits at the repository root: `crates/kxen-core` (lib `kxen_core`, all product logic), `crates/kxen-cli` (headless server bin `kxen`), and `src-tauri` (the `kxen-gui` Tauri desktop shell crate). Install and verification commands therefore run separately in the root and `website` directories, and cargo gates need `--workspace` to cover all three crates.
