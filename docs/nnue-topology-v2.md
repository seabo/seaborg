# NNUE topology v2 contract (format version 2)

This document extends `docs/nnue-design-contract.md` with the second network
topology: a **bucketed multi-layer output stack** on top of the unchanged
perspective-768 feature transformer. It is a decision record, not code, and it
is normative for both the Rust inference path (`engine/src/nnue`) and the Python
trainer/exporter (`tools/trainer`). Where this document and the v1 contract
disagree, this one governs **format version 2**; the v1 contract continues to
govern **format version 1**, which format version 2 does not replace.

Status: accepted, July 2026. Consumed by TASK-86.4 (this topology) and TASK-86.5
(the sweep that selects the first v2 network). If a decision here changes after
implementation, change it here first and bump `FORMAT_VERSION` rather than
letting the Rust and Python paths drift.

Decisions recorded in this revision: (1) one int8 weight scale **per
output-stack layer**, shared across all buckets (`stack_scales`, indexed by layer
`k` only); (2) the scales are **fixed configuration written into the file** —
specified in the trainer config, fake-quantized to during QAT, and written
verbatim by export, exactly as the single `QB` is today (not learned, not
export-derived); (3) the header `qb` field stays authoritative for the **final**
layer (`qb == QB_last`), so the dequantize keeps its v1 `QA·qb` form and there is
one source of truth. Per-bucket-per-layer scales are a future extension.

The v1 self-play purity boundary, the feature set and its index formula, the
feature-transformer quantization (int16 weights/bias at `QA`, i16 accumulator,
activation clamp to `[0, QA]`), the training-target formulation, and the rounded
dequantize all carry over **unchanged**. Only what sits *after* the two
per-perspective accumulators changes.

## Why this topology

The v1 network is `768 → H` per perspective, concatenated to `2H`, activated,
then a single linear `2H → 1`. The feature transformer dominates per-node cost
and is maintained incrementally (TASK-86.1), so the cheapest place to add
evaluation capacity is *after* the accumulator, where the work is a small dense
matmul independent of piece count. Two extensions do that:

- **A deeper output stack.** Replace the single `2H → 1` map with a small
  multi-layer stack, e.g. `2H → 16 → 32 → 1`, with a nonlinearity between layers.
- **Output buckets.** Keep several independent copies of that stack — one per
  *bucket* — and select exactly one bucket per evaluation by a static rule
  (piece count). Only the selected bucket's stack executes, so `B` buckets add
  eval capacity (a stack specialised per game phase) at the runtime cost of one
  stack.

Both are pure eval-capacity levers whose per-node cost is dominated by the
already-incremental feature transformer, which is the point: they de-risk the
training/quant/inference path for a bigger network before the costlier
king-bucketed feature set (TASK-86.6) is attempted.

## Topology (fixed structure, parameterizable dimensions)

1. **Feature transformer** — unchanged from v1: `768 → H` per perspective, int16
   weights/bias at scale `QA`, i16 accumulator, maintained incrementally.
2. **Perspective concatenation** — unchanged: `x = concat(acc[stm], acc[~stm])`,
   a `2H`-vector, side-to-move first.
3. **Activation** — unchanged: the header's `activation_id` (CReLU or SCReLU)
   applied elementwise, producing `a[j] ∈ [0, QA]`. This is the input to the
   output stack.
4. **Output stack (new).** `NUM_BUCKETS` independent stacks. The static bucket
   rule (below) selects one. The selected stack is a sequence of
   `NUM_OUTPUT_LAYERS` affine layers with the header activation applied between
   layers (not after the last):

   ```text
   in_0            = a                       // 2H, the activated accumulator
   acc_k[o]        = b_k[o] + Σ_i in_k[i] · W_k[o][i]     // affine layer k
   in_{k+1}[o]     = activation(round_div(acc_k[o], QB_k)) // requantize with layer k's scale, k < last
   ```

   The final layer produces a single scalar `acc_last`, dequantized to centipawns
   exactly as v1's output layer is (below). Layer `k` maps `in_dim_k → out_dim_k`
   with `in_dim_0 = 2H` and `in_dim_k = out_dim_{k-1}`; `out_dim_last = 1`.

**Parameterizable dimensions** (carried in the file, validated at load):

- **Hidden width `H`** — the feature-transformer output width per perspective, a
  positive multiple of 16, exactly as v1.
