use chess::position::{Piece, PieceDeltaSink, PieceType, Player, Position, Square};

/// Exchange values used by static exchange evaluation and move ordering, indexed by
/// [`PieceType`].
///
/// These are deliberately kept separate from the values the positional evaluation below uses.
/// Static exchange evaluation answers "does this capture sequence win material", which wants a
/// single, stable, side-independent price per piece; the evaluation instead wants game-phase
/// dependent values that differ between the middlegame and the endgame. Bishop and knight share a
/// price here because a bare piece-count exchange cannot tell them apart, whereas the evaluation
/// does distinguish them. See [`crate::see`].
pub const PAWN_VALUE: i16 = 100;
pub const KNIGHT_VALUE: i16 = 300;
pub const BISHOP_VALUE: i16 = 300;
pub const ROOK_VALUE: i16 = 500;
pub const QUEEN_VALUE: i16 = 900;
pub const KING_VALUE: i16 = 10000;

pub const PIECE_VALUES: [i16; 7] = [
    0, // PieceType::None,
    PAWN_VALUE,
    KNIGHT_VALUE,
    BISHOP_VALUE,
    ROOK_VALUE,
    QUEEN_VALUE,
    KING_VALUE,
];

/// The exchange value of `PieceType`, used by static exchange evaluation and move ordering.
pub fn piece_value(piece_type: PieceType) -> i16 {
    // SAFETY: `PieceType` is represented by values in 0..7. Safe matching measurably regresses SEE
    // and fixed-depth search.
    unsafe { *PIECE_VALUES.get_unchecked(piece_type as usize) }
}

/// Adds static evaluation functionality to a type representing a chess position.
pub trait Evaluation {
    /// The tapered piece-square evaluation, from White's perspective (positive favours White).
    fn static_eval(&self) -> i16;
}

impl Evaluation for Position {
    fn static_eval(&self) -> i16 {
        tapered_evaluation(self)
    }
}

/// Piece values in centipawns for the two interpolation endpoints, indexed by [`PieceType`]. The
/// king carries no material value because both sides always have exactly one.
///
/// Knight and bishop differ in both phases: a bishop is worth marginally more than a knight in the
/// middlegame and appreciably more in the endgame, where its range across an emptying board tells.
///
/// These are the Texel-tuned values of the PeSTO ("Piece-Square Tables Only") evaluation by Ronald
/// Friederich (rofChade), reproduced from the Chess Programming Wiki. They were fitted by logistic
/// regression against game outcomes, so the numbers are not round and are not meant to be adjusted
/// by hand; they come as a set with [`MG_PST`] and [`EG_PST`] below.
const MG_VALUE: [i32; 7] = [0, 82, 337, 365, 477, 1025, 0];
const EG_VALUE: [i32; 7] = [0, 94, 281, 297, 512, 936, 0];

/// The game-phase weight each piece contributes, indexed by [`PieceType`]. Summed over both sides
/// at the start of the game this reaches 24 (four minor pieces at 1, four rooks at 2, two queens at
/// 4); it falls towards 0 as pieces come off, driving the interpolation towards the endgame tables.
const GAME_PHASE_INC: [i32; 7] = [0, 0, 1, 1, 2, 4, 0];

/// The full opening game phase. Pawn promotions can push the running phase above this, so it is
/// saturated before interpolation rather than assumed to be a ceiling.
const MAX_PHASE: i32 = 24;

/// The static evaluation, from White's perspective, in centipawns.
///
/// The value interpolates between a middlegame and an endgame score by the material still on the
/// board. Each occupied square contributes a piece value plus a piece-square bonus for both phases;
/// the two totals are then blended by the game phase so that, for example, a king that wants the
/// corner in the middlegame and the centre in the endgame is scored correctly in between.
///
/// The evaluation is position-intrinsic: it reads only piece placement and colour, all of which the
/// Zobrist key covers. It must never read the halfmove clock or the move history, which the key does
/// not cover; doing so would let a transposition-table value computed in one history be reused in
/// another where it does not hold. The approach of a fifty-move draw is the search's concern, not
/// the leaf evaluation's.
///
/// This is the from-scratch computation. [`EvalState`] maintains the same value incrementally across
/// a search and shares this arithmetic exactly, so the two never disagree.
fn tapered_evaluation(pos: &Position) -> i16 {
    EvalState::from_position(pos).score()
}

