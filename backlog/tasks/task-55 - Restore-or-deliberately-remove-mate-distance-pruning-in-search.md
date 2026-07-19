---
id: TASK-55
title: Restore or deliberately remove mate-distance pruning in search
status: To Do
assignee: []
created_date: '2026-07-18 23:42'
labels:
  - engine
  - search
dependencies: []
priority: medium
type: bug
ordinal: 54000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-54 replaced the mate-distance pruning bounds in `Search::search` Step 2 with a clamp to `Score::mate(0)`..`Score::mate(1)`. That fixed a real defect: the previous bounds used the root-relative `draft`, which is wrong in an engine whose mate scores are position-relative. But the replacement no longer prunes anything.

`Score::mate(0)` (-20_100) is the minimum representable non-infinite score and `Score::mate(1)` (20_099) is the maximum, so the clamp is a no-op for every real score and only maps `INF_N`/`INF_P` inward. The `if alpha >= beta { return alpha }` early return is now reachable only in a degenerate way: a PV parent whose alpha is still `mate(0)` passes `Score(20_100)` as the child's alpha, which the child clamps back below its beta. Instrumenting `cargo test -p engine` on the merged result showed this return firing 1,544 times, every time with `alpha == Score(20_100)`, and never as a genuine mate-distance cutoff.

This is safe and `cargo bench --bench search` showed no measurable cost at startpos depth 7, where mate scores do not arise. The concern is that a standard search optimisation was silently dropped in positions where it does matter, and the comment above the block still describes pruning that no longer happens, which will mislead the next reader.

Decide deliberately between reinstating correct position-relative mate-distance pruning (which needs the node's ply-from-root, not `draft`) and removing the block outright with an honest comment. Do not leave it as-is.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 It is decided and recorded whether mate-distance pruning is reinstated or removed, with the reasoning stated in the code or the task
- [ ] #2 If reinstated, the bounds are correct for position-relative mate scores and a test demonstrates a cutoff that the current clamp does not produce
- [ ] #3 If removed, the dead clamp and its misleading 'Mate distance pruning' comment are gone and any still-needed INF clamping is stated as such
- [ ] #4 Search behaviour on mate-rich positions is unchanged or improved, evidenced by the existing mate regression tests plus a debug self-play run over suites/wac.epd with no panic or hang
- [ ] #5 cargo bench --bench search shows no repeatable regression against the pre-change commit on the same machine
<!-- AC:END -->
