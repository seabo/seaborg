//! The quantized forward pass: from the two per-perspective accumulators through
//! the clipped activation and the output layer to a single centipawn score.
//!
//! Two implementations of the output layer's clipped dot product live here — a
//! portable scalar loop and a hand-written AVX2 kernel — behind a runtime
//! selector. The scalar loop is the reference implementation of the network's
//! arithmetic: it runs on every target, including those without AVX2, and it is
//! the oracle the SIMD path and the PyTorch quantized forward are both checked
//! against, so its integer arithmetic is normative — the scale factors, the
//! clipped-ReLU domain, the accumulation widths, and the rounding mode all follow
//! `docs/nnue-design-contract.md` exactly and must not drift from it. The AVX2
//! kernel is a pure optimization: it is defined to produce the identical i32 sum
//! the scalar loop does, never a re-derived one, and the differential tests below
//! assert that bit-for-bit.
//!
//! Everything after the dot product — seeding the bias, widening to i64, scaling,
//! the rounded divide, and the centipawn clamp — is cheap scalar tail work shared
//! by both paths, so the two can differ only in how the dot product is summed and
//! not in how the result is rounded.
//!
//! The accumulators hold the first linear layer's output for both perspectives
//! (see [`Accumulator`]). This module performs only the steps after them:
//! concatenating the two perspectives side-to-move first, clipping, the output
//! dot product, and the rounded dequantization to centipawns. It holds no state
//! and borrows the network and accumulator it reads.

use chess::position::Player;

use super::{Accumulator, Activation, BucketedStack, Network, OutputStack, StackLayer};

/// The centipawn band a network evaluation is clamped into before it becomes a
/// score. It matches the range [`crate::score::Score`] reserves for centipawn
/// evaluations, well inside the mate band, so a saturated network output can
/// never be mistaken for a mate.
const EVAL_CP_MIN: i64 = -10_000;
const EVAL_CP_MAX: i64 = 10_000;

/// Evaluates `accumulator` through the output layer and returns the network's
/// score in centipawns from `side_to_move`'s perspective.
///
/// The arithmetic is the normative quantized forward pass. With `acc[stm]` and
/// `acc[~stm]` the two perspective accumulators and `H` the hidden width:
///
/// ```text
/// x[j]    = concat(acc[stm], acc[~stm])[j]            // 2H, i16
/// a[j]    = clamp(x[j], 0, QA)                        // clipped ReLU, i16 in [0, QA]
/// s       = b_out + Σ_j (a[j] as i32) · (W_out[j] as i32)     // i32 accumulate
/// eval_cp = round_div(s as i64 · SCALE, QA · QB)             // i64 multiply, rounded divide
/// ```
///
/// `eval_cp` is then clamped to the centipawn band. Concatenating side-to-move
/// first is what makes the output already relative to the mover, so unlike the
/// hand-crafted evaluation this value needs no perspective flip applied by the
/// caller.
///
/// The `a[j]` line above is the CReLU activation. A network may instead carry the
/// SCReLU activation, `a[j] = round_div(clamp(x[j], 0, QA)², QA)`; only that one
/// stage changes, and because it too produces a value in `[0, QA]`, every
/// subsequent step — the i32 sum, the `SCALE` multiply, the rounded divide — is
/// identical. See `docs/nnue-design-contract.md`.
///
/// The output accumulator `s` is i32 and the multiply by `SCALE` widens to i64
/// before the divide, exactly as the contract requires: with the accumulator in
/// i16, the activations are clamped to `[0, QA]` so each output term is bounded
/// and, for contract-bounded output weights, `s` stays well inside i32 while the
/// subsequent `s · SCALE` can exceed it and so is done in i64.
///
/// `piece_count` is the number of pieces on the board; a bucketed (version-2)
/// network uses it to select one output-stack bucket, and a single-layer
/// (version-1) network ignores it.
///
/// # Panics
///
/// Panics if `accumulator` was not built from `network` — the two perspectives
/// must be `H` long and the output weight block `2H` long. Pairing an
/// accumulator with a foreign network is a programming error, and the mismatch
/// is caught rather than silently reading past a block.
pub fn forward(
    network: &Network,
    accumulator: &Accumulator,
    side_to_move: Player,
    piece_count: u32,
) -> i32 {
    match network.output() {
        OutputStack::Single { .. } => {
            forward_with(network, accumulator, side_to_move, dot_clipped_selected)
        }
        OutputStack::Bucketed(stack) => forward_bucketed_with(
            network,
            stack,
            accumulator,
            side_to_move,
            piece_count,
            dot_i8_selected,
        ),
    }
}

/// The output bucket a position with `piece_count` pieces selects, binning the
/// reachable `1..=32` into `num_buckets` equal ranges:
/// `min((piece_count - 1) · B / 32, B - 1)`. A pure function of piece count, so it
/// is identical across the scalar, SIMD, and Python paths. See
/// `docs/nnue-topology-v2.md`.
pub fn select_bucket(piece_count: u32, num_buckets: u16) -> usize {
    let b = u32::from(num_buckets);
    let index = (piece_count.saturating_sub(1) * b / 32).min(b.saturating_sub(1));
    index as usize
}

/// The forward pass parameterized by the clipped dot product used for each
/// perspective block. [`forward`] passes [`dot_clipped_selected`], the runtime
/// dispatcher, so production always runs the widest path the CPU supports. The
/// cross-language differential test passes the scalar [`dot_clipped`] and the AVX2
/// kernel explicitly, so it can assert both land on the identical score rather than
/// running whichever one dispatch happens to pick. Everything but the dot product —
/// the bias seed, the i64 widen, the rounded divide, and the clamp — is shared
/// here, so a chosen `dot` changes only how each block sum is formed and never how
/// the result is rounded.
#[inline]
fn forward_with(
    network: &Network,
    accumulator: &Accumulator,
    side_to_move: Player,
    dot: impl Fn(&[i16], &[i16], i32) -> i32,
) -> i32 {
    let hidden = network.hidden_width() as usize;
    let qa = i32::from(network.qa());
    let weights = network.output_weights();
    assert_eq!(
        weights.len(),
        2 * hidden,
        "output weight block must be 2H long for the network's hidden width"
    );

    let own = accumulator.perspective(side_to_move);
    let enemy = accumulator.perspective(side_to_move.other_player());
    let (own_weights, enemy_weights) = weights.split_at(hidden);

    // Output bias seeds the i32 accumulator; `OUTPUT_DIM` is 1, so there is one.
    // The activation is the only stage that varies between networks: CReLU feeds
    // the raw accumulator block to the clipped dot (whose clip performs the
    // activation), while SCReLU squares each clipped entry first and then feeds the
    // pre-activated block through the identical clipped dot.
    let mut s: i32 = network.output_bias()[0];
    match network.activation() {
        Activation::ClippedRelu => {
            s += dot(own, own_weights, qa);
            s += dot(enemy, enemy_weights, qa);
        }
        Activation::SquaredClippedRelu => {
            s += dot_screlu(own, own_weights, qa, &dot);
            s += dot_screlu(enemy, enemy_weights, qa, &dot);
        }
    }

    // Widen to i64 before scaling: `s` fits i32 but `s · SCALE` need not.
    let numerator = i64::from(s) * i64::from(network.scale());
    let denominator = i64::from(network.qa()) * i64::from(network.qb());
    let eval_cp = round_div(numerator, denominator);
    eval_cp.clamp(EVAL_CP_MIN, EVAL_CP_MAX) as i32
}

/// The widest activation buffer the bucketed forward pass keeps on the stack.
/// It covers the `2H` activated input for `H ≤ 512` and any intermediate layer no
/// wider than this without heap traffic; a wider network still evaluates, via a
/// one-time heap buffer. Two of these ping-pong between layers.
const STACK_SCRATCH: usize = 1024;

/// A per-layer activation buffer that lives on the stack for the small widths this
/// engine trains and spills to the heap only for an unusually wide network, so the
/// per-node bucketed forward pass allocates nothing in the common case.
///
/// The size gap between the inline-array and heap variants is deliberate: the
/// inline array *is* the optimization — it keeps the buffer off the heap in the
/// search hot loop, where a per-node allocation would negate the incremental
/// accumulator's speed. Boxing it to equalize the variants would reintroduce that
/// allocation, so the lint is suppressed here rather than obeyed.
#[allow(clippy::large_enum_variant)]
enum Scratch {
    Stack { data: [i16; STACK_SCRATCH], len: usize },
    Heap(Vec<i16>),
}

impl Scratch {
    fn new(len: usize) -> Self {
        if len <= STACK_SCRATCH {
            Scratch::Stack {
                data: [0i16; STACK_SCRATCH],
                len,
            }
        } else {
            Scratch::Heap(vec![0i16; len])
        }
    }

    fn as_slice(&self) -> &[i16] {
        match self {
            Scratch::Stack { data, len } => &data[..*len],
            Scratch::Heap(v) => v,
        }
    }

    fn as_mut_slice(&mut self) -> &mut [i16] {
        match self {
            Scratch::Stack { data, len } => &mut data[..*len],
            Scratch::Heap(v) => v,
        }
    }
}

