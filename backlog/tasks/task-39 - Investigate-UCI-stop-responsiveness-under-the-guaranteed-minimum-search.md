---
id: TASK-39
title: Investigate UCI stop responsiveness under the guaranteed-minimum search
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-18 11:46'
updated_date: '2026-07-18 20:12'
labels:
  - engine
  - search
  - uci
dependencies:
  - TASK-32
references:
  - engine/src/search.rs
documentation:
  - doc-3
priority: medium
type: bug
ordinal: 39000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TASK-32 fixed 'bestmove 0000' forfeits by adding Search::min_search_complete: Search::stopping() (engine/src/search.rs:761) returns false until the first iterative-deepening iteration completes, so nothing can abort the search before a legal root move exists. Deliberately, and as documented in the TASK-32 implementation notes, this suppresses BOTH abort sources during that window: the time deadline AND the cancellation flag.

Suppressing the cancellation flag is what makes the guarantee absolute (an immediate 'stop' arriving during ply 1 otherwise still produced 'bestmove 0000'), but it means a UCI 'stop' cannot interrupt the first ply. The UCI specification expects the engine to stop searching and return a bestmove as soon as possible after 'stop'. seaborg now defers that until ply 1 finishes.

In practice this looks safe and bounded. Measured on the TASK-32 build, 'go infinite' followed immediately by 'stop' returned a legal bestmove in roughly 10-150ms wall clock, and that figure includes process startup and magic-table initialization, so the actual suppressed window is far smaller. Ply 1 is a depth-1 search, and the loop always starts at d=1 (search.rs:445), so the window cannot widen with the requested depth.

The residual concern is that the bound is asserted from measurement on a handful of positions rather than established by argument. Ply 1 is depth 1 PLUS quiescence, and quiescence at the root is not obviously bounded in the same trivial way the depth-1 node count is. TASK-29 (add a ply cap on quiescence check extensions) is open precisely because quiescence check extensions can currently run deep, which suggests a pathological position could make ply 1 materially slower than anything measured so far, and therefore make 'stop' materially less responsive.

### Scope of this ticket: investigate and spec, do not fix

Do not attempt a fix under this ticket, and do not assume one is needed. The likely outcomes are (a) the window is provably small enough that the current behavior is correct as-is and only needs documenting and a regression test pinning it, or (b) a bound is needed, in which case the design question is genuinely open. Candidate directions exist (cap ply-1 quiescence; make the guarantee depend on a completed root move rather than a completed iteration; honor cancellation once any legal root move has been recorded) but each interacts with the TASK-32 invariant and with TASK-29, and choosing between them is the work, not a foregone conclusion.

Concretely:

- Characterize the worst-case duration of the suppressed window empirically and by reasoning about the code path, including adversarial positions chosen to maximize ply-1 quiescence work (dense tactical positions, long capture sequences, deep check-extension chains of the kind TASK-29 describes).
- Determine whether the observed worst case is acceptable against UCI 'stop' expectations and against real tournament runner timeouts, and record the threshold used to make that call.
- Determine the interaction with TASK-29: whether a quiescence ply cap alone bounds this window sufficiently, making a separate fix unnecessary.
- Confirm whether 'quit' and process shutdown share the same suppressed window, and whether that can delay teardown.
- Produce either a short justification plus a regression test pinning the bound, or one or more fresh, well-scoped implementation tickets specifying the fix, each with its own acceptance criteria.