/// An incrementally maintainable tapered evaluation, accumulated from White's perspective.
///
/// It holds the three quantities [`EvalState::score`] interpolates: the running middlegame and
/// endgame piece-square totals and the game phase. Every one is a sum over the pieces on the board,
/// so a move changes it by only the pieces it moves. `EvalState` implements [`PieceDeltaSink`], which
/// lets [`Position::replay_last_move_deltas`] fold a move in directly and turn a full-board rescan at
/// each node into a handful of additions. That is the seam a later network accumulator slots into: it
/// is another consumer of the same per-move change set, maintained the same way and validated the
/// same way, rather than a new mechanism.
///
/// [`EvalState::from_position`] recomputes the whole board and is the reference the incremental path
/// is asserted against under debug builds at every node — the one guard that catches the slow
/// divergence a short unit test would miss.
///
/// The totals are always from White's perspective; the side to move never enters them. A null move
/// changes only the side to move, so it leaves an `EvalState` unchanged, which is what will let one
/// be carried unmodified across a null move once the search gains them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvalState {
    /// Middlegame piece-square total, White minus Black.
    mg: i32,
    /// Endgame piece-square total, White minus Black.
    eg: i32,
    /// Game phase: `MAX_PHASE` with all material on, falling towards 0 as pieces come off.
    phase: i32,
}

impl EvalState {
    /// Computes the accumulator for a position from scratch, scanning every piece.
    ///
    /// This is both how a search's accumulator is seeded and the reference the incremental updates
    /// are checked against, so building it from the same [`EvalState::add_piece`] the incremental
    /// path uses is deliberate: there is exactly one place the per-piece arithmetic lives.
    pub fn from_position(pos: &Position) -> Self {
        let mut state = Self {
            mg: 0,
            eg: 0,
            phase: 0,
        };
        for player in [Player::WHITE, Player::BLACK] {
            for piece_type in [
                PieceType::Pawn,
                PieceType::Knight,
                PieceType::Bishop,
                PieceType::Rook,
                PieceType::Queen,
                PieceType::King,
            ] {
                for sq in pos.piece_bb(player, piece_type) {
                    state.add_piece(player, piece_type, sq);
                }
            }
        }
        state
    }

    /// The tapered score in centipawns, from White's perspective (positive favours White).
    pub fn score(&self) -> i16 {
        let mg_phase = self.phase.min(MAX_PHASE);
        let eg_phase = MAX_PHASE - mg_phase;
        ((self.mg * mg_phase + self.eg * eg_phase) / MAX_PHASE) as i16
    }

    /// The signed middlegame, endgame and phase contribution of one piece on one square, from
    /// White's perspective. Adding a piece adds this; removing one subtracts it. Keeping it in a
    /// single function is what guarantees the from-scratch and incremental paths cannot drift.
    #[inline(always)]
    fn term(player: Player, piece_type: PieceType, sq: Square) -> (i32, i32, i32) {
        let sign = if player.is_white() { 1 } else { -1 };
        let pt = piece_type as usize;
        let idx = pst_index(player, sq.index());
        (
            sign * (MG_VALUE[pt] + MG_PST[pt][idx] as i32),
            sign * (EG_VALUE[pt] + EG_PST[pt][idx] as i32),
            GAME_PHASE_INC[pt],
        )
    }

    #[inline(always)]
    fn add_piece(&mut self, player: Player, piece_type: PieceType, sq: Square) {
        let (mg, eg, phase) = Self::term(player, piece_type, sq);
        self.mg += mg;
        self.eg += eg;
        self.phase += phase;
    }

    #[inline(always)]
    fn remove_piece(&mut self, player: Player, piece_type: PieceType, sq: Square) {
        let (mg, eg, phase) = Self::term(player, piece_type, sq);
        self.mg -= mg;
        self.eg -= eg;
        self.phase -= phase;
    }
}

