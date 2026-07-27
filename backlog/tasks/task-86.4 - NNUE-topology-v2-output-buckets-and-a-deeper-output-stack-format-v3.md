---
id: TASK-86.4
title: 'NNUE topology v2: output buckets and a deeper output stack (format v3)'
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-25 12:23'
updated_date: '2026-07-27 10:38'
labels:
  - nnue
dependencies:
  - TASK-86.1
parent_task_id: TASK-86
priority: high
ordinal: 146000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend the network beyond a single hidden layer to a modern topology while keeping the existing 768 feature set (no feature-set change): a wider feature transformer feeding a deeper output stack (e.g. 2H -> 16 -> 32 -> 1) with piece-count-selected output buckets (e.g. 8 buckets), where only the selected bucket's tail executes per evaluation. The feature transformer dominates per-node cost and is maintained incrementally, so the small dense tail and extra buckets add eval capacity at near-zero runtime cost. Requires a versioned file-format bump for the extra layers and buckets, int8 quantization for the dense tail, matching PyTorch training/export, and Rust scalar+SIMD inference with bucket selection. This is a topology-only change that de-risks the training/quant/inference path for a bigger net before the king-bucketed feature set is attempted.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The file format is versioned to carry a multi-layer output stack and an output-bucket count, with deterministic rejection of files whose architecture fields are unknown or mismatched
- [ ] #2 Inference selects exactly one output bucket by a documented static rule (e.g. piece count) and evaluates only that bucket's tail; scalar and AVX2 paths are bit-identical
- [ ] #3 The PyTorch model, quantization-aware training, and export produce a valid bucketed multi-layer .sbnn (int8 dense tail) that the engine loads and evaluates
- [ ] #4 A golden-vector / three-way differential equivalence test covers a bucketed multi-layer network across positions spanning multiple buckets
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Topology v2: bucketed multi-layer output stack, versioned format, int8 dense tail, matching PyTorch/export, scalar+AVX2 bit-identical inference, cross-language golden test.

ARCHITECTURE (proposed)
- Feature transformer 768->H per perspective (unchanged; int16 @ QA), concat side-to-move-first -> 2H, activation (CReLU/SCReLU) into [0,QA]. This is maintained incrementally (TASK-86.1) and dominates cost.
- New per-bucket output STACK on top of the 2H activations: e.g. 2H->L1->L2->1 (default L1=16, L2=32), int8 weights, int32 bias per layer. NUM_BUCKETS copies of the stack; exactly one bucket's stack runs per eval.
- Bucket rule (static, documented): bucket = min((piece_count-1)/((32+B-1)/B), B-1) i.e. piece-count binned into B buckets. Piece count from pos.occupied().count() at the evaluate seam.

INT8 TAIL INTEGER CONTRACT (to be ratified before coding)
- Each hidden tail layer: acc[o]=bias[o]+sum_i in[i]*W[o][i] (i32); W int8 @ QC, bias @ QA*QC; next input = activation(round_div(acc,QC)) clamped [0,QA].
- Final layer -> eval_cp = round_div(acc*SCALE, QA*QC), clamp centipawn band.
- Bit-identity scalar==AVX2 with vpmaddubsw requires bounding pairwise i16 sum: clamp tail activations to [0,127] and weights [-127,127] (Stockfish-like) OR keep [0,QA] activations and bound weights. DECISION NEEDED.

FORMAT (v2/v3 bump; keep loading v1)
- Extend header/blob to carry: num output-stack layers, their dims, num buckets, int8 tail scale(s). v1 nets (built-in gen-002) still load and run the single-linear path. Deterministic rejection of unknown/mismatched arch fields (new LoadError variants). FNV-1a hash + param_bytes recomputed for the new blob.

INFERENCE (engine/src/nnue)
- format.rs: v2 Network variant (or unified struct with optional stack+buckets); loader/writer; validation.
- inference.rs: bucket selection + deep-stack forward, scalar reference + AVX2 (vpmaddubsw/vpmaddwd), bit-identical; reuse round_div, EVAL band. forward() gains piece_count arg; update 2 call sites in search.rs.

PYTORCH (tools/trainer)
- model.py: NnueConfig gains stack dims + num_buckets; NnueModel builds per-bucket tail, QAT fake-quant on int8 grid (QC) + activation clamp; bucket selection in forward from piece count.
- export.py: quantize per-bucket int8 tail, checked casts, v2 QuantizedNetwork to_bytes/from_bytes, integer_eval_cp for the deep stack, new golden network + vectors spanning multiple buckets.
- train.py/data.py: thread piece-count/bucket through the batch; keep v1 training working.

DOC: update docs/nnue-design-contract.md (or add a v2 section) with the new topology, bucket rule, and int8 tail arithmetic BEFORE implementation (contract is normative for both languages).

TESTS: format round-trip + rejection for v2; scalar==AVX2 bit-identity for bucketed deep stack; three-way (python/scalar/simd) golden across positions spanning >=2 buckets; v1 nets still load/eval identically.

VERIFY: cargo fmt --check; clippy -D warnings; cargo test --workspace; python trainer tests.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Contract-first checkpoint: wrote docs/nnue-topology-v2.md (normative spec for format v2 topology). Key decisions: reuse QB as the int8 tail weight scale (scale-uniform stack: input @ QA, weights int8 @ QB, bias i32 @ QA*QB, acc = QA*QB*out_float); inter-layer requantize round_div(acc,QB) clamp [0,QA] then activation; final layer dequantize as v1. Activations stay [0,QA=255], tail weights full int8 [-127,127]; AVX2 widens int8->i16 and uses non-saturating vpmaddwd (not vpmaddubsw) so scalar==SIMD bit-identical. Buckets/dims parameterizable in header (num_buckets @off40, num_output_layers @off42, stack_dims table in blob prefix); defaults 8 buckets, 2H->16->32->1. Bucket rule: min((p-1)*B/32, B-1) from pos.occupied().popcnt(). v1 nets (gen-002) still load/eval unchanged. Awaiting approval before implementing.
<!-- SECTION:NOTES:END -->