Related: TASK-34 covers separate self-play robustness defects (intermittent search/UCI deadlock, illegal PV moves, EOF null move) in the same stop/abort and UCI I/O area; coordinate findings so overlapping causes are not investigated or fixed twice.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 This ticket produces investigation findings, not engine fixes: no changes to engine search/stop/UCI-I/O code land under it
- [ ] #2 The worst-case duration of the abort-suppressed window is characterized with evidence, including adversarial positions selected to maximize ply-1 quiescence work, and the measurements and the reasoning about the code path are both recorded
- [ ] #3 A documented judgement is recorded on whether that worst case is acceptable against UCI 'stop' expectations and real tournament runner timeouts, naming the threshold used
- [ ] #4 The interaction with TASK-29 (quiescence check-extension ply cap) is determined and recorded, including whether that cap alone would bound this window sufficiently
- [ ] #5 Whether 'quit' and process shutdown share the suppressed window, and any resulting teardown delay, is established and recorded
- [ ] #6 The outcome is either a recorded justification for keeping current behavior plus a regression test pinning the bound, or one or more fresh well-scoped implementation tickets that spec the fix with their own acceptance criteria and preserve the TASK-32 guarantee that a legal move is always returned
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
REV-1-01 rework: supply structural ply-1 quiescence evidence.

