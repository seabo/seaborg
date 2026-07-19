---
id: TASK-15
title: Connect engine configuration to UCI options and search resources
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
updated_date: '2026-07-19 23:22'
labels:
  - engine
  - uci
  - configuration
dependencies:
  - TASK-57
references:
  - engine/src/options.rs
  - engine/src/engine.rs
  - engine/src/search.rs
  - engine/src/uci.rs
  - tools/strength/strength_test.py
  - README.md
priority: high
type: enhancement
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Engine resource configuration is split between an unused Config type and ad hoc state in the UCI driver. Hash is currently the only advertised option; the search worker count is fixed at one. Establish one authoritative configuration owner and truthful, validated UCI resource options so the Lazy SMP programme can change hash and worker resources only at safe lifecycle boundaries.

This task owns the configuration model, validation, and quiescent application semantics. It does not itself need to spawn multiple search workers: the Lazy SMP search-team tasks consume this foundation. The Threads option must not be advertised before multiple workers are real, but the configuration design must accommodate it without another ownership rewrite.

Concurrency boundary. An active search owns Arc clones of the shared transposition table. Hash replacement, physical clearing, and any worker-resource rebuild must occur only after the complete search team has been cancelled and joined. The existing SearchEngine::clear_hash Arc::get_mut boundary and SearchHandle join-on-drop behavior are invariants to preserve.

Truthfulness boundary. The UCI handshake must advertise exactly what the running engine applies. The repository strength tool already sends Hash and Threads options, so it must remain possible to run against builds that do not yet advertise Threads and then use it once Lazy SMP lands.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One authoritative runtime configuration owns all advertised engine resource settings; obsolete or disconnected configuration types are removed or integrated
- [ ] #2 The UCI handshake advertises exactly the configurable options implemented by that build, with documented defaults and bounds
- [ ] #3 setoption validates values and applies resource changes only after any active search or search team has been cancelled and fully joined
- [ ] #4 Hash controls the actual transposition-table allocation, and allocation failure or an unsupported size is handled without leaving configuration and resources inconsistent
- [ ] #5 The configuration model supports a worker count, but Threads is not advertised until a real multi-worker search consumes it
- [ ] #6 Hash replacement, clearing, and worker-resource rebuilds occur only at an owner-controlled quiescent boundary where no worker holds the old table allocation
- [ ] #7 Tests cover defaults, handshake truthfulness, valid and invalid values, repeated changes, and changes while a search is active
- [ ] #8 Strength tooling and documentation accurately describe which options are required and do not claim unsupported Lazy SMP behavior
<!-- AC:END -->
