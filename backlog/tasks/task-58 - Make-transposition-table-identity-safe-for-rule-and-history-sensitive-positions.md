---
id: TASK-58
title: >-
  Make transposition-table identity safe for rule- and history-sensitive
  positions
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-19 00:00'
updated_date: '2026-07-19 03:38'
labels:
  - transposition-table
  - zobrist
  - search
  - correctness
  - rules
dependencies: []
references:
  - core/src/position/zobrist.rs
  - core/src/precalc/zobrist.rs
  - engine/src/search.rs
priority: high
type: bug
ordinal: 57000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Zobrist key identifies board state, side to move, castling rights, and en-passant file, but search values also depend on the halfmove clock and potentially on repetition history. Static evaluation is explicitly scaled by the halfmove clock, so identical keys can currently carry different values. Establish and enforce a documented TT-reuse policy for halfmove-clock and repetition-sensitive results. Also canonicalise en-passant hashing so an unusable target does not split positions with identical legal state.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The transposition-table policy prevents the known halfmove-clock incompatibility by using position-intrinsic leaf evaluation, suppressing history-sensitive writes, and conservatively gating reads near the fifty-move boundary; the fixed horizon allowance is accepted as an engineering safeguard rather than a proof against every theoretical extension sequence
- [ ] #2 Repetition-derived values are not stored as position-intrinsic TT information, and the known rare read-side graph-history limitation is explicitly documented and accepted without widening or re-keying the packed transposition-table entry
- [ ] #3 Positions that differ only by an en-passant target which cannot affect any legal move have the same canonical transposition identity, while a legally relevant en-passant right remains distinguished
- [ ] #4 Regression tests cover materially different halfmove clocks, read gating near the fifty-move boundary, suppression of repetition- and fifty-move-derived writes, continued storage of history-independent values, and capturable versus non-capturable en-passant targets
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Rework for review attempt 1 (REV-1-01, REV-1-02).

REV-1-02 en-passant legality (scope confirmed with user this round, overriding the previous narrower call):
1. Add Position::has_legal_ep_capture(ep, capturer) in core/src/position/mod.rs. For each enemy pawn that attacks the target, simulate the resulting occupancy (capturer pawn off its origin, double-pushed pawn removed, pawn onto the target) and test whether the capturer's king is attacked under that occupancy by any piece type, not only sliders. The existing legal_move ep branch tests sliders only, which is sound there because movegen has already restricted evasions, but is not sufficient as a standalone predicate: an ep capture that leaves a knight or pawn check unresolved is illegal too. Computed from bitboards, not State, so it is callable mid-update.
2. Call it from both derivations so parsed and played positions cannot diverge: make_move_unchecked (mod.rs, replacing the pseudo-legal pawn-attack predicate) and canonicalize_ep_square (fen.rs). Applying it to only one path would reopen the parsed-vs-played key split that AC#3 exists to close.

REV-1-01 halfmove clock (read side):
3. The write side is already sound: Step 24 suppresses the write whenever the subtree claimed a fifty-move or repetition draw, so a stored value never embeds one. The unfixed half is the read side, where a value computed where the rule was irrelevant is cut off against at a clock where it is not. Gate both cutoff sites (main search Step 4, quiescence Step 3) on a clock-relevance predicate: reuse only when the fifty-move boundary cannot be reached within the reused horizon. Document the predicate as conservative with respect to quiescence and check extensions, which are not bounded by the entry depth.

REV-1-01 repetition (read side): NOT fixed, by decision this round. Full soundness needs TT entries keyed or gated by path history; Entry is a fully packed u64 with no free bits, so it means widening the entry and reworking tt.rs layout, replacement and sizing. Confirmed with the user to document the limitation rather than take that on inside this task, and to leave the scope call visible to the reviewer.

4. Tests: warm-table reuse refused across the fifty-move boundary and still permitted well below it; pinned-capturer and discovered-check en-passant targets dropped from identity while a genuinely legal target is retained; parsed-versus-played key agreement over the new predicate on both paths.
5. Run cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, cargo test --workspace.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Made the static evaluation position-intrinsic (engine/src/search.rs). The previous halfmove-clock scaling made every propagated score a function of state the Zobrist key does not cover, which is the direct cause of AC#1: a warm table could return a score computed under a materially different clock. Removing the scaling makes the invariant hold by construction. The alternative considered was storing a clock in the packed 64-bit TT Entry and gating reuse; Entry has no free bits, and a strict equality gate would reject most reuse while a tolerance band has no clean correctness story. Approach confirmed with the user before implementing.

