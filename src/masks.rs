use crate::position::Square;

/// The number of players in a chess game.
pub const PLAYER_CNT: usize = 2;
/// The total number of files on a chessboard.
pub const FILE_CNT: usize = 8;
/// The total number of ranks on a chessboard.
pub const RANK_CNT: usize = 8;
/// The number of directions available for castling.
pub const CASTLING_SIDES: usize = 2;

/// Bit representation of all squares.
pub const ALL: u64 = 0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111111;

/// Bit representation of file A.
pub const FILE_A: u64 = 0b00000001_00000001_00000001_00000001_00000001_00000001_00000001_00000001;
/// Bit representation of file B.
pub const FILE_B: u64 = 0b00000010_00000010_00000010_00000010_00000010_00000010_00000010_00000010;
/// Bit representation of file C.
pub const FILE_C: u64 = 0b00000100_00000100_00000100_00000100_00000100_00000100_00000100_00000100;
/// Bit representation of file D.
pub const FILE_D: u64 = 0b00001000_00001000_00001000_00001000_00001000_00001000_00001000_00001000;
/// Bit representation of file E.
pub const FILE_E: u64 = 0b00010000_00010000_00010000_00010000_00010000_00010000_00010000_00010000;
/// Bit representation of file F.
pub const FILE_F: u64 = 0b00100000_00100000_00100000_00100000_00100000_00100000_00100000_00100000;
/// Bit representation of file H.
pub const FILE_G: u64 = 0b01000000_01000000_01000000_01000000_01000000_01000000_01000000_01000000;
/// Bit representation of file G.
pub const FILE_H: u64 = 0b10000000_10000000_10000000_10000000_10000000_10000000_10000000_10000000;

/// Bit representation of rank 1.
pub const RANK_1: u64 = 0x0000_0000_0000_00FF;
/// Bit representation of rank 2.
pub const RANK_2: u64 = 0x0000_0000_0000_FF00;
/// Bit representation of rank 3.
pub const RANK_3: u64 = 0x0000_0000_00FF_0000;
/// Bit representation of rank 4.
pub const RANK_4: u64 = 0x0000_0000_FF00_0000;
/// Bit representation of rank 5.
pub const RANK_5: u64 = 0x0000_00FF_0000_0000;
/// Bit representation of rank 6.
pub const RANK_6: u64 = 0x0000_FF00_0000_0000;
/// Bit representation of rank 7.
pub const RANK_7: u64 = 0x00FF_0000_0000_0000;
/// Bit representation of rank 8.
pub const RANK_8: u64 = 0xFF00_0000_0000_0000;

/// Array of all files and their corresponding bits, indexed from
/// file A to file H.
pub static FILE_BB: [u64; FILE_CNT] = [
    FILE_A, FILE_B, FILE_C, FILE_D, FILE_E, FILE_F, FILE_G, FILE_H,
];

/// Array of all ranks and their corresponding bits, indexed from
/// rank 1 to rank 8.
pub static RANK_BB: [u64; RANK_CNT] = [
    RANK_1, RANK_2, RANK_3, RANK_4, RANK_5, RANK_6, RANK_7, RANK_8,
];

/// Bits representing the castling path for a white king-side castle.
pub const CASTLING_PATH_WHITE_K_SIDE: u64 =
    (1 as u64) << Square::F1.0 as u32 | (1 as u64) << Square::G1.0 as u32;
/// Bits representing the castling path for a white queen-side castle.
pub const CASTLING_PATH_WHITE_Q_SIDE: u64 = (1 as u64) << Square::B1.0 as u32
    | (1 as u64) << Square::C1.0 as u32
    | (1 as u64) << Square::D1.0 as u32;

/// Bits representing the castling path for a black king-side castle.
pub const CASTLING_PATH_BLACK_K_SIDE: u64 =
    (1 as u64) << Square::F8.0 as u32 | (1 as u64) << Square::G8.0 as u32;
/// Bits representing the castling path for a black queen-side castle.
pub const CASTLING_PATH_BLACK_Q_SIDE: u64 = (1 as u64) << Square::B8.0 as u32
    | (1 as u64) << Square::C8.0 as u32
    | (1 as u64) << Square::D8.0 as u32;

/// Array for the bits representing the castling path for a white castle, indexed
/// per the side available (king-side, queen-side).
pub static CASTLING_PATH_WHITE: [u64; CASTLING_SIDES] =
    [CASTLING_PATH_WHITE_K_SIDE, CASTLING_PATH_WHITE_Q_SIDE];

/// Array for the bits representing the castling path for a white castle, indexed
/// per the side available (king-side, queen-side).
pub static CASTLING_PATH_BLACK: [u64; CASTLING_SIDES] =
    [CASTLING_PATH_BLACK_K_SIDE, CASTLING_PATH_BLACK_Q_SIDE];

/// Array for the bits representing the castling path for castle, indexed
/// per the side available (king-side, queen-side), as well as indexed per player.
pub static CASTLING_PATH: [[u64; CASTLING_SIDES]; PLAYER_CNT] = [
    [CASTLING_PATH_WHITE_K_SIDE, CASTLING_PATH_WHITE_Q_SIDE],
    [CASTLING_PATH_BLACK_K_SIDE, CASTLING_PATH_BLACK_Q_SIDE],
];

/// Starting square number of the black king-side rook.
pub const ROOK_BLACK_KSIDE_START: u8 = 63;
/// Starting square number of the black queen-side rook.
pub const ROOK_BLACK_QSIDE_START: u8 = 56;
/// Starting square number of the white king-side rook.
pub const ROOK_WHITE_KSIDE_START: u8 = 7;
/// Starting square number of the white queen-side rook.
pub const ROOK_WHITE_QSIDE_START: u8 = 0;

/// Castling right bit representing the white king-side castle is still possible.
pub const C_WHITE_K_MASK: u8 = 0b0000_1000;
/// Castling right bit representing the white queen-side castle is still possible.
pub const C_WHITE_Q_MASK: u8 = 0b0000_0100;
/// Castling right bit representing the black king-side castle is still possible.
pub const C_BLACK_K_MASK: u8 = 0b0000_0010;
/// Castling right bit representing the black queen-side castle is still possible.
pub const C_BLACK_Q_MASK: u8 = 0b0000_0001;

/// Array containing all the starting rook positions for each side, for each player.
pub static CASTLING_ROOK_START: [[u8; CASTLING_SIDES]; PLAYER_CNT] = [
    [ROOK_WHITE_KSIDE_START, ROOK_WHITE_QSIDE_START],
    [ROOK_BLACK_KSIDE_START, ROOK_BLACK_QSIDE_START],
];