- **`NUM_BUCKETS`** — number of independent output stacks. Default `8`. Must be
  a positive integer in `1..=32`.
- **The output-stack layer dimensions** `[out_dim_0, …, out_dim_last]`. Default
  `[16, 32, 1]` (i.e. `2H → 16 → 32 → 1`). `out_dim_last` must equal
  `output_dim = 1`; every earlier (hidden) `out_dim_k` must be a positive
  multiple of 16, so that every layer's input dimension is a multiple of 16 and
  the SIMD kernel needs no scalar remainder.
- **Per-layer int8 weight scales** `[QB_0, …, QB_last]` (`stack_scales`). Default
  all `64`. Each `> 0`; `QB_last` equals the header `qb`. Shared across buckets;
  see *Quantization of the output stack*.
- **Activation id** — CReLU (`0`) or SCReLU (`1`), as v1. The same activation is
  used at the feature-transformer output and between every pair of stack layers.

## Output bucket selection (static rule)

**Decision: select the bucket by total piece count, binned into `NUM_BUCKETS`
equal ranges over the reachable `1..=32`.** With `p` the number of pieces on the
board (`pos.occupied().popcnt()`, always `2..=32` for a legal position) and
`B = NUM_BUCKETS`:

```text
bucket = min((p - 1) · B / 32, B - 1)      // integer division
```

For `B = 8` this is `(p - 1) / 4`, i.e. buckets `{2..5, 6..9, …, 30..32}`, the
same phase binning strong engines use. The `min` is a defensive clamp: for any
`p ≤ 32` and `B ≤ 32` the quotient is already `≤ B - 1`, so it only matters if a
caller passes an out-of-range count.

The rule is **static** — a pure function of piece count, independent of the
network's own output — so it introduces no feedback and is trivially identical
across the scalar path, the AVX2 path, and the Python reference. The bucket is
chosen once per evaluation, before any stack arithmetic.

## Quantization of the output stack (int8 dense tail)

The output stack is quantized to **int8 weights** with **int32 biases**. This is
the normative integer arithmetic; the scalar reference, the AVX2 kernel, and the
Python integer forward pass must all reproduce it bit for bit.

**Scales.** Each output-stack layer `k` has its own int8 weight scale `QB_k`, a
positive integer carried in the file (`stack_scales`, below), shared across all
buckets. Defaults are all `64`; a layer with a larger natural weight range uses a
smaller scale, a layer with small weights a larger one, so each layer spends the
full `[-127, 127]` int8 range. `QA` and `SCALE` keep their v1 meanings and
defaults (`255`, `400`). The header `qb` field equals the final layer's scale
`QB_last`, preserving v1's invariant that `qb` is the scale in the dequantize.
Setting every `QB_k = 64` recovers a scale-uniform stack identical to reusing a
single `QB`. Per-bucket-per-layer scales are a future extension (a format bump),
not needed now.

**Quantized parameter types** (produced by quantization-aware export):

| Parameter | Float → integer | Integer type |
| --- | --- | --- |
| Stack weights `W_k` | `round(w · QB_k)` | **i8** |
| Stack biases `b_k` | `round(b · QA · QB_k)` | i32 |

Rounding at export is round-half-to-even, as v1. The i8 range is `[-127, 127]`
(−128 is excluded so magnitudes are symmetric and the widened-i16 SIMD kernel
never sees an asymmetric lane); export refuses any weight that rounds outside it.

**Why per-layer `QA·QB_k` for the bias.** Every stack layer's input lives in the
`[0, QA]` activation domain (the feature-transformer output for layer 0, and the
requantized activation for later layers), so an input integer `in` represents the
float `in / QA`. A layer-`k` weight integer `W` represents `W / QB_k`. Then

```text
acc_k = b_int + Σ in·W = (QA·QB_k)·b_float + Σ (QA·in_float)(QB_k·w_float)
      = (QA·QB_k) · (b_float + Σ in_float·w_float) = (QA·QB_k) · out_float
```

so layer `k`'s i32 accumulator is exactly `QA·QB_k` times the float layer output,
provided the bias is stored as `round(b_float · QA · QB_k)`. This is the *same*
relationship v1's single output layer has (`s = QA·QB · out_float`), so:

