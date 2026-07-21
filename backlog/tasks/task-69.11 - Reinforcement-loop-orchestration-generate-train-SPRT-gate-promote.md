---
id: TASK-69.11
title: 'Reinforcement loop orchestration: generate, train, SPRT-gate, promote'
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-20 19:42'
updated_date: '2026-07-21 16:53'
labels:
  - nnue
  - training
  - rl
dependencies:
  - TASK-69.4
  - TASK-69.6
  - TASK-69.9
  - TASK-69.10
parent_task_id: TASK-69
priority: high
ordinal: 113000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Automate one turn of the reinforcement loop and the iteration over turns: generate self-play data with the current best network as the evaluator (iteration 0 bootstraps from the hand-crafted evaluation), train the next candidate on it, gate the candidate against the current best with the repository strength-test SPRT harness, and promote it only if it passes. Record attribution for every iteration (data volume, node budget, network id, measured delta) so strength changes stay attributable in the way BENCHMARKS.md and the strength harness require.

This orchestration composes the datagen (TASK-69.6), training/export (TASK-69.9), inference (TASK-69.4), and equivalence (TASK-69.10) pieces; it adds no new numeric machinery, only the loop, the gate, and the bookkeeping.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A single command runs one full iteration: generate, train, export, load into the engine, and SPRT-gate the candidate against the current best
- [ ] #2 A candidate is promoted to current-best only when it passes the strength gate, and the decision plus attribution are recorded
- [ ] #3 Iteration 0 bootstraps from the hand-crafted evaluation, and the self-play purity constraint is preserved end to end
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Engine network loading (UCI EvalFile):
   - search.rs: SearchEngine holds Option<Arc<Network>>; add set_network/clear_network; thread the Arc into each per-move Search (Search.network becomes Option<Arc<Network>> to avoid a per-move deep copy). Update the existing evaluate test.
   - options.rs: add EngineOpt::EvalFile(Option<PathBuf>); advertise 'option name EvalFile type string default <empty>'; empty value clears the network (hand-crafted default preserved).
   - uci.rs: parse 'setoption name EvalFile value <path>' (string-valued option; capture the path). Unit-test the parse.
   - engine.rs: on EvalFile setoption at the quiescent boundary, Network::read the file and set it on the SearchEngine; empty clears; a load error is reported via 'info string' without disturbing the current network.
2. Datagen network selection:
   - datagen.rs: add --network <path>; load a Network and thread it (Arc) through SelfPlayConfig into each worker's SearchEngine. Absent = hand-crafted (iteration 0 bootstrap). Preserves the self-play purity boundary end to end.
3. strength_test.py: add repeatable --baseline-option/--candidate-option; emit per-side options in the -engine blocks (not -each); record them in report.json; validate them. Same seaborg binary, different EvalFile per side.
4. Orchestration (tools/rl/loop.py, Python + unittest, mirroring the trainer/strength tool conventions):
   - One command runs one iteration: locate current best net (state dir); datagen (--network best, or none for gen 0); train (--generation/--lambda schedule) + export candidate.sbnn; run the SPRT gate (baseline EvalFile=best-or-none, candidate EvalFile=candidate); parse the verdict; promote candidate to best only on PASS (exit 0); append attribution (data volume, node budget, network id, measured delta, verdict) to a ledger (JSONL + human-readable). Iterate over N turns.
   - Mockable subprocess boundaries + a smoke path so the loop's logic is testable without torch/fastchess/hours.
   - Define the state/networks/iterations directory convention (gitignored artifacts) and a committed attribution ledger.
5. Tests: Rust — EvalFile parse, engine load/clear, datagen --network threading, evaluate-through-network. Python — test_loop.py: gen-0 bootstrap uses no network, promote only on PASS, FAIL/INCONCLUSIVE do not promote, attribution fields recorded, purity (loop feeds only its own best net). Docs: tools/rl/README.md + a short note in docs/.
Validation: cargo fmt/clippy/test + Python unittest + a strength smoke run; the real authoritative programme run is TASK-69.12.
<!-- SECTION:PLAN:END -->
