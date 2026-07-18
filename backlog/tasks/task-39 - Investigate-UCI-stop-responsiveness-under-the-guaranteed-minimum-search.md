---
id: TASK-39
title: Investigate UCI stop responsiveness under the guaranteed-minimum search
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-18 11:46'
updated_date: '2026-07-18 19:41'
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
1. Trace the guaranteed first iteration, quiescence recursion, cancellation, and UCI driver shutdown paths; identify what can and cannot bound the suppressed interval.
2. Build a release UCI binary and measure immediate-stop latency in a persistent-process harness over representative and adversarial FENs, including dense tactics, long capture sequences, and check-extension chains; repeat enough samples to report distributions and a conservative threshold.
3. Compare the evidence with UCI prompt-stop semantics and common tournament-runner timeout margins, and determine whether TASK-29's proposed quiescence cap alone supplies a sufficient bound.
4. Record the investigation in Backlog documentation without changing engine search/stop/UCI code. If the evidence supports keeping behavior, add only regression coverage that pins a robust bound; otherwise create well-scoped implementation ticket(s) preserving TASK-32's legal-move guarantee.
5. Run focused verification plus the repository-required formatting, strict Clippy, and workspace tests; commit the immutable investigation target and create the In Review handoff.
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
<!-- COMMENTS:END -->