impl PieceDeltaSink for EvalState {
    #[inline(always)]
    fn add(&mut self, piece: Piece, square: Square) {
        let (player, piece_type) = piece.player_piece();
        self.add_piece(player, piece_type, square);
    }

    #[inline(always)]
    fn remove(&mut self, piece: Piece, square: Square) {
        let (player, piece_type) = piece.player_piece();
        self.remove_piece(player, piece_type, square);
    }
}

/// Maps a board square to an index into the piece-square tables.
///
/// The tables are stored exactly as published, with a8 at index 0 and rank 8 as the first row, and
/// they read from the side-to-score's own perspective. A White piece therefore mirrors its board
/// square (`index() ^ 56`, a vertical flip) to reach the matching table row, while a Black piece
/// reads its board square directly: b8 for Black and b1 for White land on the same table entry, so
/// mirror-image positions receive equal and opposite scores.
#[inline(always)]
fn pst_index(player: Player, square_index: u8) -> usize {
    let flip = if player.is_white() { 56 } else { 0 };
    (square_index ^ flip) as usize
}

// The piece-square tables, indexed by [`PieceType`] then by the table index that [`pst_index`]
// produces. Index 0 (`PieceType::None`) is unused and left at zero. Values are the PeSTO
// middlegame and endgame tables (Ronald Friederich, rofChade), reproduced from the Chess
// Programming Wiki. Each block is 8 rows of 8 squares, a8..h8 first down to a1..h1.

#[rustfmt::skip]
const MG_PST: [[i16; 64]; 7] = [
    [0; 64], // PieceType::None
    // Pawn
    [
          0,   0,   0,   0,   0,   0,   0,   0,
         98, 134,  61,  95,  68, 126,  34, -11,
         -6,   7,  26,  31,  65,  56,  25, -20,
        -14,  13,   6,  21,  23,  12,  17, -23,
        -27,  -2,  -5,  12,  17,   6,  10, -25,
        -26,  -4,  -4, -10,   3,   3,  33, -12,
        -35,  -1, -20, -23, -15,  24,  38, -22,
          0,   0,   0,   0,   0,   0,   0,   0,
    ],
    // Knight
    [
        -167, -89, -34, -49,  61, -97, -15, -107,
         -73, -41,  72,  36,  23,  62,   7,  -17,
         -47,  60,  37,  65,  84, 129,  73,   44,
          -9,  17,  19,  53,  37,  69,  18,   22,
         -13,   4,  16,  13,  28,  19,  21,   -8,
         -23,  -9,  12,  10,  19,  17,  25,  -16,
         -29, -53, -12,  -3,  -1,  18, -14,  -19,
        -105, -21, -58, -33, -17, -28, -19,  -23,
    ],
    // Bishop
    [
        -29,   4, -82, -37, -25, -42,   7,  -8,
        -26,  16, -18, -13,  30,  59,  18, -47,
        -16,  37,  43,  40,  35,  50,  37,  -2,
         -4,   5,  19,  50,  37,  37,   7,  -2,
         -6,  13,  13,  26,  34,  12,  10,   4,
          0,  15,  15,  15,  14,  27,  18,  10,
          4,  15,  16,   0,   7,  21,  33,   1,
        -33,  -3, -14, -21, -13, -12, -39, -21,
    ],
    // Rook
    [
         32,  42,  32,  51,  63,   9,  31,  43,
         27,  32,  58,  62,  80,  67,  26,  44,
         -5,  19,  26,  36,  17,  45,  61,  16,
        -24, -11,   7,  26,  24,  35,  -8, -20,
        -36, -26, -12,  -1,   9,  -7,   6, -23,
        -45, -25, -16, -17,   3,   0,  -5, -33,
        -44, -16, -20,  -9,  -1,  11,  -6, -71,
        -19, -13,   1,  17,  16,   7, -37, -26,
    ],
    // Queen
    [
        -28,   0,  29,  12,  59,  44,  43,  45,
        -24, -39,  -5,   1, -16,  57,  28,  54,
        -13, -17,   7,   8,  29,  56,  47,  57,
        -27, -27, -16, -16,  -1,  17,  -2,   1,
         -9, -26,  -9, -10,  -2,  -4,   3,  -3,
        -14,   2, -11,  -2,  -5,   2,  14,   5,
        -35,  -8,  11,   2,   8,  15,  -3,   1,
         -1, -18,  -9,  10, -15, -25, -31, -50,
    ],
    // King
    [
        -65,  23,  16, -15, -56, -34,   2,  13,
         29,  -1, -20,  -7,  -8,  -4, -38, -29,
         -9,  24,   2, -16, -20,   6,  22, -22,
        -17, -20, -12, -27, -30, -25, -14, -36,
        -49,  -1, -27, -39, -46, -44, -33, -51,
        -14, -14, -22, -46, -44, -30, -15, -27,
          1,   7,  -8, -64, -43, -16,   9,   8,
        -15,  36,  12, -54,   8, -28,  24,  14,
    ],
];

