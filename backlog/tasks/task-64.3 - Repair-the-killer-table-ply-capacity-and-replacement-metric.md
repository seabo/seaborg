---
id: TASK-64.3
title: Repair the killer table ply capacity and replacement metric
status: To Do
assignee: []
created_date: '2026-07-19 13:31'
updated_date: '2026-07-19 23:49'
labels:
  - search
  - move-ordering
dependencies:
  - TASK-64.1
references:
  - engine/src/killer.rs
  - engine/src/search.rs
parent_task_id: TASK-64
priority: high
type: bug
ordinal: 66000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The killer table is structurally integrated into the main search but is not acceptable as a strength and performance foundation in its current form. It has two concrete defects and lacks the measurement needed to establish whether its ordering policy remains useful once stronger contextual history is available.

Capacity. The main search is bounded by MAX_PLY = 256, while KILLER_PLIES = 21 covers only plies 1 through 20. Probe and store silently ignore deeper plies. Selective search, reductions and extensions make nominal iteration depth an unreliable proxy for reachable ply, and the memory saved by truncating this small per-worker table is negligible. MAX_PLY should be the single authoritative capacity unless measurement supports a deliberately smaller, explicitly tested boundary.

Replacement and ordering. Each slot carries a usize counter. Probe increments it whenever the stored move is legal in the current position, and store evicts the lower-count slot. This measures how often a move was offered, not how often it caused a cutoff. A frequently legal but ineffective move can become permanent while a newly successful refutation starts at zero and is preferentially evicted. Probe is therefore stateful, returned order reflects exposure rather than usefulness, and the unbounded counters can eventually overflow.

Replace this with a simple two-slot recency table by default: on a distinct quiet beta cutoff, move the previous first slot to the second slot and install the new killer first. Re-storing the current first slot is a no-op. This makes slot order deterministic, keeps probe observationally read-only and leaves long-term or contextual evidence to history, counter-move and continuation-history tables. Use a fixed per-worker table covering the complete main-search ply range; legality validation remains required because a killer was learned in a different position at the same ply.

Integration. Quiet beta cutoffs already update both killers and butterfly history, hash and quiet duplicates are suppressed by staged ordering, and search-local ownership is suitable for future per-worker Lazy SMP state. Preserve those properties. Define whether killers persist only across iterative-deepening iterations or also across separate Search::run calls and future worker reuse; reset behavior must be deliberate and tested.

Measurement. Existing telemetry counts legal killers offered, which is availability rather than effectiveness. Record attempted killer moves and beta cutoffs by slot after duplicate suppression so the ordering policy can be evaluated. Compare fixed-depth node counts and search throughput with killers disabled, one recency slot and two recency slots. Measure the chosen design with TASK-27. A later ablation after counter-move and continuation history lands must decide whether killers still add strength or merely duplicate stronger contextual evidence; deletion is an acceptable result if measurement supports it.

This task retains the current staged-ordering architecture rather than requiring a search rewrite. It depends on TASK-64.1 because capacity and indexing are expressed in explicit ply.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The killer table uses MAX_PLY, or another single authoritative main-search ply bound justified by measurement, and a killer stored at the deepest supported main-search ply is retrievable there
- [ ] #2 Probe is observationally read-only and slot replacement does not use legality, probe frequency or another exposure metric as a proxy for cutoff usefulness
- [ ] #3 Distinct quiet beta cutoffs use a documented deterministic replacement policy; by default the newest successful killer occupies slot one and the previous distinct slot-one move shifts to slot two
- [ ] #4 Tests cover deep-ply retrieval, neighbouring-ply isolation, duplicate stores, deterministic returned order, replacement after three distinct cutoffs and the exact supported boundary
- [ ] #5 Killer legality validation and hash/killer/quiet duplicate suppression remain intact before unsafe move execution
- [ ] #6 Killer reset and persistence semantics are documented and tested across iterative-deepening iterations, separate Search::run calls and the ownership model expected for Lazy SMP workers
- [ ] #7 Telemetry distinguishes killer attempts and beta cutoffs by slot after duplicate suppression rather than reporting availability alone
- [ ] #8 Fixed-depth node counts and search throughput are recorded for killers disabled, one recency slot and two recency slots, with the selected policy justified by the results
- [ ] #9 The selected design is measured with the TASK-27 strength-regression script and results are recorded in implementation notes
<!-- AC:END -->
