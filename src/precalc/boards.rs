use super::magic::{bishop_attacks, rook_attacks};
use crate::bb::Bitboard;
use crate::position::{file_of_sq, u8_to_u64, Player, Square};

/// Fast lookup table for Knight moves
static mut KING_TABLE: [u64; 64] = [0; 64];
/// Fast lookup table for King moves
static mut KNIGHT_TABLE: [u64; 64] = [0; 64];
/// Fast lookup table for Pawn attacks
static mut PAWN_ATTACKS_FROM: [[u64; 64]; 2] = [[0; 64]; 2];
/// Fast lookup line bitboards for any two squares.
static mut LINE_BITBOARD: [[u64; 64]; 64] = [[0; 64]; 64];
/// Fast lookup bitboards for the squares between any two squares.
static mut BETWEEN_SQUARES_BB: [[u64; 64]; 64] = [[0; 64]; 64];

#[cold]
pub fn init_boards() {
    unsafe {
        gen_knight_moves();
        gen_king_moves();
        gen_pawn_attacks();
        gen_between_and_line_bbs();
    }
}

/// Generate Knight moves Bitboard from an origin square
#[inline(always)]
pub fn knight_moves(square: Square) -> Bitboard {
    debug_assert!(square.is_okay());
    unsafe { Bitboard::new(*KNIGHT_TABLE.get_unchecked(square.0 as usize)) }
}

/// Generate King moves Bitboard from an origin square
#[inline(always)]
pub fn king_moves(square: Square) -> Bitboard {
    debug_assert!(square.is_okay());
    unsafe { Bitboard::new(*KING_TABLE.get_unchecked(square.0 as usize)) }
}

#[cold]
unsafe fn gen_knight_moves() {
    for (index, spot) in KNIGHT_TABLE.iter_mut().enumerate() {
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
        *spot = mask;
    }
}

#[cold]
unsafe fn gen_king_moves() {
    for index in 0..64 {
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
        KING_TABLE[index] = mask;
    }
}

#[cold]
unsafe fn gen_pawn_attacks() {
    // White pawn attacks
    for i in 0..56 as u8 {
        let mut bb: u64 = 0;
        if file_of_sq(i) != 0 {
            bb |= u8_to_u64(i + 7)
        }
        if file_of_sq(i) != 7 {
            bb |= u8_to_u64(i + 9)
        }
        PAWN_ATTACKS_FROM[0][i as usize] = bb;
    }

    // Black pawn attacks
    for i in 8..64 as u8 {
        let mut bb: u64 = 0;
        if file_of_sq(i) != 0 {
            bb |= u8_to_u64(i - 9)
        }
        if file_of_sq(i) != 7 {
            bb |= u8_to_u64(i - 7)
        }
        PAWN_ATTACKS_FROM[1][i as usize] = bb;
    }
}

#[cold]
unsafe fn gen_between_and_line_bbs() {
    for i in 0..64 as u8 {
        for j in 0..64 as u8 {
            let i_bb: u64 = (1 as u64) << i;
            let j_bb: u64 = (1 as u64) << j;
            if rook_attacks(0, i) & j_bb != 0 {
                LINE_BITBOARD[i as usize][j as usize] |=
                    (rook_attacks(0, j) & rook_attacks(0, i)) | i_bb | j_bb;
                BETWEEN_SQUARES_BB[i as usize][j as usize] =
                    rook_attacks(i_bb, j) & rook_attacks(j_bb, i);
            } else if bishop_attacks(0, i) & j_bb != 0 {
                LINE_BITBOARD[i as usize][j as usize] |=
                    (bishop_attacks(0, j) & bishop_attacks(0, i)) | i_bb | j_bb;
                BETWEEN_SQUARES_BB[i as usize][j as usize] =
                    bishop_attacks(i_bb, j) & bishop_attacks(j_bb, i);
            } else {
                LINE_BITBOARD[i as usize][j as usize] = 0;
                BETWEEN_SQUARES_BB[i as usize][j as usize] = 0;
            }
        }
    }
}

/// Pawn attacks `Bitboard` from a given square and player.
/// E.g. given square e6 and player Black, returns the
/// Bitboard of squares d5 and f5.
#[inline(always)]
pub fn pawn_attacks_from(sq: Square, player: Player) -> u64 {
    debug_assert!(sq.is_okay());
    unsafe {
        *PAWN_ATTACKS_FROM
            .get_unchecked(player as usize)
            .get_unchecked(sq.0 as usize)
    }
}

/// Get the line (diagonal / file / rank) `BitBoard` that two squares both exist on, if it exists.
#[inline(always)]
pub fn line_bb(sq_one: Square, sq_two: Square) -> u64 {
    debug_assert!(sq_one.is_okay());
    debug_assert!(sq_two.is_okay());
    unsafe { *(LINE_BITBOARD.get_unchecked(sq_one.0 as usize)).get_unchecked(sq_two.0 as usize) }
}

/// Get the line (diagonal / file / rank) `BitBoard` between two squares, not including the squares, if it exists.
#[inline(always)]
pub fn between_bb(sq_one: Square, sq_two: Square) -> u64 {
    debug_assert!(sq_one.is_okay());
    debug_assert!(sq_two.is_okay());
    unsafe {
        *(BETWEEN_SQUARES_BB.get_unchecked(sq_one.0 as usize)).get_unchecked(sq_two.0 as usize)
    }
}
