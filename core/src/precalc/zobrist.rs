use super::prng::PRNG;
use crate::position::CastlingRights;
use crate::position::{Piece, Player, Square};

static ZOBRIST_KEYS: ZobristKeys = ZobristKeys::new();

const SEEDS: [u64; 4] = [10_123, 43_292_194, 19_023_734, 32_336];

struct ZobristKeys {
    piece_square: [[u64; 13]; 64],
    side_to_move: [u64; 2],
    side_to_move_toggler: u64,
    castling_rights: [u64; 16],
    ep_file: [u64; 8],
}

impl ZobristKeys {
    const fn new() -> Self {
        let piece_square = gen_piece_square_keys();
        let side_to_move = gen_side_to_move_keys();
        Self {
            piece_square,
            side_to_move,
            side_to_move_toggler: side_to_move[0] ^ side_to_move[1],
            castling_rights: gen_castling_rights_keys(),
            ep_file: gen_ep_file_keys(),
        }
    }
}

#[inline(always)]
fn keys() -> &'static ZobristKeys {
    &ZOBRIST_KEYS
}

const fn gen_piece_square_keys() -> [[u64; 13]; 64] {
    let mut rng = PRNG::init(SEEDS[0]);
    let mut keys = [[0; 13]; 64];

    let mut square = 0;
    while square < keys.len() {
        let mut piece = 0;
        while piece < keys[square].len() {
            keys[square][piece] = rng.rand();
            piece += 1;
        }
        square += 1;
    }
    keys
}

const fn gen_side_to_move_keys() -> [u64; 2] {
    let mut rng = PRNG::init(SEEDS[1]);
    [rng.rand(), rng.rand()]
}

const fn gen_castling_rights_keys() -> [u64; 16] {
    let mut rng = PRNG::init(SEEDS[2]);
    let mut keys = [0; 16];

    let mut index = 0;
    while index < keys.len() {
        keys[index] = rng.rand();
        index += 1;
    }
    keys
}

const fn gen_ep_file_keys() -> [u64; 8] {
    let mut rng = PRNG::init(SEEDS[3]);
    let mut keys = [0; 8];

    let mut index = 0;
    while index < keys.len() {
        keys[index] = rng.rand();
        index += 1;
    }
    keys
}

#[inline(always)]
pub fn piece_square_key(piece: Piece, square: Square) -> u64 {
    debug_assert!(square.is_okay());
    unsafe {
        *keys()
            .piece_square
            .get_unchecked(square.0 as usize)
            .get_unchecked(piece as usize)
    }
}

#[inline(always)]
pub fn side_to_move_key(turn: Player) -> u64 {
    unsafe { *keys().side_to_move.get_unchecked(turn.inner() as usize) }
}

#[inline(always)]
pub fn side_to_move_toggler() -> u64 {
    keys().side_to_move_toggler
}

#[inline(always)]
pub fn castling_rights_keys(castling_rights: CastlingRights) -> u64 {
    debug_assert!(castling_rights.is_okay());
    unsafe {
        *keys()
            .castling_rights
            .get_unchecked(castling_rights.bits() as usize)
    }
}

#[inline(always)]
pub fn ep_file_keys(sq: Square) -> u64 {
    debug_assert!(sq.is_okay());
    unsafe { *keys().ep_file.get_unchecked(sq.rank() as usize) }
}
