---
id: TASK-69.4
title: Scalar quantized NNUE inference wired as a selectable evaluation
status: Done
assignee:
  - '@claude'
created_date: '2026-07-20 19:40'
updated_date: '2026-07-21 02:45'
labels:
  - nnue
  - inference
dependencies:
  - TASK-69.2
  - TASK-69.3
parent_task_id: TASK-69
priority: high
ordinal: 106000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the portable scalar reference forward pass: from the accumulator (TASK-69.3) through the clipped activation and remaining layers to a single centipawn score, using the exact quantized integer arithmetic from the design contract. Wire it behind Search::evaluate as a selectable evaluation path so the hand-crafted evaluation remains the default until a trained network exists and passes its gate.

This scalar path is the permanent correctness oracle: it is what runs on targets without the SIMD path, and it is the reference the SIMD path (TASK-69.5) and the PyTorch quantized forward (TASK-69.10) are both checked against. Establish the golden-vector test harness here — load (FEN, expected-score) pairs produced alongside a network and assert exact equality — even if seeded initially with a tiny hand-constructed network.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 With a loaded network, Search evaluates positions through the scalar quantized forward pass and the evaluation is selectable without disturbing the default hand-crafted path
- [x] #2 A golden-vector test loads (FEN, expected-score) pairs and asserts exact integer equality against the scalar forward pass
- [x] #3 The quantized arithmetic (scales, clipping, saturation) matches the design contract and is exercised by tests including near-overflow accumulator states
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add engine/src/nnue/inference.rs: a pure scalar forward pass forward(network, &Accumulator, side_to_move) -> i32 centipawns implementing the contract's normative arithmetic exactly (concat stm-first, clipped ReLU to [0,QA], i32 output accumulate, i64 multiply-by-SCALE, round-half-away-from-zero divide by QA*QB, clamp to [-10000,10000]). Re-export from nnue/mod.rs.
2. Selection seam: give Search an optional owned Network (default None => hand-crafted, unchanged). When Some, Search::evaluate rebuilds an Accumulator::from_position for the current leaf and runs the forward pass, returning Score::cp already from the side-to-move perspective (no *pov(), forward is stm-relative). Default path byte-for-byte unchanged. Add a constructor/setter to configure it; thread through SearchEngine minimally without disturbing existing call sites.
3. Golden-vector harness: a test loading (FEN, expected-cp) pairs and asserting exact integer equality against the scalar forward pass, seeded with a small hand-constructed network committed as fixture data (FENs + expected scores). Establish the harness structure .10 will reuse.
4. Arithmetic tests: unit tests over the forward pass covering rounding (half-away-from-zero both signs), clipping/saturation at [0,QA], and near-i16-overflow accumulator states; verify the i32/i64 widening prevents overflow before the divide.
5. Scope: from-scratch accumulator at the evaluated leaf (per approved decision); incremental threading + per-node assertion deferred (needs Accumulator API/lifetime change, companion to .5). Document the deferral in notes.
6. Run cargo fmt --check, clippy -D warnings, cargo test --workspace; hand off for review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the scalar quantized NNUE forward pass and wired it behind Search::evaluate as a selectable evaluation, keeping the hand-crafted tapered evaluation the default.

- engine/src/nnue/inference.rs (new): forward(network, &Accumulator, side_to_move) -> i32 centipawns. Implements the contract's normative arithmetic exactly: concat the two accumulators side-to-move first, clip each entry to [0, QA], accumulate the output dot product in i32 seeded by b_out, widen to i64 for the multiply by SCALE, round-half-away-from-zero divide by QA*QB, then clamp to the centipawn band [-10000, 10000]. The result is already from the side to move's perspective, so it takes no pov() flip. round_div and the i32/i64 widths are private and factored so the SIMD path (.5) and the PyTorch forward (.10) check against this oracle. Re-exported as nnue::forward.
- engine/src/search.rs: Search gains network: Option<Network> (None by default) and set_network(). evaluate() branches on it at the single leaf-scoring point the contract designates: with a network it rebuilds Accumulator::from_position for the leaf and runs forward(); otherwise the hand-crafted path is byte-for-byte unchanged. make/make_null/unmake and the eval_stack are untouched.

