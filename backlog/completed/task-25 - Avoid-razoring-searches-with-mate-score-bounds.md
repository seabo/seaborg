---
id: TASK-25
title: Avoid razoring searches with mate-score bounds
status: Done
assignee:
  - "@codex"
created_date: "2026-07-17 16:31"
updated_date: "2026-07-17 16:35"
labels: []
dependencies: []
modified_files:
  - engine/src/search.rs
type: bug
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->

Prevent the search's static-evaluation razoring optimization from constructing invalid quiescence windows when alpha is a mate score.

<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->

- [x] #1 Razoring is not applied when alpha is outside the centipawn score domain
- [x] #2 The correct-answer search regression passes in debug builds
- [x] #3 The Rust workspace test suite has no remaining search::tests::gives_correct_answers failure
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->

1. Add a centipawn-domain guard to the razoring condition.
2. Add focused regression coverage for mate-score and infinity bounds.
3. Run formatting and the full workspace test suite.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->

Added a should_razor predicate that excludes non-centipawn alpha bounds, with focused coverage for ordinary, mate, and infinity bounds.

Verification: cargo fmt --check passed; the focused razoring predicate test passed; search::tests::gives_correct_answers passed in debug mode. cargo test --workspace completed with 26 engine tests passing, including both search tests, and only the separately investigated tt::tests::gen_bound failure remaining (plus one ignored test).

<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->

Restricted static-evaluation razoring to centipawn alpha bounds and added focused mate/infinity regression coverage. Formatting and all search regressions pass; the workspace now has only the independent generation-packing failure.

<!-- SECTION:FINAL_SUMMARY:END -->
