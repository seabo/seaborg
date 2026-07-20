//! NNUE input encoding and the incrementally-maintained first-layer accumulator.
//!
//! The network sees each position through a sparse binary input of `2 colours × 6 piece types × 64
//! squares = 768` features, encoded once per *perspective*. There are two perspectives, White and
//! Black; each orients the board so its own first rank is the bottom and labels every piece as
//! friendly or enemy relative to itself. The [`Accumulator`] holds the first linear layer's output
//! for both perspectives — one `H`-vector each — and keeps them current across a search by folding in
//! only the pieces a move touches, exactly as [`crate::eval::EvalState`] does for the tapered score.
//!
//! Because a feature index depends only on the moving piece and its square, a single placement
//! toggles exactly one feature in each perspective, so the accumulator is a second
//! [`chess::position::PieceDeltaSink`] alongside `EvalState`: it consumes the same per-move change
//! set and is validated the same way, with [`Accumulator::from_position`] serving as the from-scratch
//! reference the incremental path is checked against.
//!
//! This module delivers the encoding and the accumulator only. Combining the two perspectives,
//! applying the activation, and reading out a score are the inference layer's concern and live
//! elsewhere.

use std::fmt;

use chess::position::{Piece, PieceDeltaSink, PieceType, Player, Position, Square};

/// Number of input features per perspective: `2 colours × 6 piece types × 64 squares`.
pub const INPUT_DIM: usize = 768;

/// Squares per `(colour, piece-type)` block within a perspective's input vector.
const SQUARES: usize = 64;

/// Stride between the friendly half and the enemy half of a perspective's input vector: the friendly
/// half occupies indices `0..384` (6 piece types × 64 squares) and the enemy half `384..768`.
const SIDE_STRIDE: usize = 384;

/// The two perspectives in accumulator-slot order: White is slot 0, Black is slot 1. Iterating this
/// updates both accumulators from one placement, and its order must agree with [`perspective_slot`].
const PERSPECTIVES: [Player; 2] = [Player::WHITE, Player::BLACK];

/// The hidden width `H` must be a positive multiple of this. Sixteen is the i16 lane count of the
/// AVX2 inference path, so honouring it lets one trained network load unchanged into both the scalar
/// and the vectorised accumulator.
pub const HIDDEN_MULTIPLE: usize = 16;

/// The accumulator slot for a perspective: White is 0, Black is 1. Must match [`PERSPECTIVES`].
#[inline(always)]
fn perspective_slot(perspective: Player) -> usize {
    usize::from(perspective.is_black())
}

/// The index of `piece` standing on `square` within the 768-input vector *for `perspective`*.
///
/// The board is oriented to the perspective side (its first rank at the bottom), the piece type
/// selects one of six 64-square blocks, and the friendly/enemy distinction selects the lower or upper
/// half of the vector:
///
/// ```text
/// oriented = perspective.relative_square(square)   // square for White; square ^ 56 for Black
/// piece_type_0 = piece_type - 1                    // Pawn = 0 … King = 5
/// side = 0 if the piece is the perspective's own colour, else 384
/// index = oriented + 64 · piece_type_0 + side      // in 0..768
/// ```
///
/// No horizontal mirroring and no king-relative bucketing are applied. Ordering the enemy half after
/// the friendly half (rather than by absolute colour) is what makes a position and its colour-flipped
/// mirror produce matching features from their respective perspectives.
#[inline]
pub fn feature_index(perspective: Player, piece: Piece, square: Square) -> usize {
    let (colour, piece_type) = piece.player_piece();
    debug_assert!(
        piece_type != PieceType::None,
        "feature_index requires a real piece"
    );
    let oriented = perspective.relative_square(square).index() as usize;
    // `PieceType` numbers `None = 0, Pawn = 1 … King = 6`; shift so the six real types index 0..6.
    let piece_type_0 = piece_type as usize - 1;
    let side = if colour == perspective {
        0
    } else {
        SIDE_STRIDE
    };
    oriented + SQUARES * piece_type_0 + side
}

