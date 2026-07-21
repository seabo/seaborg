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

use super::{Accumulator, Network};

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
/// The output accumulator `s` is i32 and the multiply by `SCALE` widens to i64
/// before the divide, exactly as the contract requires: with the accumulator in
/// i16, the activations are clamped to `[0, QA]` so each output term is bounded
/// and, for contract-bounded output weights, `s` stays well inside i32 while the
/// subsequent `s · SCALE` can exceed it and so is done in i64.
///
/// # Panics
///
/// Panics if `accumulator` was not built from `network` — the two perspectives
/// must be `H` long and the output weight block `2H` long. Pairing an
/// accumulator with a foreign network is a programming error, and the mismatch
/// is caught rather than silently reading past a block.
pub fn forward(network: &Network, accumulator: &Accumulator, side_to_move: Player) -> i32 {
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
    let mut s: i32 = network.output_bias()[0];
    s += dot_clipped_selected(own, own_weights, qa);
    s += dot_clipped_selected(enemy, enemy_weights, qa);

    // Widen to i64 before scaling: `s` fits i32 but `s · SCALE` need not.
    let numerator = i64::from(s) * i64::from(network.scale());
    let denominator = i64::from(network.qa()) * i64::from(network.qb());
    let eval_cp = round_div(numerator, denominator);
    eval_cp.clamp(EVAL_CP_MIN, EVAL_CP_MAX) as i32
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

    /// Builds a deterministic test network with the given hidden width and blocks,
    /// at the default scales.
    fn network(
        hidden: u32,
        w_ft: Vec<i16>,
        b_ft: Vec<i16>,
        w_out: Vec<i16>,
        b_out: i32,
    ) -> Network {
        Network::new(
            hidden,
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
        let h = hidden as usize;
        let mut w_ft = vec![0i16; INPUT_DIM as usize * h];
        for (feature, column) in w_ft.chunks_mut(h).enumerate() {
            for (unit, w) in column.iter_mut().enumerate() {
                *w = ((feature * 31 + unit * 7) % 41) as i16 - 20;
            }
        }
        let b_ft: Vec<i16> = (0..h).map(|unit| (unit as i16 % 7) - 3).collect();
        let w_out: Vec<i16> = (0..2 * h).map(|j| ((j * 13) % 49) as i16 - 24).collect();
        network(hidden, w_ft, b_ft, w_out, 0)
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

        let mut s = i64::from(net.output_bias()[0]);
        for (j, &a) in own.iter().enumerate() {
            s += a.clamp(0, qa) * i64::from(w_out[j]);
        }
        for (j, &a) in enemy.iter().enumerate() {
            s += a.clamp(0, qa) * i64::from(w_out[h + j]);
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

            let got = forward(&net, &acc, stm);
            assert_eq!(got, expected, "forward pass mismatch on {fen}");

            let reference = reference_forward(&net, &pos, stm);
            assert_eq!(
                reference, expected,
                "independent dense reference mismatch on {fen}"
            );
        }
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
                    forward(&net, &acc, stm),
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
            let s = forward(&net, &Accumulator::from_position(&net, &pos), pos.turn());
            let m = forward(&net, &Accumulator::from_position(&net, &mir), mir.turn());
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
        assert_eq!(forward(&net_hi, &acc_hi, stm), expected_hi as i32);

        // Every entry i16::MIN -> clipped to 0, so only the bias survives.
        let net_lo = constant_accumulator_network(hidden, i16::MIN, 1000, -12_345);
        let acc_lo = Accumulator::from_position(&net_lo, &pos);
        let expected_lo = round_div(
            i64::from(-12_345) * i64::from(SCALE),
            i64::from(QA) * i64::from(QB),
        );
        assert_eq!(forward(&net_lo, &acc_lo, stm), expected_lo as i32);
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
        assert_eq!(forward(&net, &acc, stm), expected);
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
        assert_eq!(forward(&net_pos, &acc_pos, stm), 10_000);

        // Large negative raw output: clamps to -10_000.
        let net_neg = constant_accumulator_network(hidden, i16::MAX, -20_000, 0);
        let acc_neg = Accumulator::from_position(&net_neg, &pos);
        assert_eq!(forward(&net_neg, &acc_neg, stm), -10_000);
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
    fn random_contract_network(rng: &mut SmallRng, hidden: u32) -> Network {
        let h = hidden as usize;
        let w_ft: Vec<i16> = (0..INPUT_DIM as usize * h)
            .map(|_| rng.random_range(-200..=200))
            .collect();
        let b_ft: Vec<i16> = (0..h).map(|_| rng.random_range(-500..=500)).collect();
        let w_out: Vec<i16> = (0..2 * h).map(|_| rng.random_range(-300..=300)).collect();
        let b_out: i32 = rng.random_range(-100_000..=100_000);
        network(hidden, w_ft, b_ft, w_out, b_out)
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
                        forward(&golden_net, &acc, stm),
                        expected,
                        "AVX2 golden {fen}"
                    );
                }

                // Randomized positions against randomized contract-valid networks.
                let mut rng = SmallRng::seed_from_u64(0x9E37_79B9);
                for hidden in [16u32, 32, 256] {
                    let net = random_contract_network(&mut rng, hidden);
                    for _ in 0..40 {
                        let plies = rng.random_range(1..=40);
                        let pos = random_position(&mut rng, plies);
                        let stm = pos.turn();
                        let acc = Accumulator::from_position(&net, &pos);

                        // Full forward (AVX2) equals the independent dense oracle.
                        assert_eq!(
                            forward(&net, &acc, stm),
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
}