/// The version-2 bucketed forward pass, parameterized by the int8 dot product used
/// for each stack layer.
///
/// [`forward`] passes [`dot_i8_selected`], the runtime dispatcher; the
/// differential test passes the scalar [`dot_i8`] and the AVX2 kernel explicitly so
/// it can assert both land on the identical score. The pass materializes the
/// activated `2H` input once (concatenated side-to-move first, each entry activated
/// into `[0, QA]`), selects the bucket by piece count, then runs that bucket's
/// affine layers: each layer's i32 accumulator is requantized with the layer's
/// scale and activated back into `[0, QA]` for the next layer, and the final layer
/// is dequantized to centipawns exactly as the version-1 output is. See
/// `docs/nnue-topology-v2.md`.
#[inline]
fn forward_bucketed_with(
    network: &Network,
    stack: &BucketedStack,
    accumulator: &Accumulator,
    side_to_move: Player,
    piece_count: u32,
    dot: impl Fn(&[i16], &[i8]) -> i32,
) -> i32 {
    let hidden = network.hidden_width() as usize;
    let two_h = 2 * hidden;
    let qa = i64::from(network.qa());
    let activation = network.activation();

    let bucket = select_bucket(piece_count, stack.num_buckets());
    let layers = stack.bucket(bucket);
    let dims = stack.layer_dims();
    let scales = stack.layer_scales();

    // Two ping-pong buffers sized to the widest vector any layer reads or writes.
    // The `2H` activated input dominates; hidden layer outputs are smaller.
    let widest_dim = dims.iter().copied().max().unwrap_or(0) as usize;
    let cap = two_h.max(widest_dim);
    let mut buf_a = Scratch::new(cap);
    let mut buf_b = Scratch::new(cap);

    // Materialize the activated input into buf_a[..2H]: own perspective first.
    let own = accumulator.perspective(side_to_move);
    let enemy = accumulator.perspective(side_to_move.other_player());
    {
        let input = &mut buf_a.as_mut_slice()[..two_h];
        let (own_half, enemy_half) = input.split_at_mut(hidden);
        for (dst, &x) in own_half.iter_mut().zip(own) {
            *dst = activate_qa_domain(activation, i64::from(x), qa);
        }
        for (dst, &x) in enemy_half.iter_mut().zip(enemy) {
            *dst = activate_qa_domain(activation, i64::from(x), qa);
        }
    }

    // Run the stack, ping-ponging between the two buffers. `in_dim` is the current
    // input width; `from_a` says which buffer holds it. The final layer produces
    // one scalar, `final_acc`, that is dequantized below.
    let mut in_dim = two_h;
    let mut from_a = true;
    let mut final_acc: i64 = 0;
    for (k, layer) in layers.iter().enumerate() {
        let out_dim = dims[k] as usize;
        let scale = i64::from(scales[k]);
        let is_last = k + 1 == layers.len();

        if is_last {
            // A single output neuron: bias plus the int8 dot over the whole input.
            let input = if from_a {
                &buf_a.as_slice()[..in_dim]
            } else {
                &buf_b.as_slice()[..in_dim]
            };
            debug_assert_eq!(out_dim, 1, "the final stack layer emits one scalar");
            final_acc = i64::from(layer.b[0]) + i64::from(dot(input, &layer.w[..in_dim]));
        } else if from_a {
            let (src, dst) = (&buf_a, &mut buf_b);
            affine_activate(
                &src.as_slice()[..in_dim],
                layer,
                in_dim,
                scale,
                activation,
                qa,
                &mut dst.as_mut_slice()[..out_dim],
                &dot,
            );
        } else {
            let (src, dst) = (&buf_b, &mut buf_a);
            affine_activate(
                &src.as_slice()[..in_dim],
                layer,
                in_dim,
                scale,
                activation,
                qa,
                &mut dst.as_mut_slice()[..out_dim],
                &dot,
            );
        }

        in_dim = out_dim;
        from_a = !from_a;
    }

    // Dequantize the scalar output to centipawns: identical to the version-1
    // read-out, with the final layer's scale (the network's `qb`).
    let numerator = final_acc * i64::from(network.scale());
    let denominator = qa * i64::from(network.qb());
    let eval_cp = round_div(numerator, denominator);
    eval_cp.clamp(EVAL_CP_MIN, EVAL_CP_MAX) as i32
}

/// One hidden affine layer: for each output neuron, seed the i32 bias, add the int8
/// dot over the input, requantize with the layer's scale, and activate back into
/// `[0, QA]` for the next layer. `output` is `out_dim` long and `input` `in_dim`.
#[inline]
#[allow(clippy::too_many_arguments)]
fn affine_activate(
    input: &[i16],
    layer: &StackLayer,
    in_dim: usize,
    scale: i64,
    activation: Activation,
    qa: i64,
    output: &mut [i16],
    dot: &impl Fn(&[i16], &[i8]) -> i32,
) {
    for (o, dst) in output.iter_mut().enumerate() {
        let row = &layer.w[o * in_dim..o * in_dim + in_dim];
        let acc = i64::from(layer.b[o]) + i64::from(dot(input, row));
        // `round_div(acc, scale) = QA · out_float`; clamping and activating lands
        // it back in the `[0, QA]` domain the next layer's input occupies.
        let t = round_div(acc, scale);
        *dst = activate_qa_domain(activation, t, qa);
    }
}

/// Activates a pre-activation value `t` (already in the `QA`-scaled domain) into the
/// `[0, QA]` activation domain, applying the network's activation: CReLU is the
/// clip alone; SCReLU squares the clipped value and divides by `QA` rounding half
/// away from zero. The result is in `[0, QA]` and, for the `QA ≤ i16::MAX` the
/// contract's i16 activation domain assumes, fits i16. This is the same arithmetic
/// as [`screlu_activation`] applied to a clipped value.
#[inline]
fn activate_qa_domain(activation: Activation, t: i64, qa: i64) -> i16 {
    let c = t.clamp(0, qa);
    match activation {
        Activation::ClippedRelu => c as i16,
        Activation::SquaredClippedRelu => round_div(c * c, qa) as i16,
    }
}

/// The int8 dot product of a stack layer row, dispatched to the widest path this
/// CPU supports. Every path returns the identical i32 the scalar [`dot_i8`] would.
#[inline]
fn dot_i8_selected(activations: &[i16], weights: &[i8]) -> i32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            // SAFETY: `dot_i8_avx2` requires AVX2, which the runtime check on this
            // line has just confirmed. Its slices are equal-length and a multiple
            // of 16 (every stack layer's input dimension is), read in bounds below.
            return unsafe { dot_i8_avx2(activations, weights) };
        }
    }
    dot_i8(activations, weights)
}

/// The scalar int8 dot product of one stack-layer row: `Σ_i act[i] · W[i]`, summed
/// in i32. `act` is an already-activated value in `[0, QA]`; `W` is an int8 weight.
/// This is the normative reference the AVX2 kernel reproduces exactly.
#[inline]
fn dot_i8(activations: &[i16], weights: &[i8]) -> i32 {
    activations
        .iter()
        .zip(weights)
        .map(|(&a, &w)| i32::from(a) * i32::from(w))
        .sum()
}

/// AVX2 implementation of [`dot_i8`], computing the bit-identical i32 sum sixteen
/// elements at a time.
///
/// The int8 weights are sign-extended to i16 and multiplied against the i16
/// activations with `_mm256_madd_epi16` (i16×i16 → i32 pairwise, non-saturating),
/// the same instruction the version-1 clipped dot uses. Because every product and
/// pair-sum lands in i32 with no saturation, the result equals the scalar loop's
/// exactly rather than approximately — the reason the tail uses this widened kernel
/// rather than the saturating `vpmaddubsw`. Every stack layer's input dimension is
/// a multiple of 16, so the whole row is processed by full 256-bit loads.
///
/// # Safety
///
/// The caller must ensure AVX2 is available. `activations` and `weights` must have
/// equal length and that length must be a multiple of 16.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dot_i8_avx2(activations: &[i16], weights: &[i8]) -> i32 {
    use std::arch::x86_64::{
        __m128i, __m256i, _mm256_add_epi32, _mm256_castsi256_si128, _mm256_cvtepi8_epi16,
        _mm256_extracti128_si256, _mm256_loadu_si256, _mm256_madd_epi16, _mm256_setzero_si256,
        _mm_add_epi32, _mm_cvtsi128_si32, _mm_loadu_si128, _mm_shuffle_epi32, _mm_unpackhi_epi64,
    };

    debug_assert_eq!(
        activations.len(),
        weights.len(),
        "int8 dot product needs equal-length inputs"
    );
    debug_assert_eq!(
        activations.len() % 16,
        0,
        "every stack layer input dimension is a multiple of 16"
    );

    let mut acc = _mm256_setzero_si256();
    let len = activations.len();
    let mut offset = 0;
    while offset < len {
        // SAFETY: `offset` steps by 16 and stops at `len`, so the i16 load reads a
        // full 16-lane vector and the i8 load a full 16-byte half, both wholly
        // inside the equal-length slices. The loads are unaligned.
        let a = _mm256_loadu_si256(activations.as_ptr().add(offset) as *const __m256i);
        let w8 = _mm_loadu_si128(weights.as_ptr().add(offset) as *const __m128i);
        // Sign-extend the sixteen i8 weights to i16, then multiply-add against the
        // i16 activations, accumulating the pairwise products in i32 lanes.
        let w16 = _mm256_cvtepi8_epi16(w8);
        acc = _mm256_add_epi32(acc, _mm256_madd_epi16(a, w16));
        offset += 16;
    }

    // Horizontal sum of the eight i32 lanes, exactly as the version-1 kernel.
    let lo = _mm256_castsi256_si128(acc);
    let hi = _mm256_extracti128_si256::<1>(acc);
    let sum128 = _mm_add_epi32(lo, hi);
    let sum64 = _mm_add_epi32(sum128, _mm_unpackhi_epi64(sum128, sum128));
    let sum32 = _mm_add_epi32(sum64, _mm_shuffle_epi32::<0b01>(sum64));
    _mm_cvtsi128_si32(sum32)
}

/// The clipped dot product of one perspective block, dispatched to the widest
/// path this CPU supports and falling back to the scalar reference.
///
/// The AVX2 kernel is selected by runtime feature detection, not by the build's
/// baseline, so one binary runs the wide path on a CPU that has AVX2 and the
/// portable path on one that does not. On a non-x86-64 target only the scalar
/// path exists. Every path returns the identical i32 the scalar [`dot_clipped`]
/// would, so which one runs is invisible to the score.
#[inline]
fn dot_clipped_selected(activations: &[i16], weights: &[i16], qa: i32) -> i32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            // SAFETY: `dot_clipped_avx2` requires the AVX2 target feature, which
            // the runtime check on this line has just confirmed is present. Its
            // pointer arguments are the two equal-length input slices, read
            // in-bounds below.
            return unsafe { dot_clipped_avx2(activations, weights, qa) };
        }
    }
    dot_clipped(activations, weights, qa)
}