/// The NNUE feature transformer: the first linear layer's quantised weights and bias.
///
/// The weights are stored feature-major — feature `f`'s `H`-element column is the contiguous slice
/// `weights[f · H .. (f + 1) · H]` — so folding a placement into the accumulator is one contiguous
/// column add. This is also the on-disk blob layout, so a loaded file needs no transposition. The
/// bias seeds each accumulator (the activation of the empty board).
///
/// This owns only what the accumulator needs to maintain activations; loading one from a network file
/// and running the rest of the forward pass are separate concerns.
pub struct FeatureTransformer {
    /// Hidden width `H`: the number of first-layer outputs per perspective.
    hidden: usize,
    /// First-layer weights, `INPUT_DIM × H` in feature-major order.
    weights: Box<[i16]>,
    /// First-layer bias, one per hidden unit (`H` entries).
    bias: Box<[i16]>,
}

impl FeatureTransformer {
    /// Builds a feature transformer from its weights and bias.
    ///
    /// `weights` must be `INPUT_DIM × hidden` in feature-major order and `bias` must be `hidden` long;
    /// `hidden` must be a positive multiple of [`HIDDEN_MULTIPLE`]. These are the same invariants a
    /// network-file loader enforces from the header, checked here so a malformed transformer cannot be
    /// constructed regardless of its source.
    pub fn new(hidden: usize, weights: Box<[i16]>, bias: Box<[i16]>) -> Self {
        assert!(
            hidden > 0 && hidden.is_multiple_of(HIDDEN_MULTIPLE),
            "hidden width must be a positive multiple of {HIDDEN_MULTIPLE}, got {hidden}"
        );
        assert_eq!(
            weights.len(),
            INPUT_DIM * hidden,
            "feature-transformer weights must be INPUT_DIM × hidden"
        );
        assert_eq!(
            bias.len(),
            hidden,
            "feature-transformer bias must have one entry per hidden unit"
        );
        Self {
            hidden,
            weights,
            bias,
        }
    }

    /// The hidden width `H`.
    #[inline(always)]
    pub fn hidden(&self) -> usize {
        self.hidden
    }

    /// The `H`-element weight column for one feature index.
    #[inline(always)]
    fn column(&self, feature: usize) -> &[i16] {
        let start = feature * self.hidden;
        &self.weights[start..start + self.hidden]
    }

    /// The first-layer bias, the activation of an empty board.
    #[inline(always)]
    fn bias(&self) -> &[i16] {
        &self.bias
    }
}

/// The first-layer activations for both perspectives, maintained incrementally.
///
/// It holds two `H`-vectors — one per perspective, White in slot 0 and Black in slot 1 — each equal
/// to the feature-transformer bias plus the weight columns of every piece on the board as that
/// perspective sees it. Every entry is a sum over the pieces, so a move changes it by only the pieces
/// it moves: [`Accumulator::add`] adds a piece's column to both perspectives and [`Accumulator::remove`]
/// subtracts it, which is how [`Position::replay_last_move_deltas`] folds a move in.
///
/// [`Accumulator::from_position`] rebuilds both vectors from scratch and is the reference the
/// incremental path is asserted against at every node under debug builds — the guard that catches the
/// slow divergence a single-move test would miss. Restoring on unmake is a copy of the saved
/// accumulator, so it is exact rather than a reverse-delta.
///
/// The activations are in the accumulator's **i16** domain. Reachable positions place at most 32
/// pieces, so each entry is `bias + (≤ 32 columns)`; keeping first-layer weight magnitudes bounded so
/// this cannot exceed `i16::MAX` is the exporter's responsibility, and an overflow here is a defect,
/// not an intended wrap.
#[derive(Clone)]
pub struct Accumulator<'ft> {
    /// The transformer supplying the weight columns and bias.
    transformer: &'ft FeatureTransformer,
    /// Per-perspective activations, indexed by [`perspective_slot`]; each `H` long.
    values: [Box<[i16]>; 2],
}

