use super::magic::sliding_attack;
use crate::bb::Bitboard;
use crate::position::{u8_to_u64, Player, Square};

const BISHOP_DELTAS: [i8; 4] = [7, 9, -9, -7];
const ROOK_DELTAS: [i8; 4] = [8, 1, -8, -1];

static BOARD_TABLES: BoardTables = BoardTables::new();

struct BoardTables {
    king: [u64; 64],
    knight: [u64; 64],
    pawn_attacks: [[u64; 64]; 2],
    lines: [[u64; 64]; 64],
    between: [[u64; 64]; 64],
}

impl BoardTables {
    const fn new() -> Self {
        let (between, lines) = gen_between_and_line_bbs();
        Self {
            king: gen_king_moves(),
            knight: gen_knight_moves(),
            pawn_attacks: gen_pawn_attacks(),
            lines,
            between,
        }
    }
}

#[inline(always)]
fn tables() -> &'static BoardTables {
    &BOARD_TABLES
}

/// Generate Knight moves Bitboard from an origin square
#[inline(always)]
pub fn knight_moves(square: Square) -> Bitboard {
    debug_assert!(square.is_okay());
    unsafe { Bitboard::new(*tables().knight.get_unchecked(square.0 as usize)) }
}

/// Generate King moves Bitboard from an origin square
#[inline(always)]
pub fn king_moves(square: Square) -> Bitboard {
    debug_assert!(square.is_okay());
    unsafe { Bitboard::new(*tables().king.get_unchecked(square.0 as usize)) }
}

const fn gen_knight_moves() -> [u64; 64] {
    let mut table = [0; 64];
    let mut index = 0;
    while index < 64 {
        let mut mask: u64 = 0;
        let file = index % 8;

        // 1 UP   + 2 LEFT
        if file > 1 && index < 56 {
            mask |= 1 << (index + 6);
        }
        // 2 UP   + 1 LEFT
        if file != 0 && index < 48 {
            mask |= 1 << (index + 15);
        }
        // 2 UP   + 1 RIGHT
        if file != 7 && index < 48 {
            mask |= 1 << (index + 17);
        }
        // 1 UP   + 2 RIGHT
        if file < 6 && index < 56 {
            mask |= 1 << (index + 10);
        }
        // 1 DOWN   + 2 RIGHT
        if file < 6 && index > 7 {
            mask |= 1 << (index - 6);
        }
        // 2 DOWN   + 1 RIGHT
        if file != 7 && index > 15 {
            mask |= 1 << (index - 15);
        }
        // 2 DOWN   + 1 LEFT
        if file != 0 && index > 15 {
            mask |= 1 << (index - 17);
        }
        // 1 DOWN   + 2 LEFT
        if file > 1 && index > 7 {
            mask |= 1 << (index - 10);
        }
        table[index] = mask;
        index += 1;
    }
    table
}

const fn gen_king_moves() -> [u64; 64] {
    let mut table = [0; 64];
    let mut index = 0;
    while index < 64 {
        let mut mask: u64 = 0;
        let file = index % 8;
        // LEFT
        if file != 0 {
            mask |= 1 << (index - 1);
        }
        // RIGHT
        if file != 7 {
            mask |= 1 << (index + 1);
        }
        // UP
        if index < 56 {
            mask |= 1 << (index + 8);
        }
        // DOWN
        if index > 7 {
            mask |= 1 << (index - 8);
        }
        // LEFT UP
        if file != 0 && index < 56 {
            mask |= 1 << (index + 7);
        }
        // LEFT DOWN
        if file != 0 && index > 7 {
            mask |= 1 << (index - 9);
        }
        // RIGHT DOWN
        if file != 7 && index > 7 {
            mask |= 1 << (index - 7);
        }
        // RIGHT UP
        if file != 7 && index < 56 {
            mask |= 1 << (index + 9);
        }
        table[index] = mask;
        index += 1;
    }
    table
}

