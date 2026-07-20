# NNUE design contract

This document is the single shared contract that every NNUE subtask of TASK-69
forks from. It is a decision record, not code. It fixes the decisions that are
expensive to change once implementation fans out across four tracks (Rust
inference, Rust self-play data generation, Python training, and the
reinforcement loop), and it deliberately leaves parameterizable what is cheap to
vary.

Status: accepted, July 2026. Consumed by TASK-69.2 through TASK-69.12. If a
decision here changes after those tasks start, change it here first and bump the
file-format version (below) rather than letting the Rust and Python paths drift.

The overriding constraint from the parent task (TASK-69) is **self-play
purity**: playing strength is bootstrapped entirely from the engine's own play.
No external games, positions, or evaluations enter the system. That constraint
shapes several decisions below and is stated concretely in its own section.

## Summary of decisions

| Area | Decision (fixed) | Left parameterizable |
| --- | --- | --- |
| Feature set | Perspective-doubled piece-square, 768 inputs per perspective, no king buckets | — (a costlier HalfKA-style set is a future `feature_set_id`) |
| Topology | Feature transformer (768 → `H` per perspective) → clipped-ReLU → single linear output (`2H` → 1); two perspectives concatenated side-to-move first | Hidden width `H`; activation id; output scale |
| Quantization | int16 feature transformer, int16 output weights, int32 output bias; clipped-ReLU to `[0, QA]`; i64 final dequantize; round-half-away-from-zero | Scale constants `QA`, `QB`, `SCALE` |
| File format | `SBNN` magic, fixed 64-byte little-endian header carrying architecture + scales + blob hash, then a tightly ordered parameter blob | Values of the header fields |
| Training target | MSE in win-probability space against `λ · game_result + (1 − λ) · sigmoid(search_cp / SCALE)`, all from the side-to-move perspective | `λ` and its schedule; loss (MSE/BCE); optimizer |
| Purity | Only internal priors (hand-crafted eval seed, architecture choice, own search scores, own game outcomes) | — |

Where the code lives: the network, its quantized inference, and its accumulator
belong in the `engine` crate as a sibling of the existing `eval` module, exactly
where `EvalState` and the `Evaluation` trait already live
(`engine/src/eval.rs`). The accumulator attaches to the incremental-update seam
in the `chess` crate (`PieceDeltaSink`, `chess/src/position/mod.rs`). Evaluation
selection is introduced at the single consumption point `Search::evaluate`
(`engine/src/search.rs`), keeping the hand-crafted tapered evaluation the default
until a trained network exists and passes its strength gate (TASK-69.4).

## Self-play purity boundary

This section is normative for every subtask. The claim TASK-69 must be able to
make is that the network's strength was produced by the engine playing itself,
seeded only by the existing hand-crafted evaluation.

**Permitted internal priors:**

- The hand-crafted tapered evaluation (`tapered_evaluation`, `engine/src/eval.rs`)
  as the *only* evaluation available at generation 0. Generation-0 self-play
  games are played by the engine using this evaluation; their search scores and
  game outcomes are the first training labels. Each later generation is labelled
  by the engine playing with the previous generation's network.
- The design priors in this document: the feature set, network topology,
  quantization scheme, and file format. These are human choices, not learned
  from data.
- Training and self-play hyperparameters (`H`, `λ`, learning rate, node budgets,
  adjudication thresholds, RNG seeds). These are configuration, not data.
- The engine's **own** search output used as labels: per-position search scores
  and self-play game results. These are the reinforcement signal.

**Forbidden external inputs:**

- External game databases (for example Lichess or CCRL PGN dumps) as training
  positions, moves, or outcomes.
- External engine evaluations or pretrained networks. No distillation from a
  foreign engine, no transfer-learning from or fine-tuning of a foreign net, no
  imported weights.