- **Between layers** (`k` not last) the activation domain is restored by
  `in_{k+1} = activation(clamp(round_div(acc_k, QB_k), 0, QA))`:
  `round_div(acc_k, QB_k) = QA · out_float`, and clamping to `[0, QA]` then
  applying the activation yields exactly `round(activation(clamp(out_float,0,1)) · QA)`
  — an integer back in `[0, QA]`, i.e. the same domain layer `k+1` expects. For
  CReLU the clamp *is* the activation; for SCReLU the clamped value is squared by
  the same `screlu_activation` v1 uses (`round_div(c·c, QA)`), which already maps
  `[0, QA] → [0, QA]`.
- **After the last layer** the scalar is dequantized to centipawns exactly as v1:
  `eval_cp = round_div(acc_last · SCALE, QA · QB_last)`, then clamped to the
  centipawn band `[-10_000, 10_000]`. Because the header `qb` equals `QB_last`,
  this is identical to `round_div(acc_last · SCALE, QA · qb)` — the v1 dequantize
  with the final-layer scale.

`round_div` is the same round-half-away-from-zero divide the v1 dequantize and
SCReLU use, computed in i64.

**Accumulation width and overflow.** Each `acc_k` is accumulated in **i32**. The
widest layer is layer 0: `2H` terms of `in ∈ [0, QA]` times `W ∈ [-127, 127]`,
so `|Σ in·W| ≤ 2H · QA · 127`. For `H = 256` that is `512 · 255 · 127 ≈ 1.66e7`,
three orders of magnitude inside i32; the i32 bias adds a value of order
`QA·QB_k · out_float`, also tiny. The subsequent `acc_last · SCALE` is widened to
**i64** before the divide, exactly as v1. Export bounds the stack weights to i8
and the biases to i32 and refuses anything that would not fit. The per-layer
scales do not affect these bounds: an int8 weight is in `[-127, 127]` whatever
scale produced it, so `|Σ in·W| ≤ 2H · QA · 127` and the i32/i64 widths are
unchanged from the uniform-scale case.

**SIMD bit-identity — the widened-i16 kernel.** The natural int8 kernel
`vpmaddubsw` (u8×i8 → i16 pairwise) *saturates* its i16 pair-sum: with `in ≤ 255`
and `|W| ≤ 127`, a pair `in0·W0 + in1·W1` can reach `±64 770`, past `i16::MAX`,
so `vpmaddubsw` would diverge from the exact scalar sum and break the scalar↔SIMD
bit-identity this contract requires. The stack is small (only the selected
bucket runs; the feature transformer dominates per-node cost), so there is no
reason to pay for that: the AVX2 kernel instead **widens the int8 weights to i16**
and uses `vpmaddwd` (i16×i16 → i32 pairwise, *non-saturating*) against the i16
activations. Every partial product and pair-sum then lands in i32 with no
saturation, so the kernel returns exactly the integer the scalar loop does,
across the full `[0, QA]` activation range and the full `[-127, 127]` weight
range. Every layer's input dimension is a multiple of 16, so each dot product is
whole 16-lane vectors with no scalar remainder. The scale enters only at the
scalar requantize divide (`round_div(acc_k, QB_k)`); the `vpmaddwd` kernel
operates on raw int8-widened weights and never sees a scale, so the bit-identity
argument is unchanged by per-layer scales.

## File format version 2

Format version 2 keeps the fixed **64-byte little-endian header** and its v1
field layout through `param_hash`, and uses the previously-reserved tail of the
header plus a short blob prefix to describe the stack. A version-1 file is still
read by the v1 rules; a version-2 file by these.

**Header (64 bytes).** Offsets `0..40` are exactly the v1 layout (`magic`,
`format_version = 2`, `feature_set_id`, `input_dim`, `hidden_width`,
`output_dim = 1`, `activation_id`, `qa`, `qb`, `scale`, `param_bytes`,
`param_hash`). The formerly-reserved region carries two new fields; the rest
stays reserved and must be zero:

| Offset | Size | Field | Type | v2 value / meaning |
| --- | --- | --- | --- | --- |
| 40 | 2 | `num_buckets` | u16 | number of output stacks; `1..=32` |
| 42 | 2 | `num_output_layers` | u16 | layers in each stack; `≥ 1` |
| 44 | 20 | `reserved` | bytes | all zero in v2 |

**Parameter blob** (immediately after the header, all little-endian, in order):

