---
id: TASK-68.2
title: Adopt serde for JSON across the workspace
status: In Review
assignee:
  - '@george'
created_date: '2026-07-19 22:33'
updated_date: '2026-07-19 22:57'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Adopted serde/serde_json for all engine/src/ui JSON handling.

- Dependencies: added serde (derive) + serde_json to [workspace.dependencies] in the root Cargo.toml and inherited both in engine/Cargo.toml via .workspace = true. core is untouched. Cargo.lock now records serde 1.0.229 / serde_json 1.0.150. Per the task description this centralizes the JSON stack ahead of the Lichess work; only engine consumes it today.
- Outbound (wire.rs): replaced the hand-written String builders with borrowing Serialize DTOs. engineLimit/gameStatus/score/engineStatus are internally 'kind'-tagged enums; snapshot/progress/moveRecord use rename_all=camelCase. From impls carry over the exact conversions (mate-in-N moves math, INF_P/INF_N taken before is_mate, score cp raw i16). snapshot_to_json now returns serde_json::to_string(&SnapshotDto::from(snapshot)).
- Byte-compatibility: serde emits fields in declaration order, so the DTO field order reproduces the previous output exactly. A new golden test (snapshot_serializes_to_the_exact_wire_bytes) pins the full byte string for a representative thinking snapshot; the frontend (which JSON.parses, order-independent) needs no changes.
- Inbound (server.rs): request bodies are parsed with serde_json::from_str::<Value>; the object check and the per-field error codes (missing_uci, missing_revision, missing_human_side, invalid_human_side, missing_engine_limit) are preserved via Value::get/as_str/as_u64. Value::as_u64 matches the old strict as_u64 for the integer revisions the frontend sends; it additionally rejects exponent-form integers (e.g. 1e3) that the old parser accepted — the frontend never emits those, so this is a safe tightening for hand-crafted inputs only.
- Errors (http.rs): write_error now builds the {"error":code} body with serde_json::json!.
- Removed engine/src/ui/json.rs entirely (AC#2). session.rs and tests.rs migrated from the custom Json/parse to serde_json::Value; wire.rs tests re-expressed against serde_json::Value plus golden-byte assertions for each tagged sub-type (AC#4).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-19 22:57
---
Implementation handoff
Branch: task-68.2-serde-json
Worktree: /Users/seabo/seaborg-worktrees/task-68.2-serde-json
Base: 064f883e63cb04883cc3c764d15dd520f7e59441
Implementation target: e78daa1bbdf576300f55073a86bb877ac8c178c1
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (all binaries; engine lib 269 passed/2 ignored, ui 77 of them; integration + doc suites green)
Known failures: none
---
<!-- COMMENTS:END -->
