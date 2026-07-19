---
id: TASK-15
title: Connect engine configuration to UCI options and search resources
status: To Do
assignee: []
created_date: '2026-07-17 17:14'
updated_date: '2026-07-19 03:37'
labels:
  - engine
  - uci
  - configuration
dependencies: []
references:
  - engine/src/options.rs
  - engine/src/engine.rs
  - README.md
priority: medium
type: enhancement
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Configuration types are disconnected from the running engine: hash size and debug settings are not applied, the table size is hardcoded, and thread count is fixed. Establish one configuration owner and make advertised options truthful.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The UCI handshake advertises exactly the configurable options the engine implements
- [ ] #2 setoption updates configuration with validation and applies resource changes at a safe lifecycle boundary
- [ ] #3 Hash size controls the actual transposition-table allocation
- [ ] #4 Thread configuration either controls real worker count or is omitted together with unsupported LazySMP claims
- [ ] #5 Tests cover default values, valid changes, invalid values, and changes around an active search
- [ ] #6 Hash replacement, resizing, and administrative clearing occur only at an owner-controlled quiescent boundary after every worker using the old shared allocation has stopped
- [ ] #7 If a Threads option is introduced, all workers share the lock-free transposition table defined by TASK-57; otherwise the option and any Lazy SMP capability claim remain absent
<!-- AC:END -->