/// The clipped-ReLU-weighted dot product of one perspective block: for each unit,
/// clamp the activation to `[0, QA]` and multiply by its output weight, summing in
/// i32.
///
/// This is the normative reference the AVX2 kernel reproduces exactly.
#[inline]
fn dot_clipped(activations: &[i16], weights: &[i16], qa: i32) -> i32 {
    activations
        .iter()
        .zip(weights)
        .map(|(&a, &w)| i32::from(a).clamp(0, qa) * i32::from(w))
        .sum()
}

/// The squared-clipped-ReLU activation of one accumulator entry: clamp to
/// `[0, QA]`, square, and divide by `QA` rounding half away from zero.
///
/// With `c = clamp(x, 0, QA)` the result is `round(c²/QA)`, which is at most
/// `c ≤ QA ≤ i16::MAX`, so it lands back in the `[0, QA]` i16 activation domain the
/// output layer expects — the same domain a CReLU activation occupies. Keeping the
/// activated value in `[0, QA]` is what lets the SCReLU path reuse the CReLU output
/// kernels unchanged: their own clip to `[0, QA]` is a no-op on this value.
#[inline]
fn screlu_activation(x: i16, qa: i32) -> i16 {
    let c = i64::from(i32::from(x).clamp(0, qa));
    // `c ≤ QA`, so `round(c²/QA) ≤ c ≤ i16::MAX`; the cast cannot truncate.
    round_div(c * c, i64::from(qa)) as i16
}

/// The SCReLU-activated dot product of one perspective block: pre-activate each
/// entry with [`screlu_activation`], then run the given clipped dot over the
/// activated values and their output weights.
///
/// Pre-activation happens in a fixed stack buffer processed in `CHUNK`-sized
/// slices, so the whole forward pass allocates nothing per evaluation regardless of
/// the hidden width. `CHUNK` is a multiple of 16 and the hidden width is too, so
/// every slice handed to `dot` — including the last — keeps the AVX2 kernel's
/// 16-lane precondition. Integer addition is associative, so summing the block in
/// chunks yields the same total as summing it whole, and passing the same
/// pre-activated buffer to the scalar and AVX2 dots makes the two bit-identical for
/// an SCReLU network by construction.
#[inline]
fn dot_screlu<F: Fn(&[i16], &[i16], i32) -> i32>(
    block: &[i16],
    weights: &[i16],
    qa: i32,
    dot: &F,
) -> i32 {
    const CHUNK: usize = 256;
    let mut buf = [0i16; CHUNK];
    let mut sum: i32 = 0;
    let mut offset = 0;
    while offset < block.len() {
        let len = (block.len() - offset).min(CHUNK);
        for (dst, &x) in buf[..len].iter_mut().zip(&block[offset..offset + len]) {
            *dst = screlu_activation(x, qa);
        }
        sum += dot(&buf[..len], &weights[offset..offset + len], qa);
        offset += len;
    }
    sum
}

/// AVX2 implementation of [`dot_clipped`], computing the bit-identical i32 sum
/// sixteen i16 units at a time.
///
/// The scalar reference sums `clamp(a, 0, qa) · w` over the block in i32.
/// Integer addition is associative and commutative, so as long as no partial sum
/// overflows i32 — which the contract's bound on `|s|` guarantees, activations
/// being clamped to `[0, QA]` and the output weights bounded — any summation
/// order yields the same total. This kernel therefore clips and multiplies in
/// vector lanes and reduces at the end, and the result equals the scalar loop's
/// exactly rather than approximately.
///
/// The clip's upper bound is `min(qa, i16::MAX)`: activations come from the i16
/// accumulator, so `a ≤ i16::MAX`, and when `qa` exceeds `i16::MAX` the upper
/// clamp can never bind — capping the vector bound at `i16::MAX` makes it
/// representable as an i16 lane while leaving `clamp(a, 0, qa)` unchanged for
/// every reachable `a`. `_mm256_madd_epi16` multiplies signed i16 lanes and
/// horizontally adds adjacent pairs into i32; the clipped activations are
/// non-negative, matching the scalar `i32::from(a).clamp(0, qa)`.
///
/// The block length is a multiple of 16 (the hidden width invariant), so the
/// whole block is processed by full 256-bit loads with no scalar remainder.
///
/// # Safety
///
/// The caller must ensure the AVX2 target feature is available on the running
/// CPU. `activations` and `weights` must have equal length and that length must
/// be a multiple of 16.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dot_clipped_avx2(activations: &[i16], weights: &[i16], qa: i32) -> i32 {
    use std::arch::x86_64::{
        __m256i, _mm256_add_epi32, _mm256_castsi256_si128, _mm256_extracti128_si256,
        _mm256_loadu_si256, _mm256_madd_epi16, _mm256_max_epi16, _mm256_min_epi16,
        _mm256_set1_epi16, _mm256_setzero_si256, _mm_add_epi32, _mm_cvtsi128_si32,
        _mm_shuffle_epi32, _mm_unpackhi_epi64,
    };

    debug_assert_eq!(
        activations.len(),
        weights.len(),
        "clipped dot product needs equal-length inputs"
    );
    debug_assert_eq!(
        activations.len() % 16,
        0,
        "hidden width is a multiple of 16, so the block has no i16 remainder"
    );

    let zero = _mm256_setzero_si256();
    // `qa` fits an i16 lane after capping at `i16::MAX`; see the doc comment for
    // why this leaves the clip unchanged for every reachable activation.
    let qa_cap = _mm256_set1_epi16(qa.min(i32::from(i16::MAX)) as i16);

    let mut acc = _mm256_setzero_si256();
    let len = activations.len();
    let mut offset = 0;
    while offset < len {
        // SAFETY: `offset` steps by 16 and stops at `len`, so both loads read a
        // full 16-lane vector wholly inside the equal-length slices. The loads
        // are unaligned; the slices carry no alignment guarantee.
        let a = _mm256_loadu_si256(activations.as_ptr().add(offset) as *const __m256i);
        let w = _mm256_loadu_si256(weights.as_ptr().add(offset) as *const __m256i);
        // Clipped ReLU into [0, qa], then multiply by the weights and accumulate
        // the pairwise products in i32 lanes.
        let clipped = _mm256_min_epi16(_mm256_max_epi16(a, zero), qa_cap);
        acc = _mm256_add_epi32(acc, _mm256_madd_epi16(clipped, w));
        offset += 16;
    }

    // Horizontal sum of the eight i32 lanes: fold the high 128 bits into the low,
    // then reduce the four remaining lanes to one.
    let lo = _mm256_castsi256_si128(acc);
    let hi = _mm256_extracti128_si256::<1>(acc);
    let sum128 = _mm_add_epi32(lo, hi);
    let sum64 = _mm_add_epi32(sum128, _mm_unpackhi_epi64(sum128, sum128));
    let sum32 = _mm_add_epi32(sum64, _mm_shuffle_epi32::<0b01>(sum64));
    _mm_cvtsi128_si32(sum32)
}