Suppressed the TT write for any node whose subtree claimed a history-sensitive draw (search.rs Step 24), tracked by a monotone history_draws counter incremented in the new is_history_draw helper. Downgrading Exact to a bound is insufficient and the comment at the write site records why: a draw score can raise a value to a beta cutoff as easily as it can cap it, so Lower and Upper are unsound in an incompatible history too. Verified the policy is not a blanket suppression: a_history_independent_value_is_still_stored_in_the_table asserts ordinary values still reach the table.

Canonicalised the en-passant target on FEN input (core/src/position/fen.rs, canonicalize_ep_square), applying the same enemy-pawn-attacks predicate make_move_unchecked already uses, and running it before set_zobrist so the canonical square is the one hashed. This closes the pre-existing TODO at fen.rs. Note AC#3 was narrower than the task assumed: make_move_unchecked already declined to set an unusable target, so the real gap was FEN input. Full legality filtering (pinned capturer, ep-discovered-check) is deliberately out of scope, as it requires legality checks inside make_move on the hot path; scope confirmed with the user.

Found and fixed two defects while establishing the above.

First, Zobrist::from_position folded a Piece::None key into every empty square, because iterating Board yields all 64 squares including empty ones, while the incremental updates in make_move_unchecked only ever toggle real pieces. The two derivations disagreed by the Piece::None keys of every square whose occupancy changed. Confirmed the divergence reproduces at the base commit on unmodified master, and confirmed the delta numerically equals piece_square_key(None, orig) xor piece_square_key(None, dest). This was a blocking prerequisite for AC#3 rather than opportunistic scope: the en-passant identity guarantee is asserted across exactly the parsed-versus-played boundary that this bug split, so AC#3 is untestable without it. Added incremental_and_full_keys_agree_after_every_legal_move and unmaking_a_move_restores_the_key over quiet moves, captures, castling, promotions and en-passant.

Second, quiescence compared the halfmove clock against 50 while the main search used the correct 100-ply boundary, so quiescence reported a draw at 25 moves. Both now route through is_history_draw.

Behavioral note for review: removing the eval scaling changes engine playing behavior in drawish endings, which is intended but strength-relevant. The existing quiescence_searches_quiet_check_evasions expectation moved from -495 to -500; the -495 was the clock smear (the evasion search advanced the clock to 1 and shaded a -500 rook by one percent), so -500 is the corrected value. No benchmark was run: the ACs do not require one and I did not want to record an uncontrolled measurement, so nps attribution is left to the merge-time gate.

## Rework for review attempt 1

Resolved REV-1-01 (halfmove-clock half) and REV-1-02. The repetition half of REV-1-01 is deliberately not closed; see below.

Resolved REV-1-01 (halfmove clock). The finding was correct and the previous round's reasoning was wrong in a specific way: making evaluate() position-intrinsic makes the *leaf* value clock-independent, but a propagated value still reflects any fifty-move draw reachable inside its own subtree. Added Search::clock_permits_tt_reuse and applied it at both cutoff sites (main search Step 4, quiescence Step 3). Reuse is permitted only while the boundary is out of reach of the stored depth plus HORIZON_SLACK. The slack is an allowance for quiescence and check extensions, which are not bounded by the entry depth, and the doc comment states plainly that it is a conservative allowance and not a proof; erring high costs hit rate only, and only near the boundary. Corrected the evaluate() doc comment, which claimed reuse was sound 'whatever the clock reads there' - that overclaim is what the finding caught.

The previous round's warm-table test was replaced rather than extended. It asserted only that a warm and a cold search agreed, which held whether or not the gate was present, so it could not have caught the defect. The replacements seed a table entry directly and drive a single NonPv node, which pins the cutoff path under test. Verified each new test fails with the gate disabled and passes with it.

