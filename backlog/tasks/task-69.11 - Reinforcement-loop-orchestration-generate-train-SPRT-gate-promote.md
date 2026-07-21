---
id: TASK-69.11
title: 'Reinforcement loop orchestration: generate, train, SPRT-gate, promote'
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-20 19:42'
updated_date: '2026-07-21 17:19'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the reinforcement-loop mechanism (69.12 runs the programme for real).

Scope decision (confirmed with user): use a UCI `EvalFile` string option to load networks, with a small per-side extension to strength_test.py, rather than env-var wrapper scripts. Validation is smoke + tests; no multi-hour authoritative run here.

What changed:
- Engine: SearchEngine now holds Option<Arc<Network>> and threads it into each per-move Search (Arc avoids a per-move deep copy). New UCI option 'EvalFile' (string, default <empty>) loads/validates an SBNN at a quiescent boundary and clears the hash (cached static evals are evaluator-dependent); <empty> restores hand-crafted; a load failure changes nothing and is reported. 'datagen --network <path>' evaluates self-play with a network; absent = generation-0 hand-crafted bootstrap.
- Strength harness: added per-side --baseline-option/--candidate-option (placed in each engine's own -engine block, recorded in report.json), so one binary plays as two engines differentiated only by EvalFile. validate() does not require distinct binaries.
- Orchestration: tools/rl/loop.py + test_loop.py + README + .gitignore. Runs one iteration (generate/train/export/gate) and iterates; promotes only on SPRT PASS (exit 0); writes an append-only ledger with data volume, node budget, candidate/baseline network ids, verdict, and measured Elo delta. External steps behind a Backend seam; loop logic tested with a fake backend. Purity preserved by construction (evaluator is only hand-crafted or a loop-promoted network).

Design note surfaced during implementation: swapping the evaluator invalidates the TT's cached static evals, so EvalFile changes clear the hash like ucinewgame. Pinned by a driver test.

Constraint: EvalFile paths must be whitespace-free (the UCI parser takes the value as a single token); the loop resolves to absolute paths and the README documents it.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 17:19
---
Implementation handoff
Branch: task-69.11-reinforcement-loop-orchestration
Worktree: /Users/seabo/seaborg-worktrees/task-69.11-reinforcement-loop-orchestration
Base: daa79cb8a19d635702e894927f44064e76480f95
Implementation target: 7e0ae8d677952170e3a28a8b8a04387494063ce7
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (engine 397, chess 50, integration + others green; new EvalFile parse/driver, SearchEngine + self-play network threading tests included)
- python3 -m unittest (tools/strength): pass (20 + new per-side option routing test)
- python3 -m unittest (tools/rl): pass (9 loop tests: bootstrap, promote-only-on-PASS, non-PASS no-promote, prior-best survival, attribution fields, purity, resume, broken-step abort)
- Real-binary smoke (release): 'datagen' hand-crafted vs '--network golden_v1.sbnn' produce different self-play; bad --network path reported; UCI advertises 'option name EvalFile type string default <empty>' and a search with EvalFile set returns a bestmove
Known failures: none
---
<!-- COMMENTS:END -->
