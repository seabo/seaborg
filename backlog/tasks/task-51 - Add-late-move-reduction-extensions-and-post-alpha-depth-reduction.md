---
id: TASK-51
title: 'Add late move reduction, extensions, and post-alpha depth reduction'
status: Done
assignee:
  - '@george'
created_date: '2026-07-18 18:30'
updated_date: '2026-07-21 01:18'
labels: []
dependencies:
  - TASK-50
  - TASK-64.1
  - TASK-64.2
references:
  - engine/src/search.rs
priority: medium
type: feature
ordinal: 51000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Search steps 16 and 17 are unimplemented placeholders, and a third TODO inside the move loop records the same idea from another angle. All three are one mechanism and cannot be split without shipping half a feature.

Step 16, reductions and extensions: extend promising lines (for example check evasions) and reduce unpromising ones before searching.
Step 17, late move reduction: search moves late in the ordering at reduced depth, re-searching at full depth only when the reduced search beats alpha.
engine/src/search.rs:692: after a move raises alpha, reduce the depth used for the remaining moves. This is LMR bookkeeping and belongs with the above.

LMR quality depends directly on move-ordering quality, which depends on SEE. TASK-49 corrects SEE for promotions; it is not a hard blocker here, but if TASK-49 is still open when this is picked up, note the interaction when interpreting strength results.

The numbered Step N comments in search.rs are a deliberate map of the intended search structure. Replace the TODO markers with implementations; do not delete the step comments.

Sequencing: gated on TASK-50 so that strength changes can be attributed to one technique at a time.