impl<'ft> Accumulator<'ft> {
    /// A fresh accumulator seeded to the transformer bias in both perspectives — the activation of an
    /// empty board, before any pieces are added.
    pub fn seeded(transformer: &'ft FeatureTransformer) -> Self {
        let bias = transformer.bias();
        Self {
            transformer,
            values: [
                bias.to_vec().into_boxed_slice(),
                bias.to_vec().into_boxed_slice(),
            ],
        }
    }

    /// Rebuilds both perspectives' activations for `pos` from scratch, scanning every piece.
    ///
    /// This seeds a search's accumulator and is the reference the incremental updates are checked
    /// against, so it drives the same [`Accumulator::add`] the incremental path uses: there is exactly
    /// one place the per-piece arithmetic lives.
    pub fn from_position(transformer: &'ft FeatureTransformer, pos: &Position) -> Self {
        let mut acc = Self::seeded(transformer);
        for &player in &PERSPECTIVES {
            for piece_type in [
                PieceType::Pawn,
                PieceType::Knight,
                PieceType::Bishop,
                PieceType::Rook,
                PieceType::Queen,
                PieceType::King,
            ] {
                for sq in pos.piece_bb(player, piece_type) {
                    acc.add(Piece::make(player, piece_type), sq);
                }
            }
        }
        acc
    }

    /// The activations for `perspective` (`H` long).
    #[inline]
    pub fn perspective(&self, perspective: Player) -> &[i16] {
        &self.values[perspective_slot(perspective)]
    }
}

impl PieceDeltaSink for Accumulator<'_> {
    #[inline]
    fn add(&mut self, piece: Piece, square: Square) {
        // Copy the shared reference out so the immutable weight-column borrow does not entangle the
        // mutable borrow of `values` below; the two touch disjoint memory.
        let transformer = self.transformer;
        for (slot, &perspective) in PERSPECTIVES.iter().enumerate() {
            let column = transformer.column(feature_index(perspective, piece, square));
            for (value, weight) in self.values[slot].iter_mut().zip(column) {
                *value += *weight;
            }
        }
    }

    #[inline]
    fn remove(&mut self, piece: Piece, square: Square) {
        let transformer = self.transformer;
        for (slot, &perspective) in PERSPECTIVES.iter().enumerate() {
            let column = transformer.column(feature_index(perspective, piece, square));
            for (value, weight) in self.values[slot].iter_mut().zip(column) {
                *value -= *weight;
            }
        }
    }
}

/// Two accumulators are equal when their activations match; the borrowed transformer, shared by
/// construction across the accumulators a search compares, is not part of the value.
impl PartialEq for Accumulator<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}