const fn gen_pawn_attacks() -> [[u64; 64]; 2] {
    let mut table = [[0; 64]; 2];

    // White pawn attacks
    let mut i = 0;
    while i < 56 {
        let mut bb: u64 = 0;
        if i % 8 != 0 {
            bb |= 1 << (i + 7);
        }
        if i % 8 != 7 {
            bb |= 1 << (i + 9);
        }
        table[0][i as usize] = bb;
        i += 1;
    }

    // Black pawn attacks
    i = 8;
    while i < 64 {
        let mut bb: u64 = 0;
        if i % 8 != 0 {
            bb |= 1 << (i - 9);
        }
        if i % 8 != 7 {
            bb |= 1 << (i - 7);
        }
        table[1][i as usize] = bb;
        i += 1;
    }
    table
}

const fn gen_between_and_line_bbs() -> ([[u64; 64]; 64], [[u64; 64]; 64]) {
    let mut between_squares = [[0; 64]; 64];
    let mut line_bitboards = [[0; 64]; 64];
    let mut i = 0;
    while i < 64 {
        let mut j = 0;
        while j < 64 {
            let i_bb = 1_u64 << i;
            let j_bb = 1_u64 << j;
            if sliding_attack(&ROOK_DELTAS, i, 0) & j_bb != 0 {
                line_bitboards[i as usize][j as usize] |= (sliding_attack(&ROOK_DELTAS, j, 0)
                    & sliding_attack(&ROOK_DELTAS, i, 0))
                    | i_bb
                    | j_bb;
                between_squares[i as usize][j as usize] =
                    sliding_attack(&ROOK_DELTAS, j, i_bb) & sliding_attack(&ROOK_DELTAS, i, j_bb);
            } else if sliding_attack(&BISHOP_DELTAS, i, 0) & j_bb != 0 {
                line_bitboards[i as usize][j as usize] |= (sliding_attack(&BISHOP_DELTAS, j, 0)
                    & sliding_attack(&BISHOP_DELTAS, i, 0))
                    | i_bb
                    | j_bb;
                between_squares[i as usize][j as usize] = sliding_attack(&BISHOP_DELTAS, j, i_bb)
                    & sliding_attack(&BISHOP_DELTAS, i, j_bb);
            }
            j += 1;
        }
        i += 1;
    }
    (between_squares, line_bitboards)
}

/// Pawn attacks `Bitboard` from a given square and player.
/// E.g. given square e6 and player Black, returns the
/// Bitboard of squares d5 and f5.
#[inline(always)]
pub fn pawn_attacks_from(sq: Square, player: Player) -> u64 {
    debug_assert!(sq.is_okay());
    unsafe {
        *tables()
            .pawn_attacks
            .get_unchecked(player.inner() as usize)
            .get_unchecked(sq.0 as usize)
    }
}

/// Get the line (diagonal / file / rank) `BitBoard` that two squares both exist on, if it exists.
#[inline(always)]
pub fn line_bb(sq_one: Square, sq_two: Square) -> u64 {
    debug_assert!(sq_one.is_okay());
    debug_assert!(sq_two.is_okay());
    unsafe {
        *tables()
            .lines
            .get_unchecked(sq_one.0 as usize)
            .get_unchecked(sq_two.0 as usize)
    }
}

/// Get the line (diagonal / file / rank) `BitBoard` between two squares, not including the squares, if it exists.
#[inline(always)]
pub fn between_bb(sq_one: Square, sq_two: Square) -> u64 {
    debug_assert!(sq_one.is_okay());
    debug_assert!(sq_two.is_okay());
    unsafe {
        *tables()
            .between
            .get_unchecked(sq_one.0 as usize)
            .get_unchecked(sq_two.0 as usize)
    }
}

/// Returns if three Squares are in the same diagonal, file, or rank.
#[inline(always)]
pub fn aligned(s1: Square, s2: Square, s3: Square) -> bool {
    (line_bb(s1, s2) & u8_to_u64(s3.0)) != 0
}