TODO sites: engine/src/search.rs:637, :640, :692.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Late move reduction is implemented with a re-search at full depth whenever the reduced search raises alpha
- [x] #2 Reductions and extensions are implemented at step 16 and are not applied in PV nodes where they would truncate the principal variation
- [x] #3 Remaining moves after an alpha raise are searched at the reduced depth described at search.rs:692
- [x] #4 The reported principal variation remains legal and complete under reduction, verified against the regression coverage added by TASK-36
- [x] #5 Measured with the TASK-27 strength-regression script showing no strength loss, with results recorded in the implementation notes
- [x] #6 The step 16 and step 17 TODO markers and the search.rs:692 TODO are replaced by implementations, with the numbered step comments retained
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Compute a per-node in-check flag once before the move loop; reuse for the extension.
2. Step 16 (extensions): check-evasion extension of +1 ply when the side to move is in check; new_depth = depth - 1 + extension. All child searches in the loop use new_depth.
3. Step 17 (LMR): compute a reduction r for quiet, non-extended, late moves at depth >= 3, applied to the zero-window scout at Step 19. Re-search at full new_depth whenever the reduced scout raises alpha (AC#1). Gate under a test-only lmr_disabled hook mirroring forward_pruning.
4. Post-alpha (former :692 TODO): once a move raises alpha in a PV node, treat remaining quiet moves as reducible even before the late-move threshold and take an extra ply off (AC#3). Replace the bare TODO with the mechanism comment; effect realized at Step 17 via did_raise_alpha.
5. Reductions never truncate the PV: move 1 is never reduced, and any reduced scout that beats alpha is re-searched at full depth before a PV re-search populates the PV table (AC#2).
6. Retain the numbered Step 16/17 comments; replace the three TODO markers with implementations (AC#6).
7. Tests: LMR does not change sound (forced-mate/clean-win) results vs lmr-disabled; LMR reduces the tree; existing TASK-36 PV-legality tests still pass (AC#4). Run fmt, clippy, cargo test. Attempt TASK-27 strength smoke and record (AC#5).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation

Implemented the single reductions/extensions/LMR mechanism in engine/src/search.rs, replacing the three TODO markers (former Step 16, Step 17, and the post-alpha-raise site) while retaining all numbered step comments.

- Step 16 (extensions): check-evasion extension. When the side to move is in check, every move is a forced evasion; the whole subtree is searched one ply deeper (new_depth = depth - 1 + extension). Extensions only add depth, so they never truncate the PV.
- Step 17 (LMR): the reduction is decided just after the move is made (labelled 'Step 17 (applied)'), where whether the move gives check is known. A late, quiet, non-checking, non-extended move is scouted at new_depth - reduction with a zero window; if that reduced scout raises alpha it is re-searched at full new_depth (Step 19). A reduced scout that fails low is trusted. The first move is never reduced, and any reduced move that would enter the PV is re-searched at full depth before Step 20's PV search writes the PV table, so the reported PV always rests on a full-depth search.
- Post-alpha-raise (former search.rs:692 TODO): once a move raises alpha in a PV node, did_raise_alpha is set; Step 17 reads it so the remaining moves are reduced immediately rather than waiting for the late-move-count threshold.
- Reduction schedule is deliberately shallow (1 ply, or 2 for deep-and-late moves) so short forcing lines stay visible to the reduced scout.
- Added test hooks lmr_disabled and extensions_disabled mirroring forward_pruning_disabled.

## Regression coverage adjustments

- gives_correct_answers suite: two of ~20 positions shifted, best move correct in both. '2q4k/3r3p/2p2P2/p7/2P5/P2Q2P1/5bK1/1R6 w' depth 6->7 (LMR defers the mate score by one iteration; d3d7 found at every depth, mate verified at depth 7). 'r5k1/p1P5/8/8/8/8/3RK3/8 w' upper bound cp955->cp985 (check-evasion extension searches deeper; d2d8 unchanged).
- Two TT/halfmove-clock tests (warm_table_reuse_agrees..., the_same_key_is_worth_different_scores...) now disable lmr/extensions in their local search: LMR's move-ordering dependence and the extension's extra ply are orthogonal to the clock invariant they pin, and would otherwise move the exact scores those tests assert.

## New tests

- late_move_reduction_does_not_change_sound_search_results (lmr on == off on decisive non-mate positions)
- late_move_reduction_reduces_the_search_tree (lmr on visits strictly fewer nodes)
- the_check_evasion_extension_deepens_an_in_check_search

## Strength measurement (AC #5)

Controlled round-robin, candidate (this branch, 356c776, the reviewed implementation target) vs baseline (merge-base 6d3d4ac, the only diff being this change), fastchess, reproducible node limit nodes=100000, openings-v1.epd, 16MB hash, single worker.
- 20-game harness smoke (tools/strength/strength_test.py --mode smoke): completed cleanly, no illegal moves / crashes; SPRT inconclusive at 20 games (LLR 0.14), as expected for that sample size.
- 200-game direct match: Elo +188.5 +/- 51.5, nElo +215.8, LOS 100.0%, 137W / 38L / 25D (74.75%), Ptnml [0,13,25,12,50]. No crashes or illegal moves.
Result is a decisive strength gain, well beyond the 'no strength loss' bar. The node-limited budget is a fair test: LMR converts the nodes it saves into greater search depth.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-21 00:01
---
Implementation handoff
Branch: task-51-lmr-extensions-post-alpha-reduction
Worktree: /Users/seabo/seaborg-worktrees/task-51-lmr-extensions-post-alpha-reduction
Base: 6d3d4ac98a40a455959b4cea18d0b0a82b0c7867
Implementation target: 356c776bc8897be983e54f18e733a9aebcdbd699
Resolved findings: none (new work)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (engine lib 306 passed / 2 ignored; workspace 0 failures)
- Strength (fastchess, nodes=100000, base 6d3d4ac vs target): +188.5 +/- 51.5 Elo over 200 games (137W/38L/25D), LOS 100%, no illegal moves or crashes
Known failures: none
---

author: @reviewer
created: 2026-07-21 00:41
---
Review attempt: 1
Reviewed branch: task-51-lmr-extensions-post-alpha-reduction
Reviewed implementation: 356c776bc8897be983e54f18e733a9aebcdbd699
Base: 6d3d4ac98a40a455959b4cea18d0b0a82b0c7867
Verdict: changes_requested

Scope of review: full base..target diff (engine/src/search.rs only, plus task file). Immutability confirmed: target is an ancestor of the branch tip and the only later commit (39a4bb5) touches the task markdown alone.

What passes:
- AC#1 LMR with full-depth re-search on an alpha-raising reduced scout: implemented (search.rs:1540-1552).
- AC#2 extensions/reductions at Step 16, PV never truncated: the check-evasion extension only adds depth; reduced scouts that beat alpha are re-searched at full new_depth before the Step 20 PV search writes the PV table.
- AC#3 remaining moves reduced after an alpha raise: did_raise_alpha gates the LMR condition (search.rs:1511, 1609-1613).
- AC#4 PV legality: reported_principal_variations_are_legal and a_node_searched_past_the_nominal_horizon_still_reports_a_legal_pv pass with LMR/extensions active.
- AC#6 the three TODO markers (Step 16, Step 17, former :692) are replaced with implementations and all numbered step comments are retained. Remaining TODOs at Steps 10/11/13/14 are unrelated future techniques, out of scope.
- No new #[allow]. Test hooks lmr_disabled/extensions_disabled are #[cfg(test)] only.
- The two shifted suite expectations (2q4k... depth 6->7; r5k1... upper bound cp955->cp985) preserve the best move and are legitimate consequences of the reduction/extension.

Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings (clean CARGO_TARGET_DIR): pass, no warnings
- cargo test --workspace: pass (engine lib 306 passed / 2 ignored; workspace 0 failures)

Blocking findings:

REV-1-01 [P2] AC#5 strength result is recorded against a no-op commit, not the implementation target
Location: task-51 Implementation Notes, "Strength measurement (AC #5)" section.
Impact: AC#5 requires the strength results recorded in the notes. The notes name the candidate as "(this branch, cbdfe4c)". cbdfe4c ("claim and plan") changes only the task markdown: `git diff 6d3d4ac cbdfe4c` touches no code, so cbdfe4c is byte-identical in engine behaviour to base 6d3d4ac. A match of cbdfe4c vs 6d3d4ac would compare two identical engines and score ~0 Elo, which contradicts the recorded +188.5. The recorded provenance is self-contradictory and, as written, does not attribute the measured gain to the implementation under review (356c776). This record merges to master.
Reproduction: `git diff --stat 6d3d4ac cbdfe4c` -> only the task .md changes; no engine sources.
Expected: Correct the candidate identity in the notes to the reviewed implementation target 356c776 (the +188.5 figure and the smoke / 200-game descriptions are otherwise consistent with a 356c776-vs-6d3d4ac match). No code change is required.

Verification (independent confirmation that 356c776 is the strong commit and the fix direction is unambiguous):
- Controlled node-limited match, target 356c776 vs base 6d3d4ac, fastchess nodes=100000, option.Hash=16, openings-v1.epd, 100 games: Elo +186.25 +/- 74.13, nElo +211.19, LOS 100.00%, 74.5% (68W/19L/13D), Ptnml [0,7,12,6,25]. Reproduces the reported gain and confirms 356c776 (not cbdfe4c) is the measured candidate.
---

author: @reviewer
created: 2026-07-21 00:50
---
Resolved REV-1-01: corrected the AC#5 strength-measurement candidate in the implementation notes from cbdfe4c (task-md only, no code) to 356c776, the reviewed implementation target. Applied under an explicit human override of the implementer/reviewer split for this notes-only textual fix; no code changed (git diff 356c776 -- ':!backlog/' is empty).
---

author: @reviewer
created: 2026-07-21 00:51
---
Review attempt: 1 (continued)
Reviewed branch: task-51-lmr-extensions-post-alpha-reduction
Reviewed implementation: 356c776bc8897be983e54f18e733a9aebcdbd699
Base: 6d3d4ac98a40a455959b4cea18d0b0a82b0c7867
Verdict: approved

REV-1-01 [P2] resolved: the AC#5 strength-measurement notes now name the candidate as 356c776 (the reviewed implementation target) rather than the no-op commit cbdfe4c. Notes-only correction, applied under an explicit human override of the implement/review split; no implementation file changed (git diff 356c776 -- ':!backlog/' is empty).

All acceptance criteria proven:
- AC#1: LMR with a full-depth re-search whenever the reduced scout raises alpha (search.rs:1540-1552).
- AC#2: Step 16 extensions/reductions; the check-evasion extension only adds depth and any reduced scout that beats alpha is re-searched at full new_depth before the Step 20 PV search writes the PV table, so PV nodes are never truncated.
- AC#3: remaining moves after an alpha raise are reduced immediately via did_raise_alpha (search.rs:1511, 1609-1613).
- AC#4: reported_principal_variations_are_legal and a_node_searched_past_the_nominal_horizon_still_reports_a_legal_pv pass with LMR/extensions active.
- AC#5: no strength loss, recorded in notes and independently reproduced (see below).
- AC#6: the three TODO markers replaced; all numbered step comments retained (remaining TODOs at Steps 10/11/13/14 are unrelated future work).

Verification commands and results (run on the implementation target code, identical across the metadata-only commits):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings (clean CARGO_TARGET_DIR): pass, no warnings
- cargo test --workspace: pass (engine lib 306 passed / 2 ignored; workspace 0 failures)
- Independent controlled strength match, target 356c776 vs base 6d3d4ac, fastchess nodes=100000, option.Hash=16, openings-v1.epd, 100 games: Elo +186.25 +/- 74.13, nElo +211.19, LOS 100.00%, 74.5% (68W/19L/13D), Ptnml [0,7,12,6,25].

Approved implementation SHA: 356c776bc8897be983e54f18e733a9aebcdbd699. No implementation file changed between the target and this approval commit.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented the unified reductions/extensions/LMR mechanism in engine/src/search.rs (Step 16 check-evasion extension, Step 17 late move reduction with a full-depth re-search on an alpha-raising reduced scout, and post-alpha-raise reduction via did_raise_alpha), replacing the three TODO markers while retaining all numbered step comments. All six acceptance criteria verified: cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings (clean CARGO_TARGET_DIR), and cargo test --workspace all pass (engine 306 passed / 2 ignored); TASK-36 PV-legality tests pass with LMR/extensions active; and an independent controlled node-limited match (target 356c776 vs base 6d3d4ac, fastchess nodes=100000, 16MB hash, openings-v1.epd, 100 games) reproduced the reported strength gain at +186.25 +/- 74.13 Elo (74.5%, LOS 100%), confirming no strength loss.
<!-- SECTION:FINAL_SUMMARY:END -->
