---
id: TASK-56
title: Keep Score bound transformations inside the documented mate encoding
status: Ready to Merge
assignee:
  - '@codex'
created_date: '2026-07-18 23:43'
updated_date: '2026-07-19 03:15'
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
- [x] #1 Bound values produced by child_bound stay within a documented, representable range, or the Score doc comment is corrected to state the range bound values may occupy and why
- [x] #2 A test covers the mate(0) and mate(1) boundary inputs to child_bound; the existing child_bounds_invert_parent_mate_distance_conversion only covers in-range values
- [x] #3 Quiescence no longer returns scores outside the documented range, demonstrated by an assertion or test that fails on the current code
- [x] #4 Debug and Display produce sensible output for every value search can now generate
- [x] #5 The TASK-54 regression child_mate_windows_preserve_distance_parity still passes and a debug WAC sweep still formats root scores without panicking
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a debug assertion that every score returned by quiesce/quiesce_evasions lies in the search-producible band [Score::mate(0), Score::mate(1)], and confirm it fires on the current code via child_mate_windows_preserve_distance_parity.
2. Make Score::child_bound total over the documented encoding: the two boundary inputs whose exact inverse would fall outside the mate band saturate to the corresponding infinity. child_bound(mate(0)) == INF_P is semantically exact, since a bound one step beyond the best achievable score is unreachable, exactly like +inf as a cutoff threshold.
3. Mirror search's Step 2 mate-distance clamp in quiesce so incoming windows are normalised into [mate(0), mate(1)] before use, with an alpha >= beta early return for the degenerate case. This stops the per-ply excursion compounding and keeps every quiescence return in band.
4. Make Debug total: values outside the documented bands render explicitly rather than as a plausible-looking Mate(-1) or Cp(15_000).
5. Correct the Score doc comment to state which values search actually produces, and document child_bound's saturation and why it is exact.
6. Tests: child_bound at the mate(0)/mate(1) boundaries; Debug and Display over every search-producible value; keep child_mate_windows_preserve_distance_parity passing.
7. Verify with cargo fmt/clippy/test plus a debug-build WAC sweep at depths 4/5/6 formatting every root score through Display.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Root cause. `Score::child_bound` is the exact inverse of `neg().inc_mate()`, and exactness is load-bearing: callers rely on it to map a null window to a null window. Exactness also means `child_bound(Score::mate(0))` is `Score(20_101)`, one step past the top of the mate band, asking for a value better than mating on the next ply. That is a sound cutoff threshold but is not a score, so the fix belongs in the callers rather than in the transformation.

Scope found to be wider than the task described. The task attributed the escape to quiescence alone, since `search` has a Step 2 mate distance clamp. That clamp only narrows towards the middle of the band (`max(mate(0), alpha)`, `min(mate(1), beta)`), so it does not catch a window that is entirely above the band. A parent searching the null window at the bottom of the band hands its child `(Score(20_100), Score(20_101))`; the old clamp left alpha at 20_100, hit `alpha >= beta` and returned 20_100 verbatim. The parent's `neg().inc_mate()` maps that to `Score(-20_099)`, a negative mate at an odd ply count, which is exactly the wrong-parity score that panics `Display` and that TASK-54 existed to eliminate. So `search` had the same defect as `quiesce`, reachable by a shorter path. Covered by `out_of_band_windows_do_not_leak_into_returned_scores`.

Fix. Both `search` and `quiesce` now clamp both bounds at both ends into the node score band on entry. Clamping outwards discards nothing attainable: `alpha` is returned as an upper bound and no score exceeds `mate(1)`, `beta` as a lower bound and none falls below `mate(0)`. Null windows are preserved or collapse into the existing `alpha >= beta` early return, so the `alpha.inc_one() == beta` invariant still holds for children.

Considered and rejected: saturating `child_bound` to the infinities at the band ends. It removes the out-of-band value at its source, but it breaks null-window preservation — the bottom-of-band null window maps to `(Score(20_100), INF_P)` — and trips `debug_assert!(Node::pv() || alpha.inc_one() == beta)`. Confirmed empirically before backing it out.

Representation. `Score::is_node_score` names the band a searched node can hold, `mate(0)..=mate(1)`. Both routines are wrapped so every returned score is debug-asserted against it. The `Score` doc comment now separates node scores from window bounds and states that bounds reach `Score(20_101)` in transit and why. `Debug` prints anything outside the named bands in raw `Score(n)` form; previously `Score(20_101)` printed as `Mate(-1)` and the 10_000-20_000 gap printed as a centipawn value, both of which read as ordinary scores and hide the defect.

Performance. The clamps sit on the quiescence hot path, so this was measured rather than assumed: release build, `cargo bench --bench search`, three alternating base/target rounds on an otherwise idle machine. Base 3fc1644 medians 40.736 / 40.695 / 40.784 us; target 39cbbb3 medians 40.404 / 40.462 / 40.782 us. No-deadline variant, base 40.128 / 40.514 / 40.140 us; target 40.059 / 39.854 / 39.968 us. Target is equal or marginally faster in every round; no regression. BENCHMARKS.md is unchanged, since a within-noise result is not grounds to move a baseline.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 02:47
---
Implementation handoff
Branch: task-56-score-bound-encoding
Worktree: /Users/seabo/seaborg-worktrees/task-56-score-bound-encoding
Base: 3fc1644e5f335c22de55f7295b8da516107dde54
Implementation target: 39cbbb3
Resolved findings: none (new work)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 237 passed / 0 failed / 2 ignored
- cargo test -p engine -- --ignored wac_root_scores_format_without_panicking: pass in 350s, debug build, 900 searches over all 300 WAC positions at depths 4/5/6, every root score in band and formatted through Display with debug assertions live
- cargo bench --bench search, release, 3 alternating base/target rounds: no regression (medians in the implementation notes)
Known failures: none