#[rustfmt::skip]
const EG_PST: [[i16; 64]; 7] = [
    [0; 64], // PieceType::None
    // Pawn
    [
          0,   0,   0,   0,   0,   0,   0,   0,
        178, 173, 158, 134, 147, 132, 165, 187,
         94, 100,  85,  67,  56,  53,  82,  84,
         32,  24,  13,   5,  -2,   4,  17,  17,
         13,   9,  -3,  -7,  -7,  -8,   3,  -1,
          4,   7,  -6,   1,   0,  -5,  -1,  -8,
         13,   8,   8,  10,  13,   0,   2,  -7,
          0,   0,   0,   0,   0,   0,   0,   0,
    ],
    // Knight
    [
        -58, -38, -13, -28, -31, -27, -63, -99,
        -25,  -8, -25,  -2,  -9, -25, -24, -52,
        -24, -20,  10,   9,  -1,  -9, -19, -41,
        -17,   3,  22,  22,  22,  11,   8, -18,
        -18,  -6,  16,  25,  16,  17,   4, -18,
        -23,  -3,  -1,  15,  10,  -3, -20, -22,
        -42, -20, -10,  -5,  -2, -20, -23, -44,
        -29, -51, -23, -15, -22, -18, -50, -64,
    ],
    // Bishop
    [
        -14, -21, -11,  -8,  -7,  -9, -17, -24,
         -8,  -4,   7, -12,  -3, -13,  -4, -14,
          2,  -8,   0,  -1,  -2,   6,   0,   4,
         -3,   9,  12,   9,  14,  10,   3,   2,
         -6,   3,  13,  19,   7,  10,  -3,  -9,
        -12,  -3,   8,  10,  13,   3,  -7, -15,
        -14, -18,  -7,  -1,   4,  -9, -15, -27,
        -23,  -9, -23,  -5,  -9, -16,  -5, -17,
    ],
    // Rook
    [
        13, 10, 18, 15, 12,  12,   8,   5,
        11, 13, 13, 11, -3,   3,   8,   3,
         7,  7,  7,  5,  4,  -3,  -5,  -3,
         4,  3, 13,  1,  2,   1,  -1,   2,
         3,  5,  8,  4, -5,  -6,  -8, -11,
        -4,  0, -5, -1, -7, -12,  -8, -16,
        -6, -6,  0,  2, -9,  -9, -11,  -3,
        -9,  2,  3, -1, -5, -13,   4, -20,
    ],
    // Queen
    [
         -9,  22,  22,  27,  27,  19,  10,  20,
        -17,  20,  32,  41,  58,  25,  30,   0,
        -20,   6,   9,  49,  47,  35,  19,   9,
          3,  22,  24,  45,  57,  40,  57,  36,
        -18,  28,  19,  47,  31,  34,  39,  23,
        -16, -27,  15,   6,   9,  17,  10,   5,
        -22, -23, -30, -16, -16, -23, -36, -32,
        -33, -28, -22, -43,  -5, -32, -20, -41,
    ],
    // King
    [
        -74, -35, -18, -18, -11,  15,   4, -17,
        -12,  17,  14,  17,  17,  38,  23,  11,
         10,  17,  23,  15,  20,  45,  44,  13,
         -8,  22,  24,  27,  26,  33,  26,   3,
        -18,  -4,  21,  24,  27,  23,   9, -11,
        -19,  -3,  11,  21,  23,  16,   7,  -9,
        -27, -11,   4,  13,  14,   4,  -5, -17,
        -53, -34, -21, -11, -28, -14, -24, -43,
    ],
];