impl fmt::Debug for Accumulator<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Accumulator")
            .field("white", &self.values[perspective_slot(Player::WHITE)])
            .field("black", &self.values[perspective_slot(Player::BLACK)])
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chess::init::init_globals;
    use chess::mono_traits::{All, Legal};
    use chess::movelist::BasicMoveList;

    /// The six real piece types, in a fixed order for exhaustive iteration.
    const PIECE_TYPES: [PieceType; 6] = [
        PieceType::Pawn,
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ];

    /// A deterministic feature transformer for tests, with weights bounded to `[-7, 7]` and bias to
    /// `[-2, 2]`. With at most 32 pieces on the board an activation stays within `2 + 32 · 7 = 226`,
    /// far inside `i16`, so these tests exercise the arithmetic without a defect-level overflow. The
    /// values vary by feature and hidden unit so different columns are distinguishable.
    fn test_transformer() -> FeatureTransformer {
        let hidden = 16;
        let mut weights = vec![0i16; INPUT_DIM * hidden];
        for (feature, column) in weights.chunks_mut(hidden).enumerate() {
            for (unit, w) in column.iter_mut().enumerate() {
                *w = ((feature * 31 + unit * 7) % 15) as i16 - 7;
            }
        }
        let bias: Vec<i16> = (0..hidden).map(|unit| (unit as i16 % 5) - 2).collect();
        FeatureTransformer::new(hidden, weights.into_boxed_slice(), bias.into_boxed_slice())
    }

    /// The feature index matches the design contract on representative pieces, squares, and
    /// perspectives: board orientation flips for Black, the six piece-type blocks are placed at
    /// multiples of 64, and the friendly/enemy halves are split at 384.
    #[test]
    fn feature_index_matches_the_contract() {
        let a1 = Square::from_rank_file(0, 0); // index 0
        let h8 = Square::from_rank_file(7, 7); // index 63
        let e4 = Square::from_rank_file(3, 4); // index 28

        // A White pawn on a1: own colour, so the friendly half; a1 is unflipped for White and flips
        // to a8 (index 56) for Black, where it is an enemy piece.
        let white_pawn = Piece::make(Player::WHITE, PieceType::Pawn);
        assert_eq!(feature_index(Player::WHITE, white_pawn, a1), 0);
        assert_eq!(
            feature_index(Player::BLACK, white_pawn, a1),
            56 + SIDE_STRIDE
        );

        // A Black king on h8: the last piece-type block (5 · 64 = 320). Enemy of White, friendly to
        // Black; h8 is unflipped for White and flips to h1 (index 7) for Black.
        let black_king = Piece::make(Player::BLACK, PieceType::King);
        assert_eq!(
            feature_index(Player::WHITE, black_king, h8),
            63 + 320 + SIDE_STRIDE
        );
        assert_eq!(feature_index(Player::BLACK, black_king, h8), 7 + 320);

        // A White knight on e4 (index 28): knight block is 1 · 64; e4 flips to e5 (index 36) for Black.
        let white_knight = Piece::make(Player::WHITE, PieceType::Knight);
        assert_eq!(feature_index(Player::WHITE, white_knight, e4), 28 + 64);
        assert_eq!(
            feature_index(Player::BLACK, white_knight, e4),
            36 + 64 + SIDE_STRIDE
        );
    }

    /// For a fixed perspective the feature index is a bijection from `(colour, piece type, square)`
    /// onto `0..768`: every combination maps to a distinct index and together they cover the whole
    /// input vector, with no collisions and nothing out of range. Checked for both perspectives.
    #[test]
    fn feature_index_is_a_bijection_onto_the_input_vector() {
        for &perspective in &PERSPECTIVES {
            let mut seen = vec![false; INPUT_DIM];
            for &colour in &PERSPECTIVES {
                for &piece_type in &PIECE_TYPES {
                    let piece = Piece::make(colour, piece_type);
                    for sq in 0..64u8 {
                        let index = feature_index(
                            perspective,
                            piece,
                            Square::from_rank_file(sq as usize / 8, sq as usize % 8),
                        );
                        assert!(index < INPUT_DIM, "index {index} out of range");
                        assert!(!seen[index], "feature index {index} collided");
                        seen[index] = true;
                    }
                }
            }
            assert!(
                seen.into_iter().all(|hit| hit),
                "not every feature index was produced"
            );
        }
    }

    /// Exhaustively walks the legal move tree from `pos` to `depth`, maintaining an incrementally
    /// updated [`Accumulator`] alongside it and asserting at every node — after each make and after
    /// each unmake — that it equals a from-scratch recomputation.
    ///
    /// This is the property a debug-build assertion in the search will rely on, exercised here over
    /// full subtrees so captures, promotions, castling, and en passant are folded and undone many
    /// times in sequence. A single-move test cannot catch an update that is self-consistent per move
    /// but drifts over a deep line; walking to depth does.
    ///
    /// On entry `acc` must already equal `Accumulator::from_position(ft, pos)`; on return the position
    /// and the accumulator are both restored to what they were.
    fn walk(ft: &FeatureTransformer, pos: &mut Position, acc: &mut Accumulator, depth: u32) {
        debug_assert_eq!(*acc, Accumulator::from_position(ft, pos));
        if depth == 0 {
            return;
        }

        let moves = pos.generate::<BasicMoveList, All, Legal>();
        for mov in &moves {
            let restore = acc.clone();

            pos.make_move(mov);
            pos.replay_last_move_deltas(acc);
            assert_eq!(
                *acc,
                Accumulator::from_position(ft, pos),
                "incremental accumulator diverged after {mov}"
            );

            walk(ft, pos, acc, depth - 1);

            pos.unmake_move();
            *acc = restore;
            assert_eq!(
                *acc,
                Accumulator::from_position(ft, pos),
                "incremental accumulator not restored after unmaking {mov}"
            );
        }
    }

    /// The incremental accumulator tracks a from-scratch recomputation across whole subtrees of every
    /// move kind: quiet moves, captures, castling both sides, en passant, and promotions with and
    /// without capture.
    #[test]
    fn incremental_accumulator_matches_from_scratch_over_subtrees() {
        init_globals();
        let ft = test_transformer();

        // Depths kept modest so the from-scratch check at every node stays cheap, while still forcing
        // long make/unmake sequences through each position's characteristic features.
        let cases = [
            // The opening: quiet development and the first captures.
            (
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                4,
            ),
            // Kiwipete: castling for both sides, captures of every piece, and pins.
            (
                "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
                3,
            ),
            // A pawn poised for a double push that creates a real en-passant target.
            ("4k3/8/8/8/5p2/8/4P3/4K3 w - - 0 1", 5),
            // Pawns on the seventh for both sides: promotions, including promotion captures.
            ("n1n5/PPPk4/8/8/8/8/4Kppp/5N1N b - - 0 1", 4),
        ];

        for (fen, depth) in cases {
            let mut pos = Position::from_fen(fen).expect("test FEN is valid");
            let mut acc = Accumulator::from_position(&ft, &pos);
            walk(&ft, &mut pos, &mut acc, depth);
        }
    }

    /// A move made and then unmade restores the accumulator bit-for-bit: the value the search restores
    /// to — a copy of the accumulator saved before the move — is exactly a from-scratch recomputation
    /// of the restored position, so restoration is exact rather than merely equivalent.
    #[test]
    fn make_then_unmake_restores_the_accumulator_exactly() {
        init_globals();
        let ft = test_transformer();

        let mut pos = Position::from_fen(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        )
        .expect("test FEN is valid");
        let before = Accumulator::from_position(&ft, &pos);

        let moves = pos.generate::<BasicMoveList, All, Legal>();
        for mov in &moves {
            let mut acc = before.clone();
            pos.make_move(mov);
            pos.replay_last_move_deltas(&mut acc);
            assert_eq!(
                acc,
                Accumulator::from_position(&ft, &pos),
                "incremental accumulator wrong after {mov}"
            );

            pos.unmake_move();
            // The stack-based restoration the search performs is this copy; assert the value it
            // restores to matches a fresh recomputation of the restored position.
            acc = before.clone();
            assert_eq!(acc, before);
            assert_eq!(before, Accumulator::from_position(&ft, &pos));
        }
    }

    /// A clone of a position carries an accumulator that a fresh `from_position` reproduces exactly,
    /// so seeding a search from a cloned position is correct by construction. The per-perspective
    /// vectors are `H` long.
    #[test]
    fn accumulator_of_a_clone_matches_a_fresh_computation() {
        init_globals();
        let ft = test_transformer();

        let mut pos = Position::from_fen("r3k2r/pp3ppp/2n5/8/3P4/2N2N2/PP3PPP/R3K2R w KQkq - 0 1")
            .expect("test FEN is valid");
        // Advance a few plies so the cloned position is mid-game rather than a start position.
        for uci in ["e1g1", "e8g8", "d4d5", "c6e5"] {
            pos.make_uci_move(uci).expect("uci move is legal");
        }

        let clone = pos.clone();
        let from_clone = Accumulator::from_position(&ft, &clone);
        assert_eq!(from_clone, Accumulator::from_position(&ft, &pos));
        assert_eq!(from_clone.perspective(Player::WHITE).len(), ft.hidden());
        assert_eq!(from_clone.perspective(Player::BLACK).len(), ft.hidden());
    }
}
