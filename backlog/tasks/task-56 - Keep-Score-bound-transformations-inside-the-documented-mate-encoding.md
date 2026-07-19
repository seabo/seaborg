---
id: TASK-56
title: Keep Score bound transformations inside the documented mate encoding
status: To Do
assignee: []
created_date: '2026-07-18 23:43'
labels:
  - engine
  - search
dependencies: []
priority: medium
type: bug
ordinal: 55000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-54 added `Score::child_bound()`, the inverse of `neg().inc_mate()`, to transform alpha/beta windows when recursing into a child node. It can produce values outside the encoding that `Score`'s own doc comment defines.

`Score` documents the mate band as 20_000..=20_100. `child_bound(Score::mate(0))` returns `Score(20_101)`, which is outside it. In `Search::search` the Step 2 clamp pulls such values back, but quiescence has no equivalent clamp, so the excursion compounds by one each ply: 20_100 -> -20_101 -> 20_102 -> -20_103. Because `quiesce` and `quiesce_evasions` return `alpha`/`beta` directly as fail-soft scores, these out-of-range values become node scores. Instrumenting the TASK-54 regression test alone showed 315 such returns from the stand-pat beta path, reaching +/-20_106. The same probe on the pre-TASK-54 base commit (ebf4289) produced none, so this is specific to the new transformation.

This is currently latent, not a live defect. Review confirmed these values are consumed only as fail-soft bounds: they never reach the transposition table, and a debug-build sweep over all 300 positions in suites/wac.epd at depths 4/5/6 formatted all 4,500 root scores through `Display` with debug assertions live and produced zero panics. The risk is that `Debug` renders them as nonsense (`Score(20_101)` prints as `Mate(-1)`) and `Display` would trip its parity assertion if one ever reached it, which is exactly the failure mode TASK-54 existed to eliminate.

Either make the representation total over the values search actually produces, or clamp bounds into the representable range before transforming. Either way the `Score` doc comment should describe what bound values can actually hold, since it is currently inaccurate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Bound values produced by child_bound stay within a documented, representable range, or the Score doc comment is corrected to state the range bound values may occupy and why
- [ ] #2 A test covers the mate(0) and mate(1) boundary inputs to child_bound; the existing child_bounds_invert_parent_mate_distance_conversion only covers in-range values
- [ ] #3 Quiescence no longer returns scores outside the documented range, demonstrated by an assertion or test that fails on the current code
- [ ] #4 Debug and Display produce sensible output for every value search can now generate
- [ ] #5 The TASK-54 regression child_mate_windows_preserve_distance_parity still passes and a debug WAC sweep still formats root scores without panicking
<!-- AC:END -->