| Block | Element | Count | Layout |
| --- | --- | --- | --- |
| `stack_dims` | u32 | `num_output_layers` | `out_dim_k` for each layer; last must be `output_dim` (1); each earlier must be a positive multiple of 16 |
| `stack_scales` | u32 | `num_output_layers` | `QB_k` for each layer; each `> 0`; last entry `== qb` |
| `W_ft` | i16 | `input_dim · H` | feature-major, exactly as v1 |
| `b_ft` | i16 | `H` | — |
| per bucket `b` in `0..num_buckets`, per layer `k` in `0..num_output_layers`: | | | |
| `W_{b,k}` | **i8** | `out_dim_k · in_dim_k` | output-major: element `(o, i)` at `o · in_dim_k + i`, with `in_dim_0 = 2H`, `in_dim_k = out_dim_{k-1}` |
| `b_{b,k}` | i32 | `out_dim_k` | — |

So, with `S = Σ_k (out_dim_k · in_dim_k)` the total stack weight count per bucket
and `T = Σ_k out_dim_k` the total stack bias count per bucket:

```text
param_bytes = 4·num_output_layers            // stack_dims
            + 4·num_output_layers            // stack_scales
            + 2·(input_dim·H) + 2·H           // feature transformer
            + num_buckets · (1·S + 4·T)       // int8 weights + i32 biases, all buckets
```

`param_hash` is the FNV-1a hash of the entire blob including `stack_dims` and
`stack_scales`, exactly as v1.

**Deterministic rejection.** In addition to the v1 rules (magic, version this
build implements, feature set, activation, input/hidden/output dims, positive
scales, `param_bytes` and `param_hash` consistency), a version-2 loader rejects,
each as a distinct error and before interpreting any weights, a file that:

1. declares `num_buckets` outside `1..=32`;
2. declares `num_output_layers < 1`;
3. has a `stack_dims` last entry `≠ output_dim` (1), or any earlier entry that is
   zero or not a multiple of 16;
4. has any of the header bytes `44..64` non-zero;
5. has a `param_bytes` that disagrees with the size the dimensions above imply;
6. has any `stack_scales` entry `≤ 0`;
7. has `stack_scales[num_output_layers − 1] ≠ qb` (the final-layer scale must
   match the header `qb`).

A version-1 file continues to require **all** of `40..64` to be zero, so a v1
loader can never misread v2 bucket/layer counts as reserved-must-be-zero, and a
v2 file is never mistaken for v1 because the version field gates the whole
interpretation.

## Backward compatibility

The engine loads and evaluates **both** versions. The built-in default network
(gen-002, format version 1, single linear output) keeps loading and evaluating
byte-for-byte as today; format version 2 is additive. The in-memory `Network`
carries the feature transformer plus an output representation that is either the
v1 single linear layer (i16 weights, i32 bias) or the v2 bucketed int8 stack
(int8 weights, i32 biases, plus the per-layer `QB_k` scale vector); the
accumulator and every feature-transformer path are shared and unchanged. The
first *v2* network is trained and promoted by the sweep (TASK-86.5); until then
the engine ships a v1 network and the v2 path is exercised only by tests and by
operator-supplied `--eval-file` networks.

## PyTorch / export correspondence

The float trainer mirrors this topology: the feature transformer and activation
are unchanged, and the single output `Linear` is replaced by `num_buckets`
independent stacks of `nn.Linear` layers with the header activation between them.
The forward pass selects a sample's bucket by the same piece-count rule and runs
only that bucket's stack. Quantization-aware training fake-quantizes layer `k`'s
weights onto the per-layer `1/QB_k` int8 grid (not a single shared grid) and the
inter-layer activations onto the `1/QA` grid, and bounds the stack weights so the
exported i8 cast cannot overflow, so the exported integer network reproduces the
trained behaviour. Export writes `stack_scales` from the config. The exporter's
integer forward pass (`integer_eval_cp`) is the same arithmetic as
`engine::nnue::forward`, which is what makes the three-way golden equivalence
(Python ↔ Rust scalar ↔ Rust AVX2) testable across positions spanning multiple
buckets.

## Test expectations

The golden fixture's network **must** span multiple buckets across its positions
and its `stack_scales` **must not be all equal** (e.g. `[64, 64, 256]`). A
uniform-scale golden net would pass even if an implementation ignored
`stack_scales` and hard-coded `qb`, so the three-way differential test must
exercise distinct per-layer scales to prove the per-layer path in all three
implementations. The bucket-selecting positions must reach at least two distinct
buckets so a stack-selection bug cannot hide behind a single always-selected
bucket.
