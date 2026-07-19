---
id: TASK-68.2
title: Adopt serde for JSON across the workspace
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-19 22:33'
updated_date: '2026-07-19 22:48'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add serde (derive) + serde_json to [workspace.dependencies] in root Cargo.toml; inherit both in engine/Cargo.toml as regular deps. Leave core untouched.
2. Rewrite engine/src/ui/wire.rs to serialize via serde-derived output DTO structs/enums that mirror the exact wire shape (internally 'kind'-tagged enums for engineLimit/gameStatus/score/engineStatus; camelCase field renames). Build DTOs from the engine's typed values (preserving mate-moves math and inf-first ordering) and emit via serde_json::to_string. Keep byte output identical: same field order (serde emits in declaration order), same integer/bool formatting, no extra escaping.
3. Replace inbound command parsing in server.rs: parse the request body with serde_json into serde_json::Value, keep the object check and per-field error codes (missing_uci, missing_revision, missing_human_side, missing_engine_limit) using Value::get/as_str/as_u64.
4. Replace http.rs write_error's hand-rolled body with serde_json.
5. Delete engine/src/ui/json.rs and drop the mod json; its strict parsing/escaping is now serde_json's job.
6. Update wire.rs tests to parse with serde_json::Value; add golden exact-byte assertions per wire sub-type and a Value round-trip, plus an inbound-body parse test. Verify fmt, clippy -D warnings, cargo test --workspace.
<!-- SECTION:PLAN:END -->
