---
id: TASK-14
title: Make UCI parsing and command handling total
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 21:44'
labels:
  - uci
  - input
dependencies: []
references:
  - engine/src/uci.rs
  - engine/src/engine.rs
modified_files:
  - engine/src/uci.rs
  - engine/src/engine.rs
priority: high
type: bug
ordinal: 19000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The UCI parser contains panic paths and unchecked numeric narrowing, while several successfully parsed standard commands fall through as unimplemented. Make parsing total and ensure supported and unsupported commands have protocol-safe outcomes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 No token sequence can reach todo, unwrap, or another parser panic
- [ ] #2 Depth and numeric parameters are range checked without truncation
- [ ] #3 Trailing tokens are handled consistently for all commands
- [ ] #4 Every parsed standard UCI command is either implemented or rejected without emitting non-protocol stdout
- [ ] #5 Parser and driver tests cover reserved standalone tokens, oversized numbers, malformed commands, setoption, and ucinewgame
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Make command parsing exhaustive, remove unchecked parser access/narrowing, and enforce end-of-command consistently.
2. Give every parsed command an explicit driver outcome: implement setoption and ucinewgame state handling, and route invalid/unsupported input only to stderr.
3. Add parser and driver regression tests for reserved tokens, overflow, malformed/trailing input, setoption, and ucinewgame.
4. Run focused tests, cargo fmt --check, and cargo test --workspace; commit implementation and prepare the review handoff.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Made top-level keyword parsing exhaustive, removed parser unwrap/expect paths, range-checked depth and Hash values, and required command termination consistently. Implemented silent Hash reconfiguration and ucinewgame transposition-table reset; malformed standard input now reports only on stderr. Added parser and driver regression coverage.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 21:44
---
Implementation handoff
Branch: task-14-uci-total
Worktree: /Users/seabo/seaborg-worktrees/task-14-uci-total
Base: 2c3a91b42c8810ca1897c4fc7675470aa4245ac0
Implementation target: 1136950f00f8628ce23160c00a2e9072675291d3
Resolved findings: none
Verification:
- cargo fmt --check: passed
- git diff --check: passed
- cargo test --workspace: passed (97 tests plus doc tests; 1 ignored)
Known failures: none
---
<!-- COMMENTS:END -->