- External or curated opening books as self-play starting positions. Opening
  diversification (TASK-69.7) must come from **internal randomization** — a few
  seeded random or softmax-sampled plies from the standard initial position —
  not from an imported position set. Randomness introduces variety without
  introducing external knowledge.
- Human-annotated labels of any kind.
- Endgame-tablebase results as training labels. Tablebase win/draw/loss is
  external perfect knowledge, so to keep the "trained entirely from self-play"
  claim unambiguous, tablebase probing is disabled during self-play data
  generation. (The bundled Syzygy tablebases remain available for *testing*,
  which measures strength and is not a training input.)

The one measurement that legitimately uses a repo-authored, external artifact is
strength testing itself: the FastChess SPRT harness (`docs/strength-testing.md`)
plays from `openings-v1.epd`. That artifact scores strength; it never becomes a
network prior, so it does not breach the boundary.

## Input feature set

**Decision: perspective-doubled piece-square, 768 inputs per perspective, no
king buckets.** This is the cheapest set that exercises the whole pipeline
end to end, because its incremental update is trivial (one feature toggles per
placement) and it attaches directly to the existing `PieceDeltaSink` seam. A
costlier HalfKA-style king-bucketed set is deferred; introducing it later is a
new `feature_set_id` and a format-version bump, not a silent reinterpretation.

There are two accumulators, one per perspective (White and Black). Each sees a
768-dimensional sparse binary input: `2 colours × 6 piece types × 64 squares`.
A perspective's input encodes every piece as "friendly" or "enemy" relative to
that perspective, and orients the board so the perspective side's first rank is
the bottom.

**Feature index (normative).** For a piece of colour `c` and type `t`
(`t ∈ {Pawn, Knight, Bishop, Rook, Queen, King}`) standing on square `sq`, its
index in the 768-vector *for perspective `p`* is:

```text
oriented = p.relative_square(sq)      // sq for White; sq ^ 56 (vertical flip) for Black
pt0      = piece_type_ordinal(t)      // Pawn=0, Knight=1, Bishop=2, Rook=3, Queen=4, King=5
side     = if c == p { 0 } else { 1 } // 0 = friendly, 1 = enemy
index    = oriented + 64 * pt0 + 384 * side   // in 0..768
```

This reuses the repository's existing conventions exactly: squares are `a1 = 0`,
rank-major little-endian file (`chess/src/position/square.rs`); `Player`'s
`relative_square` already computes `sq ^ (0 for White, 56 for Black)`
(`chess/src/position/mod.rs`); piece-type ordinals follow `PieceType`
(`Pawn = 1 … King = 6` in `chess/src/position/piece.rs`), shifted to `0..6` here
because index 0 of that enum is `None`. No horizontal mirroring and no
king-relative bucketing are applied, matching "no king buckets".

**Incremental update.** Because the index depends only on the moving piece and
its square, a single `PieceDeltaSink::add`/`remove` toggles exactly one feature
in *each* of the two perspective accumulators. On `add(piece, square)` the
accumulator adds the feature-transformer weight column for that piece's index (in
each perspective); on `remove` it subtracts. This mirrors how `EvalState`
already consumes the same delta stream, so an NNUE accumulator is a second
`PieceDeltaSink` alongside it (TASK-69.3), with the from-scratch rebuild serving
as the debug-time equivalence reference exactly as `EvalState::from_position`
does today.

## Network topology and parameterizable dimensions

**Fixed structure:**

1. **Feature transformer.** A linear map `768 → H` with bias, applied
   independently to each perspective's sparse input, producing two `H`-vectors:
   `acc[White]` and `acc[Black]`.
2. **Perspective concatenation.** At evaluation time the two accumulators are
   concatenated **side-to-move first**: `x = concat(acc[stm], acc[~stm])`, a
   `2H`-vector. Ordering by side to move (not by colour) is what makes the
   network symmetric — a position and its colour-flipped mirror produce equal
   and opposite evaluations.