Resolved REV-1-02. Added Position::has_legal_ep_capture, which simulates the post-capture occupancy for each candidate capturer and tests whether the capturer's king is left attacked. It checks every piece type, not just sliders: the existing legal_move en-passant branch tests sliders alone, which is sound there because movegen has already restricted its input to check evasions, but as a standalone predicate would call a capture legal while it left a knight or pawn check unanswered. Applied to both derivations - make_move_unchecked and canonicalize_ep_square. Applying it to FEN alone, which is what the finding literally asked for, would have reopened the parsed-versus-played key split that AC#3 exists to close; the hot-path cost was confirmed with the user this round, overriding the narrower scope recorded in the previous round. Perft passes unchanged, which is the substantive evidence that movegen semantics are preserved.

Also found while writing the predicate: FEN can name a target with no double-pushed pawn behind it. The occupancy simulation XORs that square, so without a guard a malformed FEN would conjure a phantom blocker and the answer would be arbitrary. The predicate now requires the pawn to be present. Covered by an_en_passant_target_with_no_pawn_behind_it_is_dropped.

NOT resolved: the repetition half of REV-1-01. The finding is correct that a value computed in a history where nothing repeated can be reused on a path where a descendant now repeats a pre-root position. This is the graph-history-interaction problem, and closing it needs entries keyed or gated by path history. tt::Entry is a fully packed u64 (sig16 + depth8 + gen_bound8 + score16 + move16) with no spare bits, so it means widening the entry and reworking the table's layout, replacement policy and sizing - structural change well beyond this task. Confirmed with the user this round to document the limitation rather than take that on here, and to leave the scope call visible to the reviewer rather than resolve it silently. The policy, the three rules that are enforced, and this specific gap are documented together on Search::history_draws.

Benchmarks, measured this session round-robin against base 5ce2782 in a detached worktree, three interleaved rounds for perft and two for search (Criterion medians):
- perft 5: base 22.197 / 22.129 / 22.204 ms, target 22.295 / 22.328 / 22.407 ms - about +0.8%
- search startpos depth 7: base 39.418 / 39.700 us, target 40.243 / 40.354 us - about +1.9%
Both regressions are consistent in direction across every round and well inside the 5% investigation threshold in BENCHMARKS.md. The cost is the added en-passant legality work in make_move plus the clock test at the cutoff sites. Not compared against the BENCHMARKS.md table, which was measured in another session on another commit. The machine was not fully idle (load average about 6, no competing benchmark), which is why the comparison is interleaved rather than sequential.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-19 02:36
---
Implementation handoff
Branch: task-58-tt-identity-rule-sensitive
Worktree: /Users/seabo/seaborg-worktrees/task-58-tt-identity-rule-sensitive
Base: 5ce2782948b96d070404dc73b0a91c89330a3709
Implementation target: 99e9b4f
Resolved findings: none (first review)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 40 core + 193 engine + 5 build_metadata + 1 doc test, 0 failed, 2 ignored (pre-existing)
Known failures: none

Reviewer notes:
- Two design decisions were confirmed with the user before implementing: satisfy AC#1 by removing the halfmove-clock scaling from evaluate() rather than gating TT reuse on a stored clock, and scope AC#3 to FEN reconciliation rather than full en-passant legality filtering. Both rationales are in the implementation notes.
- The change to Zobrist::from_position alters every Zobrist key in the engine. It is a fix for a real divergence between the full and incremental key derivations that reproduces on unmodified master, and is a prerequisite for AC#3 rather than opportunistic scope. Worth independent confirmation that it is correctly in scope for this task.
- Removing the eval scaling is a strength-relevant behavior change in drawish endings. No benchmark was run; nps and strength attribution is left to the merge-time gate.
---

author: @codex
created: 2026-07-19 02:42
---
Review attempt: 1
Reviewed branch: task-58-tt-identity-rule-sensitive
Reviewed implementation: 99e9b4f
Verdict: changes_requested

REV-1-01 [P1] Warm TT reads remain incompatible with halfmove clock and repetition history
Location: engine/src/search.rs:629-678 and engine/src/search.rs:1007-1044
Impact: AC#1 and AC#2 remain unsatisfied. The patch suppresses writes only after the current search observes a history-sensitive draw, but an entry written by a low-clock or history-independent search is accepted solely by key, depth, and bound. A later incompatible visit can return it before searching the subtree that would claim the draw.
Reproduction: Warm the table at halfmove clock 0, then search the same keyed position near clock 100 deeply enough that a reversible continuation reaches the boundary. The existing clock-80/depth-4 test never reaches incompatibility. The same ancestor-level issue applies when another repetition history makes a descendant draw relevant.
Expected: Gate or key TT reads so scores and bounds are reused only in compatible rule/history state, with material incompatible-history regressions.

