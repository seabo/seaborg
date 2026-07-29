---
id: TASK-97.2
title: 'B1: TT-size sweep — is the blind-node population evictable?'
status: To Do
assignee: []
created_date: '2026-07-29 18:45'
labels:
  - search
  - selectivity
  - investigation
dependencies: []
references:
  - tools/diag/selectivity_profile.py
  - engine/src/tt.rs
parent_task_id: TASK-97
priority: high
type: spike
ordinal: 167000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Track B / item B1. Hypothesis: a big share of the ~70% no-TT-move non-PV nodes are re-visits whose entry was EVICTED, not genuine first-visits — i.e. fixable via TT capacity/replacement, not inherent. Method: run the fixed-NODES selectivity profile at Hash = 16, 64, 256, 1024 MB (vary the UCI Hash option). Fixed nodes (not fixed depth) so a bigger TT can actually change the tree. No new code. Decision: TT-move-availability rises materially with size → eviction-limited → replacement-policy work (B2) is a live lever. Flat → inherent first-visits → ordering must come from history/IIR quality (D3), not TT.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The fixed-nodes selectivity profile is run at Hash = 16, 64, 256, 1024 MB and TT-move-availability (non-PV), hash-hit rate, first-move-cutoff, and depth reached are reported per size
- [ ] #2 A verdict is recorded: eviction-limited (availability rises materially with size) → unblocks B2, or inherent first-visits (flat) → points to D3
- [ ] #3 No permanent engine change ships
<!-- AC:END -->
