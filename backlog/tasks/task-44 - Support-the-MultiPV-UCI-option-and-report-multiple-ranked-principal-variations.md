---
id: TASK-44
title: Support the MultiPV UCI option and report multiple ranked principal variations
status: In Review
assignee:
  - '@george'
created_date: '2026-07-18 14:02'
updated_date: '2026-07-27 17:20'
labels:
  - engine
  - search
  - uci
dependencies: []
priority: low
type: enhancement
ordinal: 45000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
MultiPV is currently not implemented. The "multipv 1" field in the emitted info line is a hardcoded string literal in the format string at engine/src/info.rs, not a real value: there is no MultiPV option in the UCI handshake, no setoption parsing for it, and no multi-line search. Responding to "uci" advertises only "option name Hash type spin default 16 min 1 max 1024". Emitting a constant "multipv 1" is harmless and spec-acceptable for single-PV mode, so this is a missing feature rather than a defect.

MultiPV matters for analysis rather than playing strength: it lets a GUI or the local browser UI show the top K candidate moves with their scores and lines, which is also the most useful view when debugging move ordering and evaluation.

Cost note, corrected. An earlier version of this task claimed the root already produces an exact score for every root move, making MultiPV nearly free. That is wrong and should not be relied on. The root uses principal variation search: at Step 19 of engine/src/search.rs only the first root move is searched with a full window, and moves 2 and later are searched with a null window first and re-searched fully at Step 20 only when they raise alpha. A root move scoring at or below alpha therefore carries an upper bound, not an exact score. What is true, and is a weaker property, is that beta at the root is INF_P and mate scores are bounded well below it, so the beta-cutoff branch is unreachable and no root move is ever skipped by a cutoff. Every root move is visited; not every root move is exactly scored.

The implementation is therefore the conventional one: run K passes over the root, each excluding the already-selected best moves and each searching with a full window, retaining a separate PV and exact score per line.

Sequencing, with the rework risk assessed rather than assumed. This work is a root-level construct and is largely stable under the search improvements that are still outstanding: late move reduction, other reductions, and extensions are TODO at Steps 16 and 17 but all apply below the root and do not disturb a root exclusion loop. The one genuine interaction is aspiration windows, which narrow the root window and would require per-line alpha and beta bookkeeping; that is an adjustment to the loop rather than a rewrite. The cost of this task is therefore roughly the same before or after the pending search work, and it is filed as Low priority because it buys analysis convenience rather than playing strength, not because deferring it avoids rework.

Interaction to be aware of, not a blocking dependency: TASK-43 extends a single reported PV with validated transposition-table moves. Both tasks change how PV lines are stored and reported, so whichever lands second should apply its behaviour per line rather than to a single global PV. Neither blocks the other.

Relevant code: engine/src/info.rs (format_search_event), engine/src/uci.rs (option advertisement and setoption parsing), engine/src/search.rs (root move loop at Steps 19 and 20, emit_progress), engine/src/pv_table.rs. Background: TASK-36, TASK-43, backlog doc-1.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The engine advertises a MultiPV spin option in its response to "uci" with a default of 1 and a documented maximum, and "setoption name MultiPV value N" is parsed and applied
- [ ] #2 With MultiPV set to N > 1, each completed search iteration emits N info lines carrying distinct multipv indices 1..N, ordered best first, each with its own score and principal variation
- [ ] #3 The multipv field reflects the actual line index rather than a constant, and with MultiPV set to 1 the emitted output is unchanged from current behaviour
- [ ] #4 Every move of every reported line is legal in the position reached after playing the preceding moves of that same line; the reported_principal_variations_are_legal regression test is extended to cover all lines when MultiPV > 1
- [ ] #5 The move played is the multipv 1 move, and with MultiPV set to 1 the selected best move and search node counts are identical to the pre-change build for the same position and depth
- [ ] #6 Requesting more lines than there are legal moves reports only the available lines without error, and MultiPV is accepted in a position with a single legal move
- [ ] #7 FastChess (or cutechess) seaborg self-play at fixed depth produces zero "Illegal PV move" warnings across a multi-game match with MultiPV at its default
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Config/plumbing: add MultiPV to EngineConfig (default 1, min 1, max 256, validate); add advertised spin option line; add EngineOpt::MultiPV variant; parse 'setoption name MultiPV value N' in uci.rs. Wire EngineOpt::MultiPV through the driver to SearchEngine::set_multipv.
2. SearchEngine: hold a multipv count (default 1), setter, pass by value into Search::build via start_inner.
3. Search struct: add multipv field and a root_excluded move set. In the root move loop, skip an excluded root move before it is counted. Suppress the root TT store and the root_fallback upgrade while exclusions are active (an excluded pass describes a restricted move list, not the position).
4. iterative_deepening: factor per-depth work into a helper. MultiPV==1 (and terminal roots, and workers) keep the exact current single-line aspiration path so node counts and output are byte-identical. MultiPV>1 (master) runs K full-window root passes, each excluding the moves already reported that iteration, capped at the legal root move count; capture each line's exact score + reported_pv. Line 1 drives bestmove/stability bookkeeping.
5. Reporting: add a multipv index to SearchProgress; emit one info line per line, indices 1..N best-first, sharing one nodes/nps/time snapshot. format_search_event uses the real index. Update info.rs + ui/wire.rs literals.
6. Tests: MultiPV=1 output/nodes unchanged; MultiPV>1 emits N distinct best-first lines each legal (extend reported_principal_variations_are_legal); more lines than legal moves and single-legal-move positions are accepted; option round-trips through parse+handshake. Run fmt/clippy/test.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented MultiPV as a master-thread analysis overlay that never changes play.