REV-1-02 [P1] En-passant canonicalization retains legally unusable targets
Location: core/src/position/fen.rs:113-132
Impact: AC#3 requires equal identity when a target cannot affect any legal move. The pawn-attack predicate retains a target when the sole capturer is pinned or the capture exposes its king.
Reproduction: Compare "k3r3/8/8/3pP3/8/8/8/4K3 w - d6 0 1" with the same FEN using "-" for en passant. e5xd6 exposes the rook check and is illegal, yet the target is retained and hashed.
Expected: Retain the target only when at least one legal en-passant capture exists, with pinned/discovered-check regression coverage.

Verification:
- cargo fmt --check: pass
- clean CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (40 core, 193 engine, 5 build_metadata, 1 doc; 2 ignored)
- git diff --check base..target: pass
- benchmarks not run because another repository benchmark consumed about 90% CPU, preventing a valid idle-machine comparison
---

author: @codex
created: 2026-07-19 03:09
---
Implementation handoff
Branch: task-58-tt-identity-rule-sensitive
Worktree: /Users/seabo/seaborg-worktrees/task-58-tt-identity-rule-sensitive
Base: 5ce2782948b96d070404dc73b0a91c89330a3709
Implementation target: a76792d
Resolved findings: REV-1-01 (halfmove-clock half only), REV-1-02
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 43 core + 197 engine + 5 build_metadata + 1 doc, 0 failed, 2 ignored (pre-existing)
- cargo test --release -p engine perft_suite: pass (movegen semantics preserved under the new en-passant predicate)
- negative controls: each new regression test verified to fail with its fix disabled
Known failures: none

Reviewer notes:
- REV-1-01 is only partly resolved, and this is the main thing to judge. The halfmove-clock half is fixed. The repetition half is not, by a scope decision confirmed with the user this round: closing it needs TT entries keyed or gated by path history, and Entry is a fully packed u64 with no spare bits, so it means widening the entry and reworking tt.rs layout, replacement and sizing. The policy and the exact gap are documented on Search::history_draws. If you judge AC#2 to require the read side closed, this should become a separate task rather than more rework here.
- REV-1-02 is resolved more broadly than the finding asked. The finding named fen.rs, but fixing only fen.rs would reopen the parsed-versus-played key split that AC#3 exists to close, so the legality predicate is applied to make_move_unchecked too. That is a hot-path change; scope confirmed with the user this round, overriding the narrower call recorded in the previous round.
- The previous round's warm-table clock test was replaced, not extended. It could not have caught the defect, which is what your reproduction step observed. The replacements seed the entry directly and drive a single NonPv node, and each was verified to fail with the gate disabled.
- HORIZON_SLACK is a conservative allowance for quiescence and check extensions, not a proof of soundness; the doc comment says so. If you want a tighter argument, the alternative is bounding quiescence depth, which is a separate change.
- Benchmarks were run this session, round-robin against base in a detached worktree: perft 5 about +0.8%, depth-7 search about +1.9%, consistent across rounds, well inside the 5% BENCHMARKS.md threshold. Numbers and method are in the implementation notes. Not compared against the BENCHMARKS.md table, which is from another session and commit.
---

