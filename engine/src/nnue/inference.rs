//! The portable scalar quantized forward pass: from the two per-perspective
//! accumulators through the clipped activation and the output layer to a single
//! centipawn score.
//!
//! This is the reference implementation of the network's arithmetic. It runs on
//! every target, including those without the AVX2 path, and it is the oracle the
//! SIMD path and the PyTorch quantized forward are both checked against, so its
//! integer arithmetic is normative: the scale factors, the clipped-ReLU domain,
//! the accumulation widths, and the rounding mode all follow
//! `docs/nnue-design-contract.md` exactly and must not drift from it.
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
    s += dot_clipped(own, own_weights, qa);
    s += dot_clipped(enemy, enemy_weights, qa);

    // Widen to i64 before scaling: `s` fits i32 but `s · SCALE` need not.
    let numerator = i64::from(s) * i64::from(network.scale());
    let denominator = i64::from(network.qa()) * i64::from(network.qb());
    let eval_cp = round_div(numerator, denominator);
    eval_cp.clamp(EVAL_CP_MIN, EVAL_CP_MAX) as i32
}

/// The clipped-ReLU-weighted dot product of one perspective block: for each unit,
/// clamp the activation to `[0, QA]` and multiply by its output weight, summing in
/// i32.
#[inline]
fn dot_clipped(activations: &[i16], weights: &[i16], qa: i32) -> i32 {
    activations
        .iter()
        .zip(weights)
        .map(|(&a, &w)| i32::from(a).clamp(0, qa) * i32::from(w))
        .sum()
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
}
