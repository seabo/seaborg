use crate::bb::Bitboard;
use crate::position::Square;

/// Fast lookup table for Knight moves
static mut KING_TABLE: [u64; 64] = [0; 64];
/// Fast lookup table for King moves
static mut KNIGHT_TABLE: [u64; 64] = [0; 64];

#[cold]
pub fn init_boards() {
    unsafe {
        gen_knight_moves();
        gen_king_moves();
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
