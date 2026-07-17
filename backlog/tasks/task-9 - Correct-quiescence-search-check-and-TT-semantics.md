---
id: TASK-9
title: Correct quiescence search check and TT semantics
status: Changes Requested
assignee: []
created_date: '2026-07-17 17:14'
updated_date: '2026-07-17 20:28'
labels:
  - search
  - correctness
dependencies: []
references:
  - engine/src/search.rs
  - engine/src/tt.rs
priority: high
type: bug
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Quiescence currently allows stand-pat behavior while in check and reuses transposition-table search scores as static evaluations without sufficient bound or depth semantics. Restore legal check-evasion behavior and valid alpha-beta windows.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Positions in check never return a stand-pat cutoff and search all required legal evasions
- [ ] #2 Transposition-table values are used in quiescence only when their stored depth and bound semantics justify the use
- [ ] #3 A stored search score is not substituted for a static evaluation unless it was explicitly stored as one
- [ ] #4 Quiescence never recurses with an empty or inverted alpha-beta window
- [ ] #5 Regression tests cover quiet check evasions, checkmate at the horizon, and TT hit variants
<!-- AC:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-17 20:28
---
Review: Changes requested.

Issue (medium) — false checkmate score on abort in quiescence in-check branch.

In quiesce()'s in-check evasion loop (engine/src/search.rs), the mate decision keys off move_count:

    for mov in &moves {
        if self.stopping() { break; }
        move_count += 1;
        ...
    }
    return if move_count == 0 { Score::mate(0) } else { alpha };

If `moves` is non-empty but self.stopping() is true on the FIRST iteration, the loop breaks with move_count == 0 and returns Score::mate(0) — a bogus 'we are checkmated' score for a position that actually has legal evasions. The main search deliberately avoids exactly this: at search.rs:693 it returns Score::zero() when stopping BEFORE the move_count == 0 mate check.

On a timed-out search this score can still leak into a parent node's alpha / the TT.

Fix: either return Score::zero() (or the current alpha) when self.stopping() before making the mate determination, OR base the checkmate decision on the generated list emptiness (moves.is_empty()) rather than move_count, which conflates 'no moves searched' with 'no moves exist'.

The other two review findings (TT Hit collision-guard divergence from main search; missing qsearch ply cap on check extensions) have been split out into their own tickets.
---
<!-- COMMENTS:END -->