#[cfg(test)]
mod tests {
    use super::*;
    use chess::init::init_globals;
    use chess::mono_traits::{All, Legal};
    use chess::movelist::BasicMoveList;

    /// Exhaustively walks the legal move tree from `pos` to `depth`, maintaining an incrementally
    /// updated [`EvalState`] alongside it and asserting at every node — after each make and after
    /// each unmake — that it equals a from-scratch recomputation.
    ///
    /// This is the property the debug-build assertion in the search relies on, exercised here over
    /// full subtrees so that captures, promotions, castling and en passant are all folded and undone
    /// many times in sequence. A single-move test cannot catch an update that is self-consistent per
    /// move but drifts over a deep line; walking to depth does.
    ///
    /// On entry `state` must already equal `EvalState::from_position(pos)`; on return the position and
    /// the accumulator are both restored to what they were.
    fn walk(pos: &mut Position, state: &mut EvalState, depth: u32) {
        debug_assert_eq!(*state, EvalState::from_position(pos));
        if depth == 0 {
            return;
        }

        let moves = pos.generate::<BasicMoveList, All, Legal>();
        for mov in &moves {
            let restore = *state;

            pos.make_move(mov);
            pos.replay_last_move_deltas(state);
            assert_eq!(
                *state,
                EvalState::from_position(pos),
                "incremental accumulator diverged after {mov}"
            );
            assert_eq!(
                state.score(),
                pos.static_eval(),
                "incremental score diverged after {mov}"
            );

            walk(pos, state, depth - 1);

            pos.unmake_move();
            *state = restore;
            assert_eq!(
                *state,
                EvalState::from_position(pos),
                "incremental accumulator not restored after unmaking {mov}"
            );
        }
    }

    /// The incremental accumulator tracks a from-scratch recomputation across whole subtrees of every
    /// position feature the tapered evaluation reads: ordinary moves, captures, castling both sides,
    /// en passant, and promotions with and without capture.
    #[test]
    fn incremental_evaluation_matches_from_scratch_over_subtrees() {
        init_globals();

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
            let mut state = EvalState::from_position(&pos);
            walk(&mut pos, &mut state, depth);
        }
    }

    /// A move made and then unmade leaves the accumulator bit-for-bit as it was, so restoration is
    /// exact rather than merely score-equivalent. This is what lets the search restore by copying the
    /// saved accumulator instead of recomputing it.
    #[test]
    fn make_then_unmake_restores_the_accumulator_exactly() {
        init_globals();

        let mut pos = Position::from_fen(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        )
        .expect("test FEN is valid");
        let before = EvalState::from_position(&pos);

        let moves = pos.generate::<BasicMoveList, All, Legal>();
        for mov in &moves {
            let mut state = before;
            pos.make_move(mov);
            pos.replay_last_move_deltas(&mut state);
            pos.unmake_move();
            // The stack-based restoration the search performs is this copy; assert the value it
            // restores to is identical to the starting accumulator.
            assert_eq!(before, EvalState::from_position(&pos));
        }
    }

    /// The from-scratch score agrees with the position's `static_eval`, and a clone of a position
    /// carries an accumulator that a fresh `from_position` reproduces exactly. This is the guarantee
    /// that seeding a search from a cloned position is correct by construction.
    #[test]
    fn accumulator_of_a_clone_matches_a_fresh_computation() {
        init_globals();

        let mut pos = Position::from_fen("r3k2r/pp3ppp/2n5/8/3P4/2N2N2/PP3PPP/R3K2R w KQkq - 0 1")
            .expect("test FEN is valid");
        // Advance a few plies so the cloned position is mid-game rather than a start position.
        for uci in ["e1g1", "e8g8", "d4d5", "c6e5"] {
            pos.make_uci_move(uci).expect("uci move is legal");
        }

        let clone = pos.clone();
        assert_eq!(
            EvalState::from_position(&clone),
            EvalState::from_position(&pos)
        );
        assert_eq!(
            EvalState::from_position(&clone).score(),
            clone.static_eval()
        );
    }
}