/// Divides `numerator` by a positive `denominator`, rounding a half away from
/// zero, in i64.
///
/// This is the exact dequantization rounding the contract fixes so the scalar,
/// SIMD, and reference generators all agree bit for bit. Rounding away from zero
/// (rather than towards it or to even) keeps the mapping symmetric about zero, so
/// a position and its colour-flipped mirror round to equal and opposite scores.
#[inline]
fn round_div(numerator: i64, denominator: i64) -> i64 {
    debug_assert!(denominator > 0, "denominator must be positive");
    let half = denominator / 2;
    if numerator >= 0 {
        (numerator + half) / denominator
    } else {
        -((-numerator + half) / denominator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nnue::{feature_index, Parameters, INPUT_DIM, OUTPUT_DIM};
    use chess::init::init_globals;
    use chess::position::{PieceType, Position};

    // The AVX2 differential tests need move generation to reach random positions
    // and an RNG to draw networks and activations. These imports and the tests
    // that use them exist only on x86-64, where the AVX2 kernel is compiled; on
    // any other target the kernel does not exist and there is nothing to compare.
    #[cfg(target_arch = "x86_64")]
    use chess::mono_traits::{All, Legal};
    #[cfg(target_arch = "x86_64")]
    use chess::movelist::BasicMoveList;
    #[cfg(target_arch = "x86_64")]
    use rand::{rngs::SmallRng, RngExt, SeedableRng};

    const QA: u16 = 255;
    const QB: u16 = 64;
    const SCALE: i32 = 400;

    /// The six real piece types in a fixed order, for scanning a board.
    const PIECE_TYPES: [PieceType; 6] = [
        PieceType::Pawn,
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ];

    /// Builds a deterministic CReLU test network with the given hidden width and
    /// blocks, at the default scales.
    fn network(
        hidden: u32,
        w_ft: Vec<i16>,
        b_ft: Vec<i16>,
        w_out: Vec<i16>,
        b_out: i32,
    ) -> Network {
        network_with(Activation::ClippedRelu, hidden, w_ft, b_ft, w_out, b_out)
    }

    /// Builds a deterministic test network with a chosen activation, so the SCReLU
    /// path can be exercised over the same weight patterns the CReLU tests use.
    fn network_with(
        activation: Activation,
        hidden: u32,
        w_ft: Vec<i16>,
        b_ft: Vec<i16>,
        w_out: Vec<i16>,
        b_out: i32,
    ) -> Network {
        Network::new(
            hidden,
            activation,
            QA,
            QB,
            SCALE,
            Parameters {
                w_ft,
                b_ft,
                w_out,
                b_out: vec![b_out],
            },
        )
        .expect("test network parameters satisfy the build invariant")
    }

    /// A network whose weights vary by feature and unit so different columns are
    /// distinguishable, with magnitudes chosen to give a wide but in-band score
    /// spread. First-layer weights span `[-20, 20]` and the bias `[-3, 3]`, so a
    /// 32-piece board keeps every accumulator entry inside i16 (`3 + 32·20 = 643`)
    /// while pushing many entries past `QA` to exercise the clip; output weights
    /// span `[-24, 24]` so scores range across hundreds of centipawns rather than
    /// clustering, making the golden vectors discriminating.
    fn patterned_network(hidden: u32) -> Network {
        patterned_network_with(Activation::ClippedRelu, hidden)
    }

    /// [`patterned_network`] with a chosen activation, so the same discriminating
    /// weight pattern can drive either the CReLU or the SCReLU path.
    fn patterned_network_with(activation: Activation, hidden: u32) -> Network {
        let h = hidden as usize;
        let mut w_ft = vec![0i16; INPUT_DIM as usize * h];
        for (feature, column) in w_ft.chunks_mut(h).enumerate() {
            for (unit, w) in column.iter_mut().enumerate() {
                *w = ((feature * 31 + unit * 7) % 41) as i16 - 20;
            }
        }
        let b_ft: Vec<i16> = (0..h).map(|unit| (unit as i16 % 7) - 3).collect();
        let w_out: Vec<i16> = (0..2 * h).map(|j| ((j * 13) % 49) as i16 - 24).collect();
        network_with(activation, hidden, w_ft, b_ft, w_out, 0)
    }

    /// An independent, dense reference forward pass, written to share no code with
    /// [`forward`]: it materializes the full 768-input vector per perspective and
    /// multiplies by the feature transformer densely (rather than summing the
    /// sparse active columns the [`Accumulator`] maintains), then runs the output
    /// layer in plain scalar loops. Agreement between the two therefore exercises
    /// two different index derivations and two accumulation structures, so a bug in
    /// one is unlikely to be mirrored in the other.
    fn reference_forward(net: &Network, pos: &Position, stm: Player) -> i32 {
        let h = net.hidden_width() as usize;
        let w_ft = net.feature_transformer_weights();
        let b_ft = net.feature_transformer_bias();

        // Dense per-perspective accumulators from the bias plus every piece's column.
        let mut acc = [vec![0i64; h], vec![0i64; h]];
        for (slot, &perspective) in [Player::WHITE, Player::BLACK].iter().enumerate() {
            for (unit, a) in acc[slot].iter_mut().enumerate() {
                *a = i64::from(b_ft[unit]);
            }
            for &colour in &[Player::WHITE, Player::BLACK] {
                for &piece_type in &PIECE_TYPES {
                    let piece = chess::position::Piece::make(colour, piece_type);
                    for sq in pos.piece_bb(colour, piece_type) {
                        let f = feature_index(perspective, piece, sq);
                        for (unit, a) in acc[slot].iter_mut().enumerate() {
                            *a += i64::from(w_ft[f * h + unit]);
                        }
                    }
                }
            }
        }

        let own = if stm.is_white() { &acc[0] } else { &acc[1] };
        let enemy = if stm.is_white() { &acc[1] } else { &acc[0] };
        let w_out = net.output_weights();
        let qa = i64::from(net.qa());

        // The activation, applied independently of `forward`'s implementation so the
        // two derive the SCReLU value by different code. `x` is non-negative after
        // the clamp, so the rounded divide is `(c² + QA/2) / QA`.
        let activate = |x: i64| -> i64 {
            let c = x.clamp(0, qa);
            match net.activation() {
                Activation::ClippedRelu => c,
                Activation::SquaredClippedRelu => (c * c + qa / 2) / qa,
            }
        };

        let mut s = i64::from(net.output_bias()[0]);
        for (j, &a) in own.iter().enumerate() {
            s += activate(a) * i64::from(w_out[j]);
        }
        for (j, &a) in enemy.iter().enumerate() {
            s += activate(a) * i64::from(w_out[h + j]);
        }

        let scale = i64::from(net.scale());
        let den = i64::from(net.qa()) * i64::from(net.qb());
        let num = s * scale;
        let half = den / 2;
        let cp = if num >= 0 {
            (num + half) / den
        } else {
            -((-num + half) / den)
        };
        cp.clamp(-10_000, 10_000) as i32
    }

    /// (FEN, expected centipawns) golden vectors for [`patterned_network(16)`],
    /// evaluated from the side to move. The expected integers are fixed here as
    /// the golden reference; the harness that loads and checks them is what
    /// TASK-69.10 reuses to check the SIMD and PyTorch paths, replacing this
    /// hand-seeded network and its constants with vectors a trainer emits.
    ///
    /// Each value was computed by the dense [`reference_forward`], which the same
    /// test cross-checks against [`forward`] independently.
    const GOLDEN_H16: &[(&str, i32)] = &[
        ("4k3/8/8/8/8/8/8/4K3 w - - 0 1", -19),
        ("4k3/8/8/8/8/8/8/4K3 b - - 0 1", -19),
        (
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            -61,
        ),
        (
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            40,
        ),
        ("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", 5),
        (
            "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 1",
            72,
        ),
    ];

    /// Loads the golden (FEN, expected-score) pairs and asserts the scalar forward
    /// pass reproduces each expected integer exactly, and that the independent
    /// dense reference agrees on the same value. This is the golden-vector harness:
    /// exact integer equality against fixed expected scores for a known network.
    #[test]
    fn golden_vectors_match_the_scalar_forward_pass_exactly() {
        init_globals();
        let net = patterned_network(16);

        for &(fen, expected) in GOLDEN_H16 {
            let pos = Position::from_fen(fen).expect("golden FEN is valid");
            let stm = pos.turn();
            let acc = Accumulator::from_position(&net, &pos);

            let got = forward(&net, &acc, stm, pos.occupied().popcnt());
            assert_eq!(got, expected, "forward pass mismatch on {fen}");

            let reference = reference_forward(&net, &pos, stm);
            assert_eq!(
                reference, expected,
                "independent dense reference mismatch on {fen}"
            );
        }
    }

    /// The exporter-emitted golden-vector fixture: the quantized network the engine
    /// loads (`GOLDEN_NET_BYTES`) and the `(category, FEN, expected-centipawn)`
    /// triples the exporter's own integer forward pass produced for it
    /// (`GOLDEN_VECTORS`). Both are committed so the cross-language agreement is
    /// checked in every `cargo test` run without invoking Python. Regenerate them
    /// together with `python export.py --emit-golden engine/tests/fixtures`.
    const GOLDEN_NET_BYTES: &[u8] = include_bytes!("../../tests/fixtures/golden_v1.sbnn");
    const GOLDEN_VECTORS: &str = include_str!("../../tests/fixtures/golden_v1.vectors");

    /// The SCReLU counterpart of the CReLU golden fixture above, emitted by the same
    /// exporter (`python export.py --emit-golden engine/tests/fixtures`) from a
    /// network whose only difference is `activation_id = 1`. Committing it makes the
    /// three-way cross-language check cover SCReLU in every `cargo test` run.
    const GOLDEN_SCRELU_NET_BYTES: &[u8] =
        include_bytes!("../../tests/fixtures/golden_screlu_v1.sbnn");
    const GOLDEN_SCRELU_VECTORS: &str =
        include_str!("../../tests/fixtures/golden_screlu_v1.vectors");

    /// The four position kinds the golden set must span. The differential test
    /// asserts each is present so a regenerated fixture cannot silently drop one.
    const GOLDEN_CATEGORIES: [&str; 4] = ["tactical", "endgame", "king-safety", "near-overflow"];

    /// Parses a committed vectors file into `(category, FEN, expected)` triples,
    /// skipping the `#` comment header. Each line is three tab-separated fields; a
    /// FEN contains spaces but no tab, so the split is unambiguous.
    fn parse_golden_vectors(text: &'static str) -> Vec<(&'static str, &'static str, i32)> {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                let mut fields = line.split('\t');
                let category = fields.next().expect("golden line has a category");
                let fen = fields.next().expect("golden line has a FEN");
                let expected = fields
                    .next()
                    .expect("golden line has an expected score")
                    .parse::<i32>()
                    .expect("golden expected score is an integer");
                assert!(
                    fields.next().is_none(),
                    "golden line has exactly three tab-separated fields"
                );
                (category, fen, expected)
            })
            .collect()
    }

    /// The cross-language sync guarantee as a differential test, run over one
    /// committed fixture: for every golden position the score the Python exporter
    /// emitted, the Rust scalar forward pass, and — on a CPU with AVX2 — the Rust
    /// SIMD forward pass are the identical integer. The expected values were produced
    /// by the exporter's integer forward pass over the same committed network, so
    /// equality here is exact agreement across the language boundary — on the feature
    /// encoding, the quantized activation and output arithmetic, and the rounded
    /// read-out — over tactical, endgame, king-safety, and near-overflow positions.
    /// The scalar and AVX2 kernels are driven explicitly through the shared forward
    /// tail, so where the instructions exist the third check is a real one rather than
    /// the same runtime dispatch counted twice. The network's own header selects the
    /// activation, so passing the SCReLU fixture exercises the squared activation on
    /// all three paths.
    fn assert_golden_three_way(net_bytes: &[u8], vectors_text: &'static str) {
        init_globals();
        let net = Network::read(&mut &net_bytes[..]).expect("the exporter's golden network loads");
        let vectors = parse_golden_vectors(vectors_text);
        assert!(!vectors.is_empty(), "the golden fixture has vectors");
        for category in GOLDEN_CATEGORIES {
            assert!(
                vectors.iter().any(|&(c, _, _)| c == category),
                "golden set covers the {category} category"
            );
        }

        for (category, fen, expected) in vectors {
            let pos = Position::from_fen(fen).expect("golden FEN is valid");
            let stm = pos.turn();
            let acc = Accumulator::from_position(&net, &pos);

            // The Rust scalar forward pass reproduces the exporter's emitted integer.
            let scalar = forward_with(&net, &acc, stm, dot_clipped);
            assert_eq!(
                scalar, expected,
                "scalar forward vs exporter on {category} {fen}"
            );

            // Where AVX2 exists, the SIMD forward pass is included: a three-way check.
            #[cfg(target_arch = "x86_64")]
            {
                if is_x86_feature_detected!("avx2") {
                    // SAFETY: AVX2 presence was just confirmed; each perspective
                    // block and its weight slice are equal-length H (a multiple of
                    // 16), read in bounds by the kernel.
                    let simd = forward_with(&net, &acc, stm, |a, w, q| unsafe {
                        dot_clipped_avx2(a, w, q)
                    });
                    assert_eq!(
                        simd, expected,
                        "SIMD forward vs exporter on {category} {fen}"
                    );
                }
            }
        }
    }

    /// The CReLU cross-language differential check over the committed CReLU fixture.
    #[test]
    fn golden_vectors_agree_across_python_scalar_and_simd() {
        assert_golden_three_way(GOLDEN_NET_BYTES, GOLDEN_VECTORS);
    }

    /// The SCReLU cross-language differential check over the committed SCReLU fixture:
    /// the same three-way guarantee for a network whose header selects the squared
    /// activation, so the Python integer forward, the Rust scalar pre-activation, and
    /// the Rust AVX2 path all agree bit for bit on squared-clipped-ReLU semantics.
    #[test]
    fn screlu_golden_vectors_agree_across_python_scalar_and_simd() {
        assert_golden_three_way(GOLDEN_SCRELU_NET_BYTES, GOLDEN_SCRELU_VECTORS);
    }

    /// The scalar forward pass agrees with the independent dense reference across a
    /// range of positions and two hidden widths, so the sparse accumulator-based
    /// path and a dense from-the-board computation compute the same score.
    #[test]
    fn forward_agrees_with_the_dense_reference_over_many_positions() {
        init_globals();
        let fens = [
            "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R b KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 b - - 0 1",
            "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
            "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 b - - 0 1",
        ];
        for hidden in [16u32, 32] {
            let net = patterned_network(hidden);
            for fen in fens {
                let pos = Position::from_fen(fen).expect("test FEN is valid");
                let stm = pos.turn();
                let acc = Accumulator::from_position(&net, &pos);
                assert_eq!(
                    forward(&net, &acc, stm, pos.occupied().popcnt()),
                    reference_forward(&net, &pos, stm),
                    "scalar and dense reference disagree on {fen} at H={hidden}"
                );
            }
        }
    }

    /// The colour-and-rank mirror of a FEN with the side to move swapped: every piece changes
    /// colour, the board flips vertically, and the mover becomes the other side. This is the same
    /// game seen from the opposite side, so the perspective-doubled network must score it the same
    /// from the mover.
    fn colour_mirror(fen: &str) -> String {
        let mut parts = fen.split(' ');
        let board = parts.next().unwrap();
        let stm = parts.next().unwrap_or("w");
        let mirrored = board
            .split('/')
            .rev()
            .map(|rank| {
                rank.chars()
                    .map(|c| {
                        if c.is_ascii_uppercase() {
                            c.to_ascii_lowercase()
                        } else if c.is_ascii_lowercase() {
                            c.to_ascii_uppercase()
                        } else {
                            c
                        }
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("/");
        let swapped = if stm == "w" { "b" } else { "w" };
        format!("{mirrored} {swapped} - - 0 1")
    }

    /// A position and its colour-and-rank mirror (side to move swapped) evaluate to exactly the same
    /// score from the mover, because each presents the identical board to the side to move. This is
    /// the network's perspective symmetry: a feature index that mis-oriented the board for one
    /// perspective, or an accumulator that swapped the two perspectives, would break it as exact
    /// integer inequality. (The score is stm-relative, so mirror scores are equal, not negated;
    /// negation appears only after the White-relative `pov` flip the hand-crafted path applies.)
    #[test]
    fn mirrored_positions_score_identically_from_the_mover() {
        init_globals();
        let net = patterned_network(16);
        for fen in [
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 b - - 0 1",
            "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        ] {
            let pos = Position::from_fen(fen).expect("test FEN is valid");
            let mir = Position::from_fen(&colour_mirror(fen)).expect("mirror FEN is valid");
            let s = forward(&net, &Accumulator::from_position(&net, &pos), pos.turn(), pos.occupied().popcnt());
            let m = forward(&net, &Accumulator::from_position(&net, &mir), mir.turn(), mir.occupied().popcnt());
            assert_eq!(s, m, "mirror of {fen} did not match from the mover");
        }
    }

    /// `round_div` rounds a half away from zero and truncates otherwise, symmetric
    /// across the sign of the numerator. The half-away rule is what keeps mirrored
    /// scores exactly opposite.
    #[test]
    fn round_div_rounds_half_away_from_zero() {
        // Denominator 10: halves land on x.5.
        assert_eq!(round_div(25, 10), 3); // 2.5 -> 3
        assert_eq!(round_div(-25, 10), -3); // -2.5 -> -3
        assert_eq!(round_div(24, 10), 2); // 2.4 -> 2
        assert_eq!(round_div(-24, 10), -2); // -2.4 -> -2
        assert_eq!(round_div(26, 10), 3); // 2.6 -> 3
        assert_eq!(round_div(-26, 10), -3);
        assert_eq!(round_div(0, 10), 0);
        assert_eq!(round_div(5, 10), 1); // 0.5 -> 1
        assert_eq!(round_div(-5, 10), -1);
        // Odd denominator: the half is floored, so only an exact half rounds up.
        assert_eq!(round_div(7, 3), 2); // 2.33 -> 2
        assert_eq!(round_div(8, 3), 3); // 2.66 -> 3
        assert_eq!(round_div(-8, 3), -3);
    }

    /// The squared-clipped-ReLU activation clips to `[0, QA]`, squares, and divides
    /// by `QA` rounding half away from zero, always landing back in `[0, QA]`.
    #[test]
    fn screlu_activation_clips_squares_and_rounds() {
        let qa = 255;
        // Below the clip: negatives and zero activate to zero.
        assert_eq!(screlu_activation(-5, qa), 0);
        assert_eq!(screlu_activation(0, qa), 0);
        // At and above the clip: c saturates at QA, so a = round(QA²/QA) = QA.
        assert_eq!(screlu_activation(255, qa), 255);
        assert_eq!(screlu_activation(300, qa), 255);
        assert_eq!(screlu_activation(i16::MAX, qa), 255);
        // Inside the band: a = round(c²/QA), half away from zero.
        assert_eq!(screlu_activation(100, qa), 39); // round(10000/255) = round(39.22)
        assert_eq!(screlu_activation(128, qa), 64); // round(16384/255) = round(64.25)
        assert_eq!(screlu_activation(12, qa), 1); // round(144/255)  = round(0.56)
        assert_eq!(screlu_activation(11, qa), 0); // round(121/255)  = round(0.47)

        // A QA above i16::MAX: c is capped by the i16 input, and round(c²/QA) still
        // fits i16 (c²/QA < c ≤ i16::MAX), so the i16 cast never truncates.
        let big_qa = 40_000;
        let expected = ((i64::from(i16::MAX)).pow(2) + i64::from(big_qa) / 2) / i64::from(big_qa);
        assert_eq!(screlu_activation(i16::MAX, big_qa), expected as i16);
        assert!(expected <= i64::from(i16::MAX));
    }

    /// The scalar SCReLU forward pass reproduces the independent dense reference over
    /// a range of positions and widths. This runs on every target — it does not need
    /// AVX2 — so it guards the squared-activation arithmetic (clip, square, rounded
    /// divide, then the shared output layer) against a from-the-board computation that
    /// shares no code with `forward`.
    #[test]
    fn screlu_forward_agrees_with_the_dense_reference() {
        init_globals();
        let fens = [
            "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R b KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 b - - 0 1",
            "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 1",
        ];
        // Widths on both sides of the pre-activation chunk (256) so the chunked and
        // whole-block sums are both exercised.
        for hidden in [16u32, 256, 272] {
            let net = patterned_network_with(Activation::SquaredClippedRelu, hidden);
            for fen in fens {
                let pos = Position::from_fen(fen).expect("test FEN is valid");
                let stm = pos.turn();
                let acc = Accumulator::from_position(&net, &pos);
                assert_eq!(
                    forward(&net, &acc, stm, pos.occupied().popcnt()),
                    reference_forward(&net, &pos, stm),
                    "SCReLU forward vs dense reference on {fen} at H={hidden}"
                );
            }
        }
    }

    /// Constructs an accumulator whose every entry is a chosen constant by setting
    /// the feature-transformer bias to it and giving every feature a zero weight
    /// column, so the pieces on the board leave the seeded bias unchanged.
    fn constant_accumulator_network(
        hidden: u32,
        entry: i16,
        w_out_value: i16,
        b_out: i32,
    ) -> Network {
        let h = hidden as usize;
        network(
            hidden,
            vec![0i16; INPUT_DIM as usize * h],
            vec![entry; h],
            vec![w_out_value; 2 * h],
            b_out,
        )
    }

    /// A large positive accumulator entry is clipped to `QA`, and a negative one to
    /// `0`, before it reaches the output layer. Driving the accumulator to the i16
    /// extremes exercises the clip at both ends: at `i16::MAX` every unit
    /// contributes `QA · W_out`, and at `i16::MIN` every unit contributes nothing.
    #[test]
    fn activations_saturate_at_the_clip_bounds() {
        init_globals();
        let hidden = 16u32;
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").expect("valid");
        let stm = pos.turn();

        // Every entry i16::MAX -> clipped to QA. Output weight 1, bias 0.
        let net_hi = constant_accumulator_network(hidden, i16::MAX, 1, 0);
        let acc_hi = Accumulator::from_position(&net_hi, &pos);
        // s = 2H · QA · 1 = 32 · 255 = 8160; eval = round(8160 · 400 / (255·64)).
        let s_hi = 2 * hidden as i64 * i64::from(QA);
        let expected_hi = round_div(s_hi * i64::from(SCALE), i64::from(QA) * i64::from(QB));
        assert_eq!(forward(&net_hi, &acc_hi, stm, pos.occupied().popcnt()), expected_hi as i32);

        // Every entry i16::MIN -> clipped to 0, so only the bias survives.
        let net_lo = constant_accumulator_network(hidden, i16::MIN, 1000, -12_345);
        let acc_lo = Accumulator::from_position(&net_lo, &pos);
        let expected_lo = round_div(
            i64::from(-12_345) * i64::from(SCALE),
            i64::from(QA) * i64::from(QB),
        );
        assert_eq!(forward(&net_lo, &acc_lo, stm, pos.occupied().popcnt()), expected_lo as i32);
    }

    /// With the accumulator saturated and the output weights near the top of their
    /// i16 range, the output accumulator `s` approaches `i32::MAX` and the multiply
    /// by `SCALE` exceeds it: the pass must widen to i64 before the divide rather
    /// than overflow. A wide hidden width and large weights push `s` close to the
    /// i32 ceiling so a 32-bit multiply here would wrap.
    #[test]
    fn output_accumulation_does_not_overflow_near_the_i32_ceiling() {
        init_globals();
        // H = 256 -> 2H = 512 output terms; each clipped activation is QA = 255.
        // s = 512 · 255 · w_out. Choose w_out so s is just under i32::MAX.
        let hidden = 256u32;
        let w_out_value = 15_300i16;
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").expect("valid");
        let stm = pos.turn();

        let net = constant_accumulator_network(hidden, i16::MAX, w_out_value, 0);
        let acc = Accumulator::from_position(&net, &pos);

        // Independently, in i64: s and its scaling stay exact, then clamp.
        let s = 2 * i64::from(hidden) * i64::from(QA) * i64::from(w_out_value);
        assert!(s < i64::from(i32::MAX), "test setup must keep s inside i32");
        assert!(
            s * i64::from(SCALE) > i64::from(i32::MAX),
            "test setup must force the i64 widen to matter"
        );
        let expected = round_div(s * i64::from(SCALE), i64::from(QA) * i64::from(QB))
            .clamp(-10_000, 10_000) as i32;
        assert_eq!(forward(&net, &acc, stm, pos.occupied().popcnt()), expected);
        // This saturated network exceeds the centipawn band, so the result clamps.
        assert_eq!(expected, 10_000);
    }

    /// The evaluation is clamped into the centipawn band at both ends, so a network
    /// whose raw output runs past `±10_000` still yields a score a `Score::cp` can
    /// hold rather than one that could be mistaken for a mate.
    #[test]
    fn evaluation_is_clamped_into_the_centipawn_band() {
        init_globals();
        let hidden = 32u32;
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").expect("valid");
        let stm = pos.turn();

        // Large positive raw output: clamps to +10_000.
        let net_pos = constant_accumulator_network(hidden, i16::MAX, 20_000, 0);
        let acc_pos = Accumulator::from_position(&net_pos, &pos);
        assert_eq!(forward(&net_pos, &acc_pos, stm, pos.occupied().popcnt()), 10_000);

        // Large negative raw output: clamps to -10_000.
        let net_neg = constant_accumulator_network(hidden, i16::MAX, -20_000, 0);
        let acc_neg = Accumulator::from_position(&net_neg, &pos);
        assert_eq!(forward(&net_neg, &acc_neg, stm, pos.occupied().popcnt()), -10_000);
    }

    /// `OUTPUT_DIM` is 1, so the output layer reads exactly one bias; this guards
    /// the assumption the single-scalar read-out makes.
    #[test]
    fn output_dimension_is_a_single_scalar() {
        assert_eq!(OUTPUT_DIM, 1);
    }

    /// Runs `body` only when the running CPU has AVX2, printing a skip note
    /// otherwise. The AVX2 kernel's correctness is bit-identity with the scalar
    /// oracle, which can only be observed on hardware that has the instructions;
    /// CI runs these on an AVX2 host, and an x86-64 CPU without AVX2 skips rather
    /// than silently reporting a pass it never checked.
    #[cfg(target_arch = "x86_64")]
    fn with_avx2(name: &str, body: impl FnOnce()) {
        if is_x86_feature_detected!("avx2") {
            body();
        } else {
            eprintln!("skipping {name}: AVX2 not available on this CPU");
        }
    }

    /// Builds a random network whose weight magnitudes stay within the contract's
    /// bounds, so for any reachable position the i16 accumulator and the i32
    /// output sum both stay far from overflow. Comparing the scalar and AVX2 paths
    /// is only meaningful where neither overflows — a wrap would be a defect both
    /// paths inherit differently — so the bounds here keep the comparison inside
    /// the regime the paths are defined to agree on. `|acc| ≤ 500 + 32·200 = 6900`
    /// and `|s| ≤ 2H·QA·300 + |b_out|` both sit well inside their integer types.
    #[cfg(target_arch = "x86_64")]
    fn random_contract_network(rng: &mut SmallRng, activation: Activation, hidden: u32) -> Network {
        let h = hidden as usize;
        let w_ft: Vec<i16> = (0..INPUT_DIM as usize * h)
            .map(|_| rng.random_range(-200..=200))
            .collect();
        let b_ft: Vec<i16> = (0..h).map(|_| rng.random_range(-500..=500)).collect();
        let w_out: Vec<i16> = (0..2 * h).map(|_| rng.random_range(-300..=300)).collect();
        let b_out: i32 = rng.random_range(-100_000..=100_000);
        network_with(activation, hidden, w_ft, b_ft, w_out, b_out)
    }

    /// Reaches a random legal position by walking up to `plies` random legal moves
    /// from the initial position, restarting the walk if a line ends so the result
    /// is always a real, non-terminal position rather than depending on how a
    /// checkmate or stalemate truncates.
    #[cfg(target_arch = "x86_64")]
    fn random_position(rng: &mut SmallRng, plies: usize) -> Position {
        const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        let mut pos = Position::from_fen(START).expect("start position is valid");
        for _ in 0..plies {
            let moves = pos.generate::<BasicMoveList, All, Legal>();
            if moves.is_empty() {
                pos = Position::from_fen(START).expect("start position is valid");
                continue;
            }
            let choices: Vec<&_> = (&moves).into_iter().collect();
            let mov = choices[rng.random_range(0..choices.len())];
            pos.make_move(mov);
        }
        pos
    }

    /// The AVX2 clipped dot product returns exactly the integer the scalar
    /// [`dot_clipped`] does across randomized blocks. Activations span the whole
    /// i16 range so the clip is exercised at both ends — negatives clamp to zero,
    /// values above `qa` clamp to `qa` — and `qa` includes values above `i16::MAX`
    /// so the kernel's cap at `i16::MAX` is exercised while leaving the clip
    /// unchanged. Weight magnitudes are bounded per block so the scalar `i32` sum
    /// cannot overflow, keeping the comparison inside the agreement regime.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn avx2_dot_product_is_bit_identical_to_the_scalar_oracle() {
        with_avx2(
            "avx2_dot_product_is_bit_identical_to_the_scalar_oracle",
            || {
                let mut rng = SmallRng::seed_from_u64(0x5EAB_0695);
                let qa_choices: [i32; 8] = [1, 2, 63, 64, 255, 256, 32_767, 40_000];
                for _ in 0..2_000 {
                    let hidden = 16 * rng.random_range(1..=16usize);
                    let qa = qa_choices[rng.random_range(0..qa_choices.len())];
                    // Bound the weights so no partial sum can leave i32: with the clip
                    // capping each activation at `min(qa, i16::MAX)`, the largest term
                    // is `clip · w_max`, and `hidden` of them must stay inside i32.
                    let clip = qa.min(i32::from(i16::MAX)) as i64;
                    let w_max = (1_000_000_000 / (hidden as i64 * clip)).clamp(1, 4096) as i16;

                    let activations: Vec<i16> = (0..hidden)
                        .map(|_| rng.random_range(i16::MIN..=i16::MAX))
                        .collect();
                    let weights: Vec<i16> = (0..hidden)
                        .map(|_| rng.random_range(-w_max..=w_max))
                        .collect();

                    let scalar = dot_clipped(&activations, &weights, qa);
                    // SAFETY: guarded by the AVX2 detection in `with_avx2`; the slices
                    // are equal-length and `hidden` is a multiple of 16.
                    let simd = unsafe { dot_clipped_avx2(&activations, &weights, qa) };
                    assert_eq!(
                        simd, scalar,
                        "AVX2 dot product diverged at H={hidden}, qa={qa}"
                    );
                }
            },
        );
    }

    /// The full AVX2 forward pass reproduces the scalar path and the independent
    /// dense reference bit for bit, over the golden vectors and a randomized
    /// position set. `forward` dispatches to the AVX2 kernel on this host, so its
    /// agreement with both the forced-scalar dot product and the from-the-board
    /// `reference_forward` exercises the SIMD path end to end — the accumulator,
    /// the perspective ordering, the clip, and the rounded read-out.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn avx2_forward_matches_the_scalar_path_over_golden_and_random_positions() {
        with_avx2(
            "avx2_forward_matches_the_scalar_path_over_golden_and_random_positions",
            || {
                init_globals();

                // Golden vectors: the AVX2 forward pass must land on the fixed
                // expected integers, not merely on the scalar path's output.
                let golden_net = patterned_network(16);
                for &(fen, expected) in GOLDEN_H16 {
                    let pos = Position::from_fen(fen).expect("golden FEN is valid");
                    let stm = pos.turn();
                    let acc = Accumulator::from_position(&golden_net, &pos);
                    assert_eq!(
                        forward(&golden_net, &acc, stm, pos.occupied().popcnt()),
                        expected,
                        "AVX2 golden {fen}"
                    );
                }

                // Randomized positions against randomized contract-valid networks.
                let mut rng = SmallRng::seed_from_u64(0x9E37_79B9);
                for hidden in [16u32, 32, 256] {
                    let net = random_contract_network(&mut rng, Activation::ClippedRelu, hidden);
                    for _ in 0..40 {
                        let plies = rng.random_range(1..=40);
                        let pos = random_position(&mut rng, plies);
                        let stm = pos.turn();
                        let acc = Accumulator::from_position(&net, &pos);

                        // Full forward (AVX2) equals the independent dense oracle.
                        assert_eq!(
                            forward(&net, &acc, stm, pos.occupied().popcnt()),
                            reference_forward(&net, &pos, stm),
                            "AVX2 forward vs dense reference at H={hidden}"
                        );

                        // And the kernel matches the scalar oracle on each real
                        // perspective block the forward pass reads.
                        let own = acc.perspective(stm);
                        let enemy = acc.perspective(stm.other_player());
                        let (own_w, enemy_w) = net.output_weights().split_at(hidden as usize);
                        let qa = i32::from(net.qa());
                        for (block, weights) in [(own, own_w), (enemy, enemy_w)] {
                            // SAFETY: guarded by AVX2 detection in `with_avx2`;
                            // block and weights are equal-length H (a multiple of 16).
                            let simd = unsafe { dot_clipped_avx2(block, weights, qa) };
                            assert_eq!(
                                simd,
                                dot_clipped(block, weights, qa),
                                "block kernel mismatch"
                            );
                        }
                    }
                }
            },
        );
    }

    /// For an SCReLU network the scalar and AVX2 forward passes are bit-identical,
    /// and both reproduce the independent dense reference, over randomized networks
    /// and positions. The squared activation is a shared scalar pre-step, so the
    /// forward passes differ only in the output dot kernel; this asserts the SCReLU
    /// path inherits the same scalar/SIMD agreement the CReLU path has. Widths
    /// straddle the pre-activation chunk so the chunked kernel dispatch is covered.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn screlu_scalar_and_avx2_forward_are_bit_identical() {
        with_avx2("screlu_scalar_and_avx2_forward_are_bit_identical", || {
            init_globals();
            let mut rng = SmallRng::seed_from_u64(0x5C2E_1011);
            for hidden in [16u32, 32, 256, 272] {
                let net = random_contract_network(&mut rng, Activation::SquaredClippedRelu, hidden);
                for _ in 0..40 {
                    let plies = rng.random_range(1..=40);
                    let pos = random_position(&mut rng, plies);
                    let stm = pos.turn();
                    let acc = Accumulator::from_position(&net, &pos);

                    let scalar = forward_with(&net, &acc, stm, dot_clipped);
                    // SAFETY: AVX2 presence confirmed by `with_avx2`; `forward_with`
                    // hands the kernel equal-length H (a multiple of 16) blocks.
                    let simd = forward_with(&net, &acc, stm, |a, w, q| unsafe {
                        dot_clipped_avx2(a, w, q)
                    });
                    assert_eq!(scalar, simd, "SCReLU scalar vs AVX2 at H={hidden}");
                    assert_eq!(
                        scalar,
                        reference_forward(&net, &pos, stm),
                        "SCReLU forward vs dense reference at H={hidden}"
                    );
                }
            }
        });
    }

    // --- Version-2 bucketed multi-layer int8 stack ---

    use super::super::{BucketedParameters, BucketedStack, OutputStack, StackLayer};

    const V2_DIMS: [u32; 3] = [16, 32, 1];
    // Distinct per-layer int8 weight scales (last is the network's qb) so a path
    // that ignored `stack_scales` and assumed a uniform scale would diverge.
    const V2_SCALES: [u32; 3] = [64, 128, 256];

    /// The bucketed stack of a network, or a panic if it is a single-layer one.
    fn stack_of(net: &Network) -> &BucketedStack {
        match net.output() {
            OutputStack::Bucketed(stack) => stack,
            OutputStack::Single { .. } => panic!("expected a bucketed network"),
        }
    }

    /// Round half away from zero, in i64 — an independent transcription of the
    /// contract's rounded divide, so the bucketed reference shares no arithmetic
    /// helper with the module under test.
    fn rdiv(numerator: i64, denominator: i64) -> i64 {
        let half = denominator / 2;
        if numerator >= 0 {
            (numerator + half) / denominator
        } else {
            -((-numerator + half) / denominator)
        }
    }

    /// Builds a bucketed network whose feature transformer matches
    /// [`patterned_network`] and whose per-bucket int8 stacks vary by bucket, layer,
    /// and position, at the distinct per-layer scales above. `2H → 16 → 32 → 1`.
    fn patterned_bucketed_network(
        hidden: u32,
        activation: Activation,
        num_buckets: u16,
    ) -> Network {
        let h = hidden as usize;
        let mut w_ft = vec![0i16; INPUT_DIM as usize * h];
        for (feature, column) in w_ft.chunks_mut(h).enumerate() {
            for (unit, w) in column.iter_mut().enumerate() {
                *w = ((feature * 31 + unit * 7) % 41) as i16 - 20;
            }
        }
        let b_ft: Vec<i16> = (0..h).map(|unit| (unit as i16 % 7) - 3).collect();

        let buckets = (0..num_buckets as usize)
            .map(|bucket| {
                let mut in_dim = 2 * h;
                V2_DIMS
                    .iter()
                    .map(|&out_dim| {
                        let out_dim = out_dim as usize;
                        let w: Vec<i8> = (0..out_dim * in_dim)
                            .map(|i| ((i * 13 + bucket * 17 + out_dim * 5) % 151) as i32 - 75)
                            .map(|v| v as i8)
                            .collect();
                        let b: Vec<i32> = (0..out_dim)
                            .map(|o| o as i32 * 37 + bucket as i32 * 11 - 300)
                            .collect();
                        in_dim = out_dim;
                        StackLayer { w, b }
                    })
                    .collect()
            })
            .collect();

        Network::new_bucketed(
            hidden,
            activation,
            QA,
            SCALE,
            BucketedParameters {
                w_ft,
                b_ft,
                layer_dims: V2_DIMS.to_vec(),
                layer_scales: V2_SCALES.to_vec(),
                buckets,
            },
        )
        .expect("patterned bucketed network satisfies the build invariant")
    }

    /// An independent dense forward pass for a bucketed network, sharing no code
    /// with [`forward`]: it materializes the full 768-input dense accumulator per
    /// perspective, activates in `i64`, selects the bucket, and runs the stack in
    /// plain nested loops with its own rounded divide. Agreement therefore exercises
    /// two different index derivations, accumulation widths, and layer loops.
    fn reference_bucketed_forward(net: &Network, pos: &Position, stm: Player) -> i32 {
        let h = net.hidden_width() as usize;
        let w_ft = net.feature_transformer_weights();
        let b_ft = net.feature_transformer_bias();
        let qa = i64::from(net.qa());

        let mut acc = [vec![0i64; h], vec![0i64; h]];
        for (slot, &perspective) in [Player::WHITE, Player::BLACK].iter().enumerate() {
            for (unit, a) in acc[slot].iter_mut().enumerate() {
                *a = i64::from(b_ft[unit]);
            }
            for &colour in &[Player::WHITE, Player::BLACK] {
                for &piece_type in &PIECE_TYPES {
                    let piece = chess::position::Piece::make(colour, piece_type);
                    for sq in pos.piece_bb(colour, piece_type) {
                        let f = feature_index(perspective, piece, sq);
                        for (unit, a) in acc[slot].iter_mut().enumerate() {
                            *a += i64::from(w_ft[f * h + unit]);
                        }
                    }
                }
            }
        }

        let own = if stm.is_white() { &acc[0] } else { &acc[1] };
        let enemy = if stm.is_white() { &acc[1] } else { &acc[0] };
        let activate = |x: i64| -> i64 {
            let c = x.clamp(0, qa);
            match net.activation() {
                Activation::ClippedRelu => c,
                Activation::SquaredClippedRelu => rdiv(c * c, qa),
            }
        };

        // Activated input, own perspective first.
        let mut input: Vec<i64> = own.iter().chain(enemy.iter()).map(|&x| activate(x)).collect();

        let stack = stack_of(net);
        let bucket = select_bucket(pos.occupied().popcnt(), stack.num_buckets());
        let layers = stack.bucket(bucket);
        let dims = stack.layer_dims();
        let scales = stack.layer_scales();

        let mut final_acc: i64 = 0;
        for (k, layer) in layers.iter().enumerate() {
            let out_dim = dims[k] as usize;
            let in_dim = input.len();
            let scale = i64::from(scales[k]);
            let is_last = k + 1 == layers.len();
            let mut next = vec![0i64; out_dim];
            for (o, slot) in next.iter_mut().enumerate() {
                let mut s = i64::from(layer.b[o]);
                for (i, &a) in input.iter().enumerate() {
                    s += a * i64::from(layer.w[o * in_dim + i]);
                }
                if is_last {
                    final_acc = s;
                } else {
                    *slot = activate(rdiv(s, scale));
                }
            }
            if !is_last {
                input = next;
            }
        }

        let num = final_acc * i64::from(net.scale());
        let den = qa * i64::from(net.qb());
        rdiv(num, den).clamp(-10_000, 10_000) as i32
    }

    /// The bucket rule bins piece count into `num_buckets` equal ranges over the
    /// reachable `1..=32`, clamps at `B-1`, and never underflows.
    #[test]
    fn select_bucket_bins_piece_count() {
        // Eight buckets: (p-1)/4.
        assert_eq!(select_bucket(2, 8), 0);
        assert_eq!(select_bucket(5, 8), 1);
        assert_eq!(select_bucket(6, 8), 1);
        assert_eq!(select_bucket(32, 8), 7);
        assert_eq!(select_bucket(1, 8), 0);
        // Never underflows below the first bucket.
        assert_eq!(select_bucket(0, 8), 0);
        // A single bucket always selects itself.
        assert_eq!(select_bucket(20, 1), 0);
        // Four buckets: (p-1)/8.
        assert_eq!(select_bucket(9, 4), 1);
        assert_eq!(select_bucket(32, 4), 3);
    }

    /// The scalar bucketed forward pass reproduces the independent dense reference
    /// across positions spanning multiple buckets and both activations, at two
    /// hidden widths — so the sparse accumulator path, the bucket selection, the
    /// int8 layers, the inter-layer requantize, and the final dequantize all match a
    /// from-the-board computation that shares no code with `forward`.
    #[test]
    fn bucketed_forward_agrees_with_the_dense_reference() {
        init_globals();
        // FENs chosen to span several piece counts, hence several buckets.
        let fens = [
            "4k3/8/8/8/8/8/8/4K3 w - - 0 1",             // 2 pieces  -> bucket 0
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", // 8 pieces  -> bucket 1
            "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 b - - 0 1", // busy middlegame
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", // 32 -> bucket 7
        ];
        for activation in [Activation::ClippedRelu, Activation::SquaredClippedRelu] {
            for hidden in [16u32, 256] {
                let net = patterned_bucketed_network(hidden, activation, 8);
                let mut buckets_seen = std::collections::HashSet::new();
                for fen in fens {
                    let pos = Position::from_fen(fen).expect("test FEN is valid");
                    let stm = pos.turn();
                    let acc = Accumulator::from_position(&net, &pos);
                    let pc = pos.occupied().popcnt();
                    buckets_seen.insert(select_bucket(pc, 8));
                    assert_eq!(
                        forward(&net, &acc, stm, pc),
                        reference_bucketed_forward(&net, &pos, stm),
                        "bucketed forward vs dense reference on {fen} at H={hidden}"
                    );
                }
                assert!(
                    buckets_seen.len() >= 2,
                    "the position set must span at least two buckets"
                );
            }
        }
    }

    /// The selected bucket actually drives the evaluation: scoring one fixed
    /// accumulator under piece counts that map to different buckets reads different
    /// per-bucket stacks and so produces more than one distinct score.
    #[test]
    fn bucket_selection_uses_the_selected_bucket_stack() {
        init_globals();
        let net = patterned_bucketed_network(16, Activation::ClippedRelu, 8);
        let pos = Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
            .expect("valid");
        let stm = pos.turn();
        let acc = Accumulator::from_position(&net, &pos);

        // A piece count landing squarely in each bucket 0..8.
        let scores: Vec<i32> = (0..8u32)
            .map(|b| forward(&net, &acc, stm, (4 * b + 2).min(32)))
            .collect();
        let distinct: std::collections::HashSet<_> = scores.iter().collect();
        assert!(
            distinct.len() > 1,
            "distinct buckets should yield distinct scores, got {scores:?}"
        );
    }

    /// A random bucketed network with int8 stack weights and i32 biases whose FT
    /// stays inside i16 for any reachable position, so scalar/SIMD comparison stays
    /// in the regime the paths are defined to agree on.
    #[cfg(target_arch = "x86_64")]
    fn random_bucketed_network(
        rng: &mut SmallRng,
        activation: Activation,
        hidden: u32,
        num_buckets: u16,
    ) -> Network {
        let h = hidden as usize;
        let w_ft: Vec<i16> = (0..INPUT_DIM as usize * h)
            .map(|_| rng.random_range(-200..=200))
            .collect();
        let b_ft: Vec<i16> = (0..h).map(|_| rng.random_range(-500..=500)).collect();
        let buckets = (0..num_buckets as usize)
            .map(|_| {
                let mut in_dim = 2 * h;
                V2_DIMS
                    .iter()
                    .map(|&out_dim| {
                        let out_dim = out_dim as usize;
                        let w: Vec<i8> = (0..out_dim * in_dim)
                            .map(|_| rng.random_range(-127..=127i16) as i8)
                            .collect();
                        let b: Vec<i32> = (0..out_dim)
                            .map(|_| rng.random_range(-1_000_000..=1_000_000))
                            .collect();
                        in_dim = out_dim;
                        StackLayer { w, b }
                    })
                    .collect()
            })
            .collect();
        Network::new_bucketed(
            hidden,
            activation,
            QA,
            SCALE,
            BucketedParameters {
                w_ft,
                b_ft,
                layer_dims: V2_DIMS.to_vec(),
                layer_scales: V2_SCALES.to_vec(),
                buckets,
            },
        )
        .expect("random bucketed network satisfies the build invariant")
    }

    /// For a bucketed network the scalar and AVX2 forward passes are bit-identical,
    /// and both reproduce the independent dense reference, over randomized networks
    /// and positions spanning buckets and both activations. The int8 AVX2 kernel
    /// (`vpmaddwd` over widened weights) is driven explicitly through the shared
    /// bucketed tail so the check is a real third path, not the runtime dispatch
    /// counted twice.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn bucketed_scalar_and_avx2_forward_are_bit_identical() {
        with_avx2("bucketed_scalar_and_avx2_forward_are_bit_identical", || {
            init_globals();
            let mut rng = SmallRng::seed_from_u64(0xB0CE_7A11);
            for activation in [Activation::ClippedRelu, Activation::SquaredClippedRelu] {
                for hidden in [16u32, 256] {
                    let net = random_bucketed_network(&mut rng, activation, hidden, 8);
                    let stack = stack_of(&net);
                    for _ in 0..40 {
                        let plies = rng.random_range(1..=40);
                        let pos = random_position(&mut rng, plies);
                        let stm = pos.turn();
                        let acc = Accumulator::from_position(&net, &pos);
                        let pc = pos.occupied().popcnt();

                        let scalar = forward_bucketed_with(&net, stack, &acc, stm, pc, dot_i8);
                        // SAFETY: AVX2 presence confirmed by `with_avx2`; every stack
                        // layer input is a multiple of 16, handed to the kernel whole.
                        let simd = forward_bucketed_with(&net, stack, &acc, stm, pc, |a, w| unsafe {
                            dot_i8_avx2(a, w)
                        });
                        assert_eq!(scalar, simd, "bucketed scalar vs AVX2 at H={hidden}");
                        assert_eq!(
                            scalar,
                            reference_bucketed_forward(&net, &pos, stm),
                            "bucketed forward vs dense reference at H={hidden}"
                        );
                    }
                }
            }
        });
    }

    /// The version-2 exporter's golden fixtures: a bucketed multi-layer int8 network
    /// the engine loads, and the `(category, FEN, expected-cp)` triples the exporter's
    /// own integer forward pass produced for it. Committed so the cross-language
    /// agreement is checked in every `cargo test` without invoking Python. Regenerate
    /// with `python export.py --emit-golden engine/tests/fixtures`.
    const GOLDEN_V2_NET_BYTES: &[u8] = include_bytes!("../../tests/fixtures/golden_v2.sbnn");
    const GOLDEN_V2_VECTORS: &str = include_str!("../../tests/fixtures/golden_v2.vectors");
    const GOLDEN_SCRELU_V2_NET_BYTES: &[u8] =
        include_bytes!("../../tests/fixtures/golden_screlu_v2.sbnn");
    const GOLDEN_SCRELU_V2_VECTORS: &str =
        include_str!("../../tests/fixtures/golden_screlu_v2.vectors");

    /// The three-way cross-language guarantee for a bucketed network: for every
    /// golden position the score the Python exporter emitted, the Rust scalar
    /// bucketed forward pass, and — on a CPU with AVX2 — the Rust SIMD bucketed
    /// forward pass are the identical integer. The positions span multiple buckets
    /// and the network's per-layer int8 scales differ, so the check proves the
    /// bucket selection and the per-layer-scale path in all three implementations.
    fn assert_golden_three_way_bucketed(net_bytes: &[u8], vectors_text: &'static str) {
        init_globals();
        let net =
            Network::read(&mut &net_bytes[..]).expect("the exporter's bucketed golden network loads");
        let stack = stack_of(&net);
        // A uniform-scale fixture would pass even if an implementation ignored
        // `stack_scales`; require the golden net's per-layer scales to differ.
        let scales = stack.layer_scales();
        assert!(
            scales.iter().any(|s| *s != scales[0]),
            "golden stack scales must differ per layer to exercise the per-layer path"
        );

        let vectors = parse_golden_vectors(vectors_text);
        assert!(!vectors.is_empty(), "the golden fixture has vectors");
        for category in GOLDEN_CATEGORIES {
            assert!(
                vectors.iter().any(|&(c, _, _)| c == category),
                "golden set covers the {category} category"
            );
        }

        let mut buckets_seen = std::collections::HashSet::new();
        for (category, fen, expected) in vectors {
            let pos = Position::from_fen(fen).expect("golden FEN is valid");
            let stm = pos.turn();
            let acc = Accumulator::from_position(&net, &pos);
            let pc = pos.occupied().popcnt();
            buckets_seen.insert(select_bucket(pc, stack.num_buckets()));

            let scalar = forward_bucketed_with(&net, stack, &acc, stm, pc, dot_i8);
            assert_eq!(
                scalar, expected,
                "scalar bucketed forward vs exporter on {category} {fen}"
            );

            #[cfg(target_arch = "x86_64")]
            {
                if is_x86_feature_detected!("avx2") {
                    // SAFETY: AVX2 presence just confirmed; the kernel is handed
                    // multiple-of-16 stack rows.
                    let simd = forward_bucketed_with(&net, stack, &acc, stm, pc, |a, w| unsafe {
                        dot_i8_avx2(a, w)
                    });
                    assert_eq!(
                        simd, expected,
                        "SIMD bucketed forward vs exporter on {category} {fen}"
                    );
                }
            }

            // The public dispatch path lands on the same value.
            assert_eq!(forward(&net, &acc, stm, pc), expected);
        }
        assert!(
            buckets_seen.len() >= 2,
            "golden positions must span at least two buckets"
        );
    }

    /// The CReLU bucketed cross-language differential check over the committed v2 fixture.
    #[test]
    fn bucketed_golden_vectors_agree_across_python_scalar_and_simd() {
        assert_golden_three_way_bucketed(GOLDEN_V2_NET_BYTES, GOLDEN_V2_VECTORS);
    }

    /// The SCReLU bucketed cross-language differential check over the committed v2 fixture.
    #[test]
    fn screlu_bucketed_golden_vectors_agree_across_python_scalar_and_simd() {
        assert_golden_three_way_bucketed(GOLDEN_SCRELU_V2_NET_BYTES, GOLDEN_SCRELU_V2_VECTORS);
    }
}