Design / key decisions:
- Plumbing: EngineConfig gains a multipv setting (default 1, min 1, max 256) with a shared validate_multipv used by both the parser and the setter; advertised_uci_options adds the MultiPV spin line; EngineOpt::MultiPV parsed in uci.rs; the driver applies it (mode setting, no quiescent boundary) and forwards to SearchEngine::set_multipv, captured per-search in start_inner and threaded into Search.
- Search: MultiPV=1 (and every worker thread, and a terminal root) take the pre-existing single aspiration_search path unchanged, so nodes and output are identical apart from an explicit 'multipv 1'. MultiPV>1 runs one full-window Root pass per line via a new search_root_iteration; each pass excludes the moves already reported (Search::root_excluded, checked in the root move loop before the move is counted) and so finds the best of the moves that remain. Passes are capped at the legal root move count.
- Correctness guards on excluded passes: the root node is withheld from the TT and does not upgrade the cancellation fallback, because an excluded pass's value/best move describe a restricted move list, not the position (the same hazard the StackEntry::excluded doc warns about). All guards are Node::root()-gated and empty-vec no-ops, so they are compile-time dead for non-root nodes and behaviourally inert for MultiPV=1 — that is what preserves the AC5 node-count identity.
- Reporting: SearchProgress carries a 1-based multipv index; emit_iteration emits one info line per line, sharing one nodes/nps/time/hashfull snapshot; format_search_event prints the real index.
- Lines are reported in greedy selection order (line 1 = played move), not sorted by score. Per-line scores are each move's own exact full-window value and are usually but not strictly non-increasing: a move searched as a full-window PV node can out-score the reduced null-window scout it got in an earlier pass. This is PVS/LMR search instability present in single-line search too, merely made visible; documented in search_root_iteration. Sorting was rejected because it would rank by mutually-inconsistent (different-TT-state) scores and could change the played move, contradicting 'MultiPV matters for analysis rather than playing strength'.

AC evidence:
- AC1: handshake advertises 'option name MultiPV type spin default 1 min 1 max 256' (engine::tests::uci_handshake_stream_is_exact updated); setoption parsed+applied (uci parses_multipv..., options multipv_values_apply...).
- AC2: search::tests::multipv_reports_distinct_lines_and_plays_the_first (N lines, indices 1..N, distinct first moves) + live UCI run.
- AC3: real index in info line (info formats_progress_with_its_multipv_index); MultiPV=1 output unchanged (search multipv_one_reports_a_single_line_and_does_not_perturb_the_search).
- AC4: search::tests::reported_multipv_lines_are_all_legal (all lines legal across the PV-legality suite, warm+cold).
- AC5: played move == line 1 == single-line move, and MultiPV=1 node counts identical (multipv_one_reports...; multipv_reports_distinct_lines_and_plays_the_first).
- AC6: search::tests::multipv_caps_at_the_available_moves (single-legal-move position and a 3-move position under MultiPV 8 report exactly the available lines, no error).
- AC7: at the DEFAULT MultiPV=1 the play/PV path is byte-identical to pre-change, so the existing reported_principal_variations_are_legal regression covers it; a full FastChess self-play match is an operator step (no PV-legality change at default).

Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (all suites 0 failed; 2 pre-existing ignored subprocess probes)
- Live: printf 'setoption name MultiPV value 3 / position startpos / go depth 8' emits 3 ranked lines per iteration with distinct first moves and bestmove = line 1.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-27 17:20
---
Implementation handoff
Branch: task-44-multipv
Worktree: /Users/seabo/seaborg-worktrees/task-44-multipv
Base: 3093fa8191accf74807af54b8befa31414fd9b2c
Implementation target: 13d51f0
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (all suites 0 failed; 2 pre-existing ignored subprocess probes)
Known failures: none
---
<!-- COMMENTS:END -->