3. **Activation.** A clipped ReLU is applied elementwise to `x`.
4. **Output.** A single linear map `2H → 1` with bias produces the scalar
   network output.

This is the smallest topology that proves the reinforcement loop: one nonlinear
hidden stage plus a linear read-out. Deeper stacks (for example `2H → 32 → 32 →
1`) are a deliberate future extension. Adding one is a new format version and a
new `activation`/layer descriptor, never a silent change to the `v1` layout.

**Parameterizable dimensions** (carried in the file header, validated at load):

- **Hidden width `H`** — the feature-transformer output width per perspective.
  Default `256` (so `2H = 512` into the output layer). `H` must be a positive
  multiple of `16` so a single file loads unchanged into both the scalar and the
  future AVX2 path (TASK-69.5), whose i16 lanes process 16 elements at a time.
- **Activation id** — `0 = clipped ReLU (CReLU)`, the default and only value
  `v1` inference implements. `1 = squared clipped ReLU (SCReLU)` is reserved for
  a later version; a loader that does not implement an id rejects the file.
- **Output scale `SCALE`** — the constant that converts the network's internal
  output to centipawns and, identically, ties centipawns to win probability in
  the training target (see below). Default `400`.

## Quantization scheme

This is the single place the Rust inference path and the PyTorch training path
most often silently diverge, so the integer types, scale factors, activation
semantics, and overflow behaviour are all fixed here and must be implemented
identically on both sides. The three-way differential test (TASK-69.10) exists
to catch any drift.

**Scale constants** (header fields, defaults): `QA = 255` (feature-transformer /
activation scale), `QB = 64` (output-weight scale), `SCALE = 400` (internal →
centipawn conversion). All are positive.

**Quantized parameter types** (produced by quantization-aware export,
TASK-69.9):

| Parameter | Float → integer | Integer type |
| --- | --- | --- |
| Feature-transformer weights `W_ft` | `round(w · QA)` | i16 |
| Feature-transformer bias `b_ft` | `round(b · QA)` | i16 |
| Output weights `W_out` | `round(w · QB)` | i16 |
| Output bias `b_out` | `round(b · QA · QB)` | i32 |

Rounding at export is **round half to even** (the NumPy/PyTorch `.round()`
default), applied identically wherever weights are quantized.

**Quantized inference (normative arithmetic).** With `f` ranging over the active
features of each perspective:

```text
acc[p][i] = b_ft[i] + Σ_f W_ft[f][i]          // i16 accumulator, one per perspective
x         = concat(acc[stm], acc[~stm])        // 2H, i16
a[j]      = clamp(x[j], 0, QA)                  // clipped ReLU, i16 in [0, QA]
s         = b_out + Σ_j (a[j] as i32) * (W_out[j] as i32)   // i32 accumulate
eval_cp   = round_div(s as i64 * SCALE, QA * QB)            // i64 multiply, then rounded divide
```

`eval_cp` is finally clamped to the centipawn band `[-10_000, 10_000]` and used
as `Score::cp(eval_cp)` — the same band the `Score` type reserves for centipawn
evaluations (`engine/src/score.rs`), well below the mate band at `±20_000`.

Semantics that must match on both sides:

- **Clipped-ReLU domain.** The float model clips activations to `[0, 1]`; the
  quantized model clips to `[0, QA]`. `1.0` in float corresponds to `QA` in
  integer. Values outside the range are hard-clamped, not scaled.
- **Accumulator type and saturation.** The feature-transformer accumulator is
  **i16**. At most 32 features are active per perspective (one per piece on the
  board), so `acc = b_ft + (≤ 32 weight columns)`. Export must verify that
  `|acc|` cannot exceed `i16::MAX` for any reachable position by bounding the
  feature-transformer weight magnitudes during quantization-aware training;
  i16 overflow here is a defect, not a wrap. Keeping the accumulator in i16 is
  what lets the AVX2 path (TASK-69.5) use i16 lanes.
