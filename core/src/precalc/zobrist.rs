use super::prng::PRNG;
use crate::position::CastlingRights;
use crate::position::{Piece, Player, Square};

/// Keys indexed by square and piece type
static mut PIECE_SQUARE_KEYS: [[u64; 13]; 64] = [[0; 13]; 64];
static mut SIDE_TO_MOVE_KEYS: [u64; 2] = [0; 2];
static mut SIDE_TO_MOVE_TOGGLER: u64 = 0;
static mut CASTLING_RIGHTS_KEYS: [u64; 16] = [0; 16];
static mut EP_FILE_KEYS: [u64; 8] = [0; 8];

const SEEDS: [u64; 4] = [10_123, 43_292_194, 19_023_734, 32_336];

#[cold]
pub fn init_zobrist() {
    unsafe {
        gen_piece_square_keys();
        gen_side_to_move_keys();
        gen_castling_rights_keys();
        gen_ep_file_keys();
    }
}

#[cold]
unsafe fn gen_piece_square_keys() {
    let mut rng = PRNG::init(SEEDS[0]);

    for square in PIECE_SQUARE_KEYS.iter_mut() {
        for piece_square in square {
            *piece_square = rng.rand();
        }
    }
}

#[cold]
unsafe fn gen_side_to_move_keys() {
    let mut rng = PRNG::init(SEEDS[1]);

    SIDE_TO_MOVE_KEYS[0] = rng.rand();
    SIDE_TO_MOVE_KEYS[1] = rng.rand();

    SIDE_TO_MOVE_TOGGLER = SIDE_TO_MOVE_KEYS[0] ^ SIDE_TO_MOVE_KEYS[1];
}

#[cold]
unsafe fn gen_castling_rights_keys() {
    let mut rng = PRNG::init(SEEDS[2]);

    for spot in CASTLING_RIGHTS_KEYS.iter_mut() {
        *spot = rng.rand();
    }
}

#[cold]
unsafe fn gen_ep_file_keys() {
    let mut rng = PRNG::init(SEEDS[3]);

    for spot in EP_FILE_KEYS.iter_mut() {
        *spot = rng.rand();
    }
}

#[inline(always)]
pub fn piece_square_key(piece: Piece, square: Square) -> u64 {
    debug_assert!(square.is_okay());
    unsafe {
        *PIECE_SQUARE_KEYS
            .get_unchecked(square.0 as usize)
            .get_unchecked(piece as usize)
    }
}

#[inline(always)]
pub fn side_to_move_key(turn: Player) -> u64 {
    unsafe { *SIDE_TO_MOVE_KEYS.get_unchecked(turn.inner() as usize) }
}

#[inline(always)]
pub fn side_to_move_toggler() -> u64 {
    unsafe { SIDE_TO_MOVE_TOGGLER }
}

#[inline(always)]
pub fn castling_rights_keys(castling_rights: CastlingRights) -> u64 {
    debug_assert!(castling_rights.is_okay());
    unsafe { *CASTLING_RIGHTS_KEYS.get_unchecked(castling_rights.bits() as usize) }
}

#[inline(always)]
pub fn ep_file_keys(sq: Square) -> u64 {
    debug_assert!(sq.is_okay());
    unsafe { *EP_FILE_KEYS.get_unchecked(sq.rank() as usize) }
}