Reproduction evidence for AC #3. Each of the three tests below was confirmed to fail with the entry clamps reverted to their pre-change form (search back to the one-sided max/min, quiescence's clamp removed), then to pass with them restored:
- search::tests::out_of_band_windows_do_not_leak_into_returned_scores
- search::tests::quiescence_clamps_out_of_band_windows_into_the_node_score_band
- search::tests::child_mate_windows_preserve_distance_parity (the TASK-54 regression, which trips the new band assertion on the pre-change code with 'quiescence returned Mate(-1) outside the node score band')

Reviewer note. The change is wider than the task's description in one respect, and deliberately so: 'search' had the same defect as quiescence, not merely the clamp that protects against it. See the second paragraph of the implementation notes. Worth confirming independently that clamping a bound outwards is sound, since that is the step the correctness of the fix rests on.
---

author: @codex
created: 2026-07-19 03:15
---
Review verdict: APPROVED

Implementation target: 39cbbb3 (immutable; ancestor of branch tip 2b4fea0, which changes only the task file)
Base: 3fc1644
Reviewed diff: 3fc1644..39cbbb3 — engine/src/score.rs, engine/src/search.rs, task file only. No unrelated changes, no #[allow] added.

Verification run independently in the task worktree at 39cbbb3:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, zero warnings, re-confirmed with a clean CARGO_TARGET_DIR so the result is not cached (core, engine and seaborg all linted)
- cargo test --workspace: pass
- The eight tests bearing on this change all pass, including the TASK-54 regression child_mate_windows_preserve_distance_parity (AC #5)
- cargo test -p engine -- --ignored wac_root_scores_format_without_panicking: pass in 354s, debug build, 900 searches (AC #5)
- cargo bench --bench search, release, 3 alternating base/target rounds on this machine with no competing build or test processes. Deadline medians base 40.634 / 40.183 / 40.509 us vs target 40.132 / 40.569 / 40.266 us; no-deadline base 39.975 / 39.956 / 40.507 us vs target 40.355 / 39.834 / 39.789 us. Intervals overlap in every round and all figures sit well under the BENCHMARKS.md regression thresholds of 42.26 / 41.72 us. No regression; agree that BENCHMARKS.md should not move.

AC #3 negative control verified independently rather than taken from the handoff. In a throwaway worktree at 39cbbb3 with the two clamps reverted to their pre-change form, all three tests fail with exactly the described out-of-band values: quiescence returned Score(-20101) (window Score(-20102)..Score(-20101)), search returned Score(20100) rendered Mate(0) (window Mate(0)..Score(20101)), and the TASK-54 regression panics the search thread on quiescence returned Score(20101). Restored, all three pass. The worktree was removed and the review worktree left clean.

On the reviewer note — clamping a bound outwards is sound, confirmed by working the case through. Take the parent null window at the bottom of the band: alpha = mate(0), so the child receives (child_bound(alpha.inc_one()), child_bound(alpha)) = (Score(20100), Score(20101)), entirely above the band. Both ends clamp to mate(1), alpha >= beta fires, and the child returns mate(1). That is a valid fail-low upper bound unconditionally, since no node scores above mate(1). The parent computes neg().inc_mate() = Score(-20098) = mate(-2), an even-ply negative mate, so Display's parity assertion holds; and mate(-2) is a true lower bound on the parent because a parent with a legal move cannot be mated in zero. Under the old one-sided clamp the same path returned Score(20100), which the parent mapped to Score(-20099) — an odd-ply negative mate, precisely the wrong-parity value that panics Display. The widening of scope to search as well as quiescence is justified and correctly described in the notes.

Also confirmed the null-window invariant survives the clamp: for an incoming (a, a+1), either both ends stay in band unchanged, or they collapse to the same clamped value and the alpha >= beta early return fires first, so debug_assert!(Node::pv() || alpha.inc_one() == beta) still holds for every child window. In-band windows are bit-identical to the old behaviour, which is consistent with the flat benchmark and the unchanged test results.

Non-blocking observations, no action required for this task:
- The Implementation Plan still describes step 2 as saturating child_bound to the infinities. That approach was tried, found to break null-window preservation, and backed out; the Implementation Notes record this clearly, but the plan text was not updated to match the final solution.
- Display was deliberately not made total: Score(20101) would still trip its parity assertion. That is consistent with AC #4, which scopes to values search can generate, and the new band assertions are what keep such values from reaching it.
- Debug renders Score(20100) as Mate(0), inside the documented encoding band but outside the node score band, distinguished from mate(0) only by the sign in Mate(-0). Legible but subtle when reading assertion output.

All five acceptance criteria are checked on objective evidence. Approving 39cbbb3 and moving to Ready to Merge.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Clamps both alpha and beta into the node score band `mate(0)..=mate(1)` on entry to both `search` and `quiesce`, so the exact-but-out-of-band values `Score::child_bound` produces are consumed rather than returned. `child_bound` itself is left exact, preserving null-window mapping. `search` and `quiesce` are wrapped with a `debug_assert` on `Score::is_node_score`, `Debug` now renders unencodable values in raw `Score(n)` form instead of a plausible-looking variant, and the `Score` doc comment separates node scores from window bounds. Verified at 39cbbb3 with cargo fmt --check, a clean-CARGO_TARGET_DIR clippy (zero warnings), cargo test --workspace, the ignored WAC sweep (900 debug searches, 354s), and a 3-round alternating base/target `cargo bench --bench search` showing no regression.
<!-- SECTION:FINAL_SUMMARY:END -->