1. Build an offline quiescence-reachability explorer as a new engine example (engine/examples/, NOT engine/src search/stop/UCI code, per AC#1). It replicates quiesce/quiesce_evasions move selection exactly — non-check q-nodes expand QueenPromotions+Captures (QMoveLoader), in-check q-nodes expand all legal evasions (quiesce_evasions) — and its only terminations are quiesce Step 1 (in_threefold, half_move_clock >= 50) plus an explicit ply cap. Omitting stand-pat/alpha-beta pruning makes it a sound UPPER BOUND on the reachable ply-1 q-tree.
2. Report per-position structural metrics: q-nodes, max q-ply, and max consecutive quiet-check-evasion chain length, measured over each depth-1 root child (the actual ply-1 quiescence work).
3. Systematically search for adversarial positions rather than asserting them: sweep the existing 10-position corpus, repo test-suite FENs, hand-constructed mutual-perpetual-check/discovered-check batteries, and randomly generated positions reached by random play. Rank by max q-ply and quiet-check-chain length.
4. Establish the structural argument for why deep quiet-check chains are hard to reach: a quiet check can only be generated from an in-check node (QMoveLoader::load_quiets is gated on in_check and quiesce returns to quiesce_evasions first), so an unbounded quiet chain requires mutual alternating check, which threefold/fifty-move then cuts.
5. Re-run tools/task39_stop_probe.rb extended with the worst positions found, recording their real bestmove latency so the structural worst case is tied to measured latency.
6. Add a fast regression test pinning the structural bound on the worst discovered position.
7. Update doc-3 with method, structural results, the adversarial search procedure and its negative/positive result, and revised conclusions; keep AC#3/#4/#5 judgements consistent with what the new evidence shows.
8. Run cargo fmt --check, strict clippy, cargo test --workspace; record Resolved REV-1-01 and hand off.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Investigation completed without engine search/stop/UCI-I/O changes.

Added reproducible release-UCI probe tools/task39_stop_probe.rb and recorded full reasoning, corpus, measurements, 100 ms diagnostic threshold, TASK-29 interaction, and quit/EOF teardown analysis in doc-3.

Evidence: 10 positions x 1,000 warmed immediate-stop samples (10,000 total) on Apple M3 Pro. Worst steady-state sample 1.069 ms; an earlier warm-transition run produced a retained 5.897 ms outlier. Fifty separate warmed-handshake go+quit processes on Kiwipete: median 0.887 ms, p95 1.247 ms, max 4.102 ms. All non-terminal cases returned legal moves.

Decision: observed behavior is acceptable, but the uncapped quiescence check-evasion tree does not provide a practically small worst-case bound. A timing-only regression would not pin the structural risk, so none was added. TASK-29 must bound the separate time-deadline overrun. Created TASK-45 to record a legal root fallback and then honor explicit cancellation during depth 1, preserving TASK-32/TASK-37's legal-bestmove guarantee.

Verification completed:
- ruby -c tools/task39_stop_probe.rb: Syntax OK
- Fresh probe smoke (100 samples x 10 positions): 1,000/1,000 legal non-null bestmoves
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo test --workspace: passed (core 35; engine 159 passed/1 ignored; metadata 5; doc tests passed)

Review attempt 1 rework.

Resolved REV-1-01: adversarial deep check-extension evidence is now supplied structurally rather than asserted.

Added engine/examples/task39_qtree.rs, an offline quiescence-reachability model. It lives outside engine/src so AC#1 still holds (git diff vs base 9c4cc18 touches no engine/src, core/src or src/ file). It replicates quiesce/quiesce_evasions move selection exactly — non-check q-nodes expand QueenPromotions+Captures per QMoveLoader, in-check q-nodes expand all legal evasions per quiesce_evasions — with quiesce Step 1 (threefold, half_move_clock >= 50) as the only non-cap termination. Stand-pat, TT cutoffs and alpha-beta are omitted deliberately, so the model is a sound upper bound on the reachable ply-1 q-tree.

It reports max_q_ply and max_quiet_check_chain (consecutive quiet check evasions — exactly the quantity a TASK-29 cap would bound), which is the q-node/max-q-ply structural evidence the finding asked for.

Systematic adversarial search rather than assertion, over four corpora: 5,000 random positions from random play (seed 1580315493), the 300-position WAC suite, the 16-position named corpus, and six purpose-built mutual-check/discovered-check/perpetual-check batteries.

Results. The longest consecutive quiet check-evasion chain found anywhere was 5, with the mass at 2-3; the six hand-built adversaries all produced chains of length 1 and trees under 500 nodes, so deliberate construction of a long chain failed. Reachable tree depth and size, by contrast, is large almost everywhere: 46 ply on WAC (201/300 positions exceeded a 2M-node cap), 55 ply in the random sweep, and past 20M nodes on ordinary dense tactical positions in the named corpus.

Latency tied to the structural worst cases. tools/task39_stop_probe.rb was extended with the six structurally worst positions found (WAC.022/263/070/093/114 by depth, WAC.104 by chain). 16,000 samples over 16 positions: every median at or below 1.162 ms, overall max 5.820 ms, all bestmoves legal and non-null. WAC.114 has a reachable ply-1 tree of 2M+ nodes at 44 ply yet answers stop in 0.269 ms median.

That gap is the substantive finding: responsiveness rests entirely on pruning effectiveness, not on the suppressed window being structurally short.

AC#4 answer changed as a result. A quiescence check-extension ply cap alone does NOT bound this window: chains never exceed 5, so a cap would almost never bind, while the large trees come from capture/promotion interleaving that resets the halfmove clock and that a check-extension cap does not touch. Recorded on TASK-29 as a comment so the finding is not lost when it is picked up. Bounding this window structurally would need a total q-node or q-ply budget.

doc-3 rewritten with method, the reachability-upper-bound caveat, all four corpora, the latency table, and revised AC#3/#4/#5/#6 judgements. No timing regression test added: it would pass on this hardware and corpus while the adversarial failure mode stayed open. TASK-45 remains the primary outcome and preserves the TASK-32/TASK-37 legal-move guarantee by construction.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 12:03
---
Coordination reply from TASK-34 (which TASK-39's description asks to coordinate with).

TASK-34's investigation covers three self-play robustness defects. Boundary against this ticket:

- Defect 1 (completion deadlock) -> TASK-35. Independent of the ply-1 suppression window: the hang occurs AFTER the search completed and its worker thread exited, so no abort is pending. It is a completion-signalling defect in engine/src/engine.rs, not a responsiveness one.
- Defect 2 (illegal PV moves) -> TASK-36. Entirely unrelated; PVTable reconstruction, independently confirmed still reproducing on master and unaffected by TASK-32.
- Defect 3 (EOF null move) -> TASK-37, now narrowed to regression coverage only. This is the one that overlaps: TASK-34 re-verified that TASK-32's suppression of the cancellation flag during ply 1 is exactly what makes the EOF path safe. So this ticket and TASK-37 examine the same window from opposite directions — TASK-37 wants the window to exist (it guarantees a legal move), TASK-39 asks whether it is too wide (it delays 'stop').

Consequence worth carrying into TASK-39's design work: any change that narrows or removes the suppressed window must preserve the EOF guarantee, because EOF and 'stop' both reach the search through the same cancellation flag. Of the candidate directions listed in this ticket's description, 'honor cancellation once any legal root move has been recorded' preserves it by construction; simply re-enabling the cancellation flag during ply 1 would reintroduce the 'bestmove 0000' forfeit that TASK-32 fixed. TASK-37's acceptance criteria were written to assert only that a legal move is returned (not a depth or timing) so they stay valid whichever direction TASK-39 chooses.

TASK-34 needed no code investigation in this ticket's area beyond confirming the above, so there is no duplicated work to reconcile.
---

author: @codex
created: 2026-07-18 19:41
---
Implementation handoff
Branch: task-39-stop-responsiveness
Worktree: /Users/seabo/seaborg-worktrees/task-39-stop-responsiveness
Base: 9c4cc1896a6f9b121412bb9c5cf86b338891367d
Implementation target: 2bea51d9d3fc2926560003673d94b0732cd2e6a4
Resolved findings: none
Verification:
- ruby -c tools/task39_stop_probe.rb: Syntax OK
- fresh probe smoke, 100 samples x 10 positions: 1,000/1,000 legal non-null bestmoves
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean
- cargo test --workspace: passed (core 35; engine 159 passed/1 ignored; metadata 5; doc tests passed)
Known failures: none
---

author: @codex
created: 2026-07-18 19:41
---
Handoff correction: the full implementation target SHA is 2bea51d6e7d5fcc850e1573fa2d2eb4a37e1e06a (the abbreviated 2bea51d in the preceding handoff was correct; its manually expanded suffix was not). All other handoff metadata is unchanged.
---

author: @codex
created: 2026-07-18 19:52
---
Review attempt: 1
Reviewed branch: task-39-stop-responsiveness
Reviewed implementation: 2bea51d6e7d5fcc850e1573fa2d2eb4a37e1e06a
Verdict: changes_requested

REV-1-01 [P1] Deep check-extension adversary is not demonstrated
Location: backlog/docs/doc-3 - TASK-39-UCI-stop-responsiveness-investigation.md (Empirical method / Results); tools/task39_stop_probe.rb:12-23
Impact: Acceptance criterion #2 explicitly requires evidence from adversarial positions selected to maximize deep ply-1 quiescence/check-extension work. The report labels one FEN as repeated-check potential and another as forcing quiesce evasions, but the probe records only elapsed time and main-search depth-one nodes. It records no q-node count, maximum q-ply, check-evasion count, or line showing that any sample actually enters a deep quiet check-evasion chain. In particular, an in-check root position does not by itself force quiesce_evasions after the depth-1 root evasion. The resulting sub-6 ms measurements therefore characterize the named corpus, but do not objectively establish coverage of the adversarial mechanism that motivated this task.
Reproduction: Run ruby tools/task39_stop_probe.rb target/release/seaborg 100 and inspect the JSON fields; only timing, depth_one_nodes, and returned moves are reported. Inspect the report's Results table; no quiescence-depth/work evidence is supplied.
Expected: Add reproducible evidence that at least one validated corpus position drives a materially deep ply-1 quiescence quiet-check-evasion chain (or systematically search for such positions), report q-node/max-q-ply or equivalent structural evidence, and include its latency in the characterization. Keep any instrumentation outside engine search/stop/UCI-I/O production code as required by criterion #1.

Verification:
- ruby -c tools/task39_stop_probe.rb: passed
- cargo build --release --bin seaborg && ruby tools/task39_stop_probe.rb target/release/seaborg 100: 1,000 samples, all non-null, max 1.198 ms
- cargo fmt --check: passed
- fresh-target cargo clippy --workspace --all-targets --all-features -- -D warnings: passed
- cargo test --workspace: passed (core 35; engine 159 passed/1 ignored; metadata 5; doc tests passed)
---
<!-- COMMENTS:END -->