Scope decision (confirmed with the user before implementation): the accumulator is rebuilt from scratch at each evaluated leaf. Incremental accumulator threading through make/unmake (with an at-every-node debug assertion mirroring EvalState) is deferred: doing it cleanly needs either a network lifetime on Search (ripples through ~40 call sites) or a change to TASK-69.3's approved Accumulator<'net> API, and it is a performance concern with no benefit while no trained network runs. This scalar path is the correctness oracle; the incremental+SIMD work is TASK-69.5. AC#1 ('Search evaluates positions through the scalar quantized forward pass ... selectable without disturbing the default hand-crafted path') is met without it.

Tests:
- Golden-vector harness (AC#2): golden_vectors_match_the_scalar_forward_pass_exactly loads (FEN, expected-cp) pairs for a hand-seeded H=16 network and asserts exact integer equality against both forward() and an independent dense reference forward pass (materializes the full 768-input vector and multiplies densely, sharing no accumulation structure with forward). The king-only golden value (-19) was additionally hand-computed end-to-end (per-unit accumulator, clip, both output blocks, rounded dequant) to anchor the constants. forward_agrees_with_the_dense_reference_over_many_positions cross-checks the two paths over more positions and two hidden widths. The harness structure is what TASK-69.10 reuses with trainer-emitted vectors.
- Arithmetic (AC#3): round_div_rounds_half_away_from_zero (both signs, odd/even denominators); activations_saturate_at_the_clip_bounds (i16::MAX -> QA, i16::MIN -> 0); output_accumulation_does_not_overflow_near_the_i32_ceiling (H=256, s just under i32::MAX so s*SCALE exceeds i32, proving the i64 widen); evaluation_is_clamped_into_the_centipawn_band; mirrored_positions_score_identically_from_the_mover (perspective symmetry).
- Selectability (AC#1): search::tests::evaluate_selects_the_nnue_forward_pass_when_a_network_is_set confirms evaluate() returns the independently-computed forward score when a network is set, differs from the hand-crafted score, and restores the hand-crafted value exactly when cleared.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 02:02
---
Claiming for implementation on task-69.4-scalar-nnue-inference (worktree /Users/seabo/seaborg-worktrees/task-69.4-scalar-nnue-inference, base 0f73ec8). Integration depth confirmed with the user: from-scratch accumulator at the evaluated leaf; incremental threading deferred.
---

author: @claude
created: 2026-07-21 02:17
---
Implementation handoff
Branch: task-69.4-scalar-nnue-inference
Worktree: /Users/seabo/seaborg-worktrees/task-69.4-scalar-nnue-inference
Base: 0f73ec88f5e22bb0db44839e4599077f5d4b1593
Implementation target: 1278acfab0c5df4540c3a6f890f62eadfab59ef5
Resolved findings: none (new work)
Verification (on 1278acf):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (0 warnings/errors)
- cargo test --workspace: pass (chess 49, engine 352 / 2 ignored incl. 8 new nnue::inference tests + 1 new search selectability test, lichess 93, build_metadata 19, doctest 1; 0 failed)
Known failures: none
Scope note: from-scratch accumulator at the evaluated leaf; incremental accumulator threading through search deferred to the SIMD/perf track (TASK-69.5). Confirmed with the user before implementation. Not a movegen hot path; the NNUE branch runs only when a network is selected, which no default search does, so hot-path benchmarks are not applicable.
---

author: @claude
created: 2026-07-21 02:30
---
Review attempt: 1
Reviewed branch: task-69.4-scalar-nnue-inference
Reviewed implementation: 1278acfab0c5df4540c3a6f890f62eadfab59ef5
Verdict: approved

All three acceptance criteria are proven by objective evidence; no blocking findings.

Full diff (0f73ec8..1278acf) touches only engine/src/nnue/inference.rs (new), engine/src/nnue/mod.rs (submodule + re-export + doc), and engine/src/search.rs (network field, set_network, evaluate branch, tests). No unrelated changes. The default (network=None) evaluation path is byte-for-byte unchanged apart from a single always-not-taken Option check at the top of evaluate().

AC#1 (selectable NNUE eval without disturbing the default): evaluate() branches on network at the single leaf-scoring point; set_network toggles it. Proven by search::tests::evaluate_selects_the_nnue_forward_pass_when_a_network_is_set (NNUE score matches an independent forward(), differs from the hand-crafted score, and clearing restores the hand-crafted value exactly).

AC#2 (golden-vector exact-equality harness): nnue::inference::tests::golden_vectors_match_the_scalar_forward_pass_exactly loads (FEN, expected-cp) pairs and asserts exact integer equality against forward() and an independent dense reference. I independently reproduced the king-only anchor by hand from the contract arithmetic (s=-762 -> round_div(-304800, 16320) = -19), confirming the harness is not self-fulfilling.

AC#3 (contract arithmetic incl. near-overflow): arithmetic matches docs/nnue-design-contract.md exactly (stm-first concat, clip [0,QA], i32 accumulate, i64 multiply-by-SCALE, round-half-away-from-zero divide, clamp [-10000,10000]). Exercised by round_div_rounds_half_away_from_zero, activations_saturate_at_the_clip_bounds, output_accumulation_does_not_overflow_near_the_i32_ceiling (H=256, s just under i32::MAX so s*SCALE overflows i32, forcing the i64 widen), and evaluation_is_clamped_into_the_centipawn_band.

Scope: from-scratch accumulator rebuilt at each evaluated leaf; incremental accumulator threading deferred to TASK-69.5. This satisfies AC#1 (which does not require incremental update) and the deferral was confirmed with the user per the notes.

Benchmarks: perft/movegen benchmarks were not run and are not applicable. Neither exercises evaluate(); the only default-path change is a single predictable Option check, and the NNUE branch runs only when a network is selected (no default search does). No search/eval benchmark exists in the prescribed set to measure it.

Verification (on 1278acf):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass on a clean CARGO_TARGET_DIR (exit 0, 0 warnings)
- cargo test --workspace: pass (engine 352 / 2 ignored, chess 49, lichess 93, build_metadata 19, doctest 1; 0 failed)
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Scalar quantized NNUE forward pass (engine/src/nnue/inference.rs) wired behind Search::evaluate as a selectable evaluation; the hand-crafted tapered evaluation stays the default (network: Option<Network> = None). The forward pass implements the design contract's normative arithmetic exactly (stm-first concat, clipped ReLU to [0,QA], i32 output accumulate seeded by b_out, i64 multiply by SCALE, round-half-away-from-zero divide by QA*QB, clamp to [-10000,10000]); stm-relative so no pov flip. Verified on implementation target 1278acf: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -- -D warnings pass on a clean CARGO_TARGET_DIR (exit 0, 0 warnings); cargo test --workspace pass (engine 352/2 ignored incl. 8 nnue::inference tests + 1 search selectability test, chess 49, lichess 93, build_metadata 19, doctest 1). AC#1 proven by evaluate_selects_the_nnue_forward_pass_when_a_network_is_set; AC#2 by golden_vectors_match_the_scalar_forward_pass_exactly (the king-only anchor -19 was additionally reproduced by hand from the contract arithmetic, confirming the harness is not circular); AC#3 by round_div/saturation/near-i32-ceiling/clamp tests plus an independent dense reference forward and mirror-symmetry check.
<!-- SECTION:FINAL_SUMMARY:END -->