author: @codex
created: 2026-07-19 03:25
---
Review attempt: 2\nReviewed branch: task-58-tt-identity-rule-sensitive\nReviewed implementation: a76792d\nVerdict: changes_requested\n\nREV-1-01 [P1] Repetition-incompatible TT reads remain unguarded (still open)\nLocation: engine/src/search.rs:420-448 and engine/src/search.rs:686-711\nImpact: AC#2 and the incompatible-repetition portion of AC#4 remain unsatisfied. Write suppression prevents storing a value after this search observes a repetition, but an entry computed in a history with no repetition is still accepted in a different history where a descendant repeats a pre-root position. The implementation documentation explicitly identifies and accepts this gap; that is not enforcement of the acceptance criterion.\nReproduction: Seed or warm an exact entry in a history-independent visit, then revisit the same keyed node with a path history in which a descendant reaches threefold. Step 4 accepts the entry using key, depth, bound, and halfmove clock only, before that descendant is searched.\nExpected: Gate or key TT reads using compatible repetition state, and add compatible/incompatible repetition-history regression tests that exercise the cutoff read path.\n\nREV-2-01 [P1] Fixed horizon slack does not guarantee halfmove-clock compatibility\nLocation: engine/src/search.rs:982-1007\nImpact: AC#1 says an incompatible clock result cannot be reused. The 16-ply allowance is explicitly documented as not being a proof, while quiescence and check extensions can follow an unbounded quiet-evasion sequence. A cutoff can therefore be admitted when the fifty-move boundary is reachable beyond depth + 16, reusing a value that ignores the draw.\nReproduction: clock_permits_tt_reuse returns true whenever clock + entry_depth + 16 < 100, even though the implementation states that the actual forcing horizon has no tight static bound. The focused tests validate only arithmetic around the chosen constant, not the claimed incompatibility guarantee.\nExpected: Use a sound compatibility condition or key, or conservatively refuse reuse whenever the unbounded extension horizon could reach the rule, and cover the boundary with a behavioral regression.\n\nVerification:\n- cargo fmt --check: pass\n- clean CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings: pass\n- cargo test --workspace: pass (43 core, 197 engine, 5 build_metadata, 1 doc; 2 ignored)\n- cargo test --release -p engine perft_suite: pass\n- git diff --check base..target: pass\n- implementation target ancestry and task-only handoff commit: pass
---

author: @codex
created: 2026-07-19 03:26
---
Review attempt: 2
Reviewed branch: task-58-tt-identity-rule-sensitive
Reviewed implementation: a76792d
Verdict: changes_requested

REV-1-01 [P1] Repetition-incompatible TT reads remain unguarded (still open)
Location: engine/src/search.rs:420-448 and engine/src/search.rs:686-711
Impact: AC#2 and the incompatible-repetition portion of AC#4 remain unsatisfied. Write suppression prevents storing a value after this search observes a repetition, but an entry computed in a history with no repetition is still accepted in a different history where a descendant repeats a pre-root position. The implementation documentation explicitly identifies and accepts this gap; that is not enforcement of the acceptance criterion.
Reproduction: Seed or warm an exact entry in a history-independent visit, then revisit the same keyed node with a path history in which a descendant reaches threefold. Step 4 accepts the entry using key, depth, bound, and halfmove clock only, before that descendant is searched.
Expected: Gate or key TT reads using compatible repetition state, and add compatible/incompatible repetition-history regression tests that exercise the cutoff read path.

REV-2-01 [P1] Fixed horizon slack does not guarantee halfmove-clock compatibility
Location: engine/src/search.rs:982-1007
Impact: AC#1 says an incompatible clock result cannot be reused. The 16-ply allowance is explicitly documented as not being a proof, while quiescence and check extensions can follow an unbounded quiet-evasion sequence. A cutoff can therefore be admitted when the fifty-move boundary is reachable beyond depth + 16, reusing a value that ignores the draw.
Reproduction: clock_permits_tt_reuse returns true whenever clock + entry_depth + 16 < 100, even though the implementation states that the actual forcing horizon has no tight static bound. The focused tests validate only arithmetic around the chosen constant, not the claimed incompatibility guarantee.
Expected: Use a sound compatibility condition or key, or conservatively refuse reuse whenever the unbounded extension horizon could reach the rule, and cover the boundary with a behavioral regression.

Verification:
- cargo fmt --check: pass
- clean CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (43 core, 197 engine, 5 build_metadata, 1 doc; 2 ignored)
- cargo test --release -p engine perft_suite: pass
- git diff --check base..target: pass
- implementation target ancestry and task-only handoff commit: pass

Formatting note: this supersedes comment #4, whose newlines were escaped by the CLI invocation.
---

author: @human
created: 2026-07-19 03:38
---
Human scope decision, 2026-07-19: accept the implementation as a pragmatic correctness improvement. Full mathematical immunity to every unbounded check-extension sequence and fully history-keyed repetition-safe TT reads are not required for TASK-58. The remaining cases are documented, rare search-cache misvaluation risks; widening or re-keying the packed TT entry would add disproportionate memory, performance, and implementation complexity. Acceptance criteria have been revised to describe the policy actually required. No follow-up task is requested unless engine testing later demonstrates practical impact.
---
<!-- COMMENTS:END -->