- **Output accumulation.** `s` is accumulated in **i32**. With `|a| ≤ QA` and
  the bounded `W_out`, `|s|` stays far inside i32; the subsequent multiply by
  `SCALE` is done in **i64** to avoid overflow before the divide.
- **Rounded division.** `round_div(num, den)` for positive `den` is
  `num >= 0 ? (num + den/2) / den : -((-num + den/2) / den)` — round half away
  from zero, computed in i64. The golden-vector reference generator (TASK-69.10)
  uses this exact formula so the integer paths agree bit for bit; the float
  model is expected to agree only within a documented tolerance.

## On-disk file format

**Decision: a versioned binary format with a fixed 64-byte little-endian header
that carries the architecture parameters, the quantization scales, and a hash of
the parameter blob, followed by the parameter blob.** A loader constructs the
network from the header and refuses any file it cannot interpret exactly, rather
than misinterpreting it. The repository has no prior binary-loading precedent
(runtime tables are generated, not loaded), so this format is greenfield and
owned by TASK-69.2.

All multi-byte fields are little-endian (the engine targets x86-64).

**Header (64 bytes):**

| Offset | Size | Field | Type | `v1` value / meaning |
| --- | --- | --- | --- | --- |
| 0 | 4 | `magic` | bytes | `SBNN` (`0x53 0x42 0x4E 0x4E`) |
| 4 | 2 | `format_version` | u16 | `1` |
| 6 | 2 | `feature_set_id` | u16 | `0` = perspective-768 |
| 8 | 4 | `input_dim` | u32 | `768` |
| 12 | 4 | `hidden_width` (`H`) | u32 | e.g. `256`; positive multiple of 16 |
| 16 | 2 | `output_dim` | u16 | `1` |
| 18 | 2 | `activation_id` | u16 | `0` = CReLU |
| 20 | 2 | `qa` | u16 | `255` |
| 22 | 2 | `qb` | u16 | `64` |
| 24 | 4 | `scale` | i32 | `400` |
| 28 | 4 | `param_bytes` | u32 | byte length of the parameter blob |
| 32 | 8 | `param_hash` | u64 | FNV-1a hash of the parameter blob |
| 40 | 24 | `reserved` | bytes | all zero in `v1` |

**Parameter blob** (immediately after the header, all little-endian, in this
exact order):

| Block | Element type | Count | Layout |
| --- | --- | --- | --- |
| `W_ft` | i16 | `input_dim × H` | feature-major: element `(f, i)` at offset `f · H + i`, so one feature's `H` weights are contiguous |
| `b_ft` | i16 | `H` | — |
| `W_out` | i16 | `2H` | first `H` = own-perspective block, next `H` = enemy block, matching `concat(acc[stm], acc[~stm])` |
| `b_out` | i32 | `output_dim` (`1`) | — |

So `param_bytes = 2·(input_dim·H) + 2·H + 2·(2H) + 4·output_dim`.

**Deterministic rejection.** The loader must reject, each as a distinct error and
before allocating or interpreting any weights, a file that:

1. is shorter than the 64-byte header, or whose trailing bytes do not exactly
   match `param_bytes`;
2. has a `magic` other than `SBNN`;
3. has a `format_version` this build does not implement;
4. has a `feature_set_id` or `activation_id` this build does not implement;
5. has an `input_dim` inconsistent with `feature_set_id` (768 for id 0), or an
   `H` that is not a positive multiple of 16, or an `output_dim ≠ 1`;
6. has a non-positive `qa`, `qb`, or `scale`;
7. has any non-zero `reserved` byte (so a future flag can never be silently
   ignored by an older loader);
8. has a `param_bytes` that disagrees with the size implied by the dimensions;
9. has a `param_hash` that does not match the blob (corruption or truncation).

This satisfies "a loader refuses a file it does not understand rather than
misinterpreting it": every field that determines how the bytes are read is both
stored and checked, and an architecture the running build cannot evaluate is
rejected rather than run.

