---
id: TASK-68.2
title: Adopt serde for JSON across the workspace
status: To Do
assignee: []
created_date: '2026-07-19 22:33'
labels: []
dependencies: []
parent_task_id: TASK-68
priority: medium
type: task
ordinal: 88000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Introduce `serde` (derive) + `serde_json` as workspace dependencies and migrate the browser UI's hand-rolled JSON code to serde, so the codebase has one consistent JSON approach ahead of the Lichess work (which will also use serde).

Current state: the UI hand-rolls JSON in engine/src/ui/json.rs and serializes/parses wire types in engine/src/ui/wire.rs (and json.rs) for the /api/* endpoints (state, events/SSE, move, undo, new-game, engine-limit). Replace this with serde derive on the wire types and serde_json for (de)serialization. Behavior over the wire must be byte-compatible enough that the existing TypeScript frontend keeps working unchanged.

Add serde/serde_json via the centralized workspace manifest policy (workspace deps in the root Cargo.toml; see TASK-67). Keep them off the `core` crate if it doesn't need them.

Note: npx tsc silently no-ops in this repo, so do not rely on a TypeScript compile as verification of the frontend — verify the UI manually or via the existing server/wire tests.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `serde` (with derive) and `serde_json` are added as workspace dependencies following the workspace manifest policy
- [ ] #2 engine/src/ui JSON handling uses serde/serde_json; the bespoke json.rs hand-rolled encoder/parser is removed or reduced to nothing custom
- [ ] #3 All existing /api/* endpoints produce and accept the same wire format the current frontend expects (no frontend changes required)
- [ ] #4 Existing UI server/wire tests pass; add serde round-trip coverage for the wire types
- [ ] #5 cargo fmt --check, clippy (workspace, all-targets, all-features, -D warnings), and cargo test --workspace all pass
<!-- AC:END -->