## Training target formulation

**Decision: blend the engine's own search score with the self-play game outcome,
compared in win-probability space, with the blend weight `λ` scheduled.**

Each training sample carries a position, its side to move, the engine's search
score for that position in centipawns from the side-to-move perspective
(`search_cp`, produced during self-play data generation, TASK-69.6), and the
game result for the side to move (`r ∈ {1.0 win, 0.5 draw, 0.0 loss}`).

```text
score_target = sigmoid(search_cp / SCALE)          // search score → win probability
y            = λ · r + (1 − λ) · score_target       // blended target in [0, 1]
p            = sigmoid(fout)                         // model prediction; fout is the float output
loss         = (p − y)^2                             // MSE in win-probability space
```

The **same `SCALE`** converts centipawns to win probability here and converts the
network's internal output to centipawns at inference. Tying them means the value
the network learns to emit and the value search consumes are the same quantity;
this coupling is deliberate and must not be broken by choosing a different scale
in the trainer.

**Convention:** `λ` is the weight on the **game outcome**. `λ = 0` trusts the
search score entirely; `λ = 1` trusts the game result entirely. Downstream code
must use this convention.

**Schedule (parameterizable).** Self-play game outcomes from a weak bootstrap are
noisy, so early training should lean on search scores (small `λ`) and shift
toward outcomes as strength grows across reinforcement generations. Default: a
constant `λ = 0.3` as the simplest starting point, with a documented option to
ramp `λ` from `0.1` to `0.5` over generations. `λ`, its schedule, the loss
(MSE is the default; BCE is a permitted alternative), and the optimizer are
training-side configuration (TASK-69.9, TASK-69.11) and are **not** stored in the
network file — only the trained weights and the architecture that reproduces
them are.

## Fixed versus parameterizable, at a glance

**Fixed** (expensive to change; baked into code and the `v1` format, and a change
requires a format-version bump):

- The perspective-768 feature set and its exact index formula.
- Two per-perspective accumulators, concatenated side-to-move first.
- The quantization scheme: integer types, the role of each scale constant,
  clipped-ReLU semantics, i16 accumulator with its saturation invariant, i32
  output accumulation, i64 rounded dequantization, and the rounding modes.
- The file-format structure: magic, header field layout, blob order, endianness,
  and the rejection rules.
- The training *formulation*: a blended sigmoid target with a shared `SCALE`,
  measured from the side to move.
- The self-play purity boundary.

**Parameterizable** (cheap to vary; header fields or training config):

- Hidden width `H`, activation id, and the scale constants `QA`, `QB`, `SCALE`.
- `λ` and its schedule, the loss function, the optimizer and learning rate, the
  self-play node budget, adjudication thresholds, and RNG seeds.
- The centipawn clamp applied to the network output.

## What each subtask consumes

- **TASK-69.2** (file format and loader): the header layout, blob order, and
  rejection rules.
- **TASK-69.3** (accumulator as `PieceDeltaSink`): the feature set, the feature
  index formula, and the two-perspective accumulator structure.
- **TASK-69.4** (scalar selectable inference): the quantized inference
  arithmetic and the `Search::evaluate` selection seam.
- **TASK-69.5** (AVX2 inference): the i16 accumulator and the `H`-multiple-of-16
  constraint that lets one file serve both paths.
- **TASK-69.6, .7** (self-play data, sample format, diversification): the label
  definitions (`search_cp`, `r`) and the purity boundary, including internal
  opening randomization.
- **TASK-69.8, .9** (PyTorch model and quantization-aware export): the topology,
  the quantization scheme, and the export rounding rules.
- **TASK-69.10** (three-way differential test): the exact integer arithmetic and
  rounding, which make bit-for-bit agreement testable.
- **TASK-69.11, .12** (reinforcement loop and bootstrap run): the training-target
  formulation, `λ` schedule, and purity boundary across generations.
