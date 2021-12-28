mod fen;

use crate::bb::Bitboard;
use crate::bit_twiddles::diff;
use crate::masks::{CASTLING_PATH, CASTLING_ROOK_START, FILE_BB, RANK_BB};
use crate::movegen::{bishop_moves, rook_moves};
use crate::precalc::boards::{king_moves, knight_moves, pawn_attacks_from};

use std::fmt;
use std::ops::*;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Player {
    White = 0,
    Black = 1,
}

impl Player {
    /// Returns the other player.
    pub fn other_player(&self) -> Self {
        match self {
            Player::White => Player::Black,
            Player::Black => Player::White,
        }
    }

    /// Returns the relative square from a given square.
    #[inline(always)]
    pub fn relative_square(self, sq: Square) -> Square {
        assert!(sq.is_okay());
        sq ^ Square((self) as u8 * 56)
    }
}

impl fmt::Display for Player {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Player::White => write!(f, "White"),
            Player::Black => write!(f, "Black"),
        }
    }
}

/// Represents a single square of a chess board.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct Square(pub u8);

impl_bit_ops!(Square, u8);

impl Square {
    #[inline]
    pub const fn is_okay(&self) -> bool {
        self.0 < 64
    }

    #[inline]
    pub fn distance(&self, other: Self) -> u8 {
        let x = diff(self.rank_idx_of_sq(), other.rank_idx_of_sq());
        let y = diff(self.file_idx_of_sq(), other.file_idx_of_sq());
        if x > y {
            x
        } else {
            y
        }
    }

    /// Returns the rank index (number) of a `SQ`.
    #[inline(always)]
    pub const fn rank_idx_of_sq(self) -> u8 {
        (self.0 >> 3) as u8
    }

    /// Returns the file index (number) of a `SQ`.
    #[inline(always)]
    pub const fn file_idx_of_sq(self) -> u8 {
        (self.0 & 0b0000_0111) as u8
    }

    /// Returns the rank that the square lies on.
    pub fn rank(self) -> u8 {
        (self.0 >> 3) & 0b0000_0111
    }

    /// Returns the file that the square lies on.
    pub fn file(self) -> u8 {
        self.0 & 0b0000_0111
    }
}

// constants
impl Square {
    pub const A1: Square = Square(0b000000);
    #[doc(hidden)]
    pub const B1: Square = Square(0b000001);
    #[doc(hidden)]
    pub const C1: Square = Square(0b000010);
    #[doc(hidden)]
    pub const D1: Square = Square(0b000011);
    #[doc(hidden)]
    pub const E1: Square = Square(0b000100);
    #[doc(hidden)]
    pub const F1: Square = Square(0b000101);
    #[doc(hidden)]
    pub const G1: Square = Square(0b000110);
    #[doc(hidden)]
    pub const H1: Square = Square(0b000111);
    #[doc(hidden)]
    pub const A2: Square = Square(0b001000);
    #[doc(hidden)]
    pub const B2: Square = Square(0b001001);
    #[doc(hidden)]
    pub const C2: Square = Square(0b001010);
    #[doc(hidden)]
    pub const D2: Square = Square(0b001011);
    #[doc(hidden)]
    pub const E2: Square = Square(0b001100);
    #[doc(hidden)]
    pub const F2: Square = Square(0b001101);
    #[doc(hidden)]
    pub const G2: Square = Square(0b001110);
    #[doc(hidden)]
    pub const H2: Square = Square(0b001111);
    #[doc(hidden)]
    pub const A3: Square = Square(0b010000);
    #[doc(hidden)]
    pub const B3: Square = Square(0b010001);
    #[doc(hidden)]
    pub const C3: Square = Square(0b010010);
    #[doc(hidden)]
    pub const D3: Square = Square(0b010011);
    #[doc(hidden)]
    pub const E3: Square = Square(0b010100);
    #[doc(hidden)]
    pub const F3: Square = Square(0b010101);
    #[doc(hidden)]
    pub const G3: Square = Square(0b010110);
    #[doc(hidden)]
    pub const H3: Square = Square(0b010111);
    #[doc(hidden)]
    pub const A4: Square = Square(0b011000);
    #[doc(hidden)]
    pub const B4: Square = Square(0b011001);
    #[doc(hidden)]
    pub const C4: Square = Square(0b011010);
    #[doc(hidden)]
    pub const D4: Square = Square(0b011011);
    #[doc(hidden)]
    pub const E4: Square = Square(0b011100);
    #[doc(hidden)]
    pub const F4: Square = Square(0b011101);
    #[doc(hidden)]
    pub const G4: Square = Square(0b011110);
    #[doc(hidden)]
    pub const H4: Square = Square(0b011111);
    #[doc(hidden)]
    pub const A5: Square = Square(0b100000);
    #[doc(hidden)]
    pub const B5: Square = Square(0b100001);
    #[doc(hidden)]
    pub const C5: Square = Square(0b100010);
    #[doc(hidden)]
    pub const D5: Square = Square(0b100011);
    #[doc(hidden)]
    pub const E5: Square = Square(0b100100);
    #[doc(hidden)]
    pub const F5: Square = Square(0b100101);
    #[doc(hidden)]
    pub const G5: Square = Square(0b100110);
    #[doc(hidden)]
    pub const H5: Square = Square(0b100111);
    #[doc(hidden)]
    pub const A6: Square = Square(0b101000);
    #[doc(hidden)]
    pub const B6: Square = Square(0b101001);
    #[doc(hidden)]
    pub const C6: Square = Square(0b101010);
    #[doc(hidden)]
    pub const D6: Square = Square(0b101011);
    #[doc(hidden)]
    pub const E6: Square = Square(0b101100);
    #[doc(hidden)]
    pub const F6: Square = Square(0b101101);
    #[doc(hidden)]
    pub const G6: Square = Square(0b101110);
    #[doc(hidden)]
    pub const H6: Square = Square(0b101111);
    #[doc(hidden)]
    pub const A7: Square = Square(0b110000);
    #[doc(hidden)]
    pub const B7: Square = Square(0b110001);
    #[doc(hidden)]
    pub const C7: Square = Square(0b110010);
    #[doc(hidden)]
    pub const D7: Square = Square(0b110011);
    #[doc(hidden)]
    pub const E7: Square = Square(0b110100);
    #[doc(hidden)]
    pub const F7: Square = Square(0b110101);
    #[doc(hidden)]
    pub const G7: Square = Square(0b110110);
    #[doc(hidden)]
    pub const H7: Square = Square(0b110111);
    #[doc(hidden)]
    pub const A8: Square = Square(0b111000);
    #[doc(hidden)]
    pub const B8: Square = Square(0b111001);
    #[doc(hidden)]
    pub const C8: Square = Square(0b111010);
    #[doc(hidden)]
    pub const D8: Square = Square(0b111011);
    #[doc(hidden)]
    pub const E8: Square = Square(0b111100);
    #[doc(hidden)]
    pub const F8: Square = Square(0b111101);
    #[doc(hidden)]
    pub const G8: Square = Square(0b111110);
    #[doc(hidden)]
    pub const H8: Square = Square(0b111111);
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Square(idx) = self;
        let rank = idx / 8 + 1;
        let file = idx % 8;

        let file_name = match file {
            0 => "a",
            1 => "b",
            2 => "c",
            3 => "d",
            4 => "e",
            5 => "f",
            6 => "g",
            7 => "h",
            _ => panic!(
                "error: square struct has idx {} and file index {}",
                idx, file
            ),
        };

        write!(f, "{}{}", file_name, rank.to_string())
    }
}

pub struct Board {
    pub arr: [Piece; 64],
}

impl Board {
    pub fn new() -> Self {
        Self {
            arr: [Piece::None; 64],
        }
    }

    pub fn from_array(board: [Piece; 64]) -> Self {
        Self { arr: board }
    }

    pub fn piece_at_sq(&self, sq: Square) -> Piece {
        assert!(sq.is_okay());
        unsafe { *self.arr.get_unchecked(sq.0 as usize) }
    }

    pub fn pretty_string(&self) -> String {
        let mut s = String::new();
        let mut squares: [[Piece; 8]; 8] = [[Piece::None; 8]; 8];

        for i in 0..64 {
            let rank = i / 8;
            let file = i % 8;
            squares[rank][file] = self.arr[i]
        }

        s.push_str("   ┌────────────────────────┐\n");
        for (i, row) in squares.iter().rev().enumerate() {
            s.push_str(&format!(" {} │", 8 - i));
            for square in row {
                s.push_str(&format!(" {} ", square));
            }
            s.push_str("│\n");
        }
        s.push_str("   └────────────────────────┘\n");
        s.push_str("     a  b  c  d  e  f  g  h \n");

        s
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pretty_string())
    }
}

#[derive(Copy, Clone, Debug)]
pub enum Piece {
    None,
    WhitePawn,
    WhiteKnight,
    WhiteBishop,
    WhiteRook,
    WhiteQueen,
    WhiteKing,
    BlackPawn,
    BlackKnight,
    BlackBishop,
    BlackRook,
    BlackQueen,
    BlackKing,
}

impl Piece {
    /// Returns the type of the given piece, without information about
    /// its colour.
    pub fn type_of(&self) -> PieceType {
        match *self {
            Piece::None => PieceType::None,
            Piece::WhitePawn => PieceType::Pawn,
            Piece::WhiteKnight => PieceType::Knight,
            Piece::WhiteBishop => PieceType::Bishop,
            Piece::WhiteRook => PieceType::Rook,
            Piece::WhiteQueen => PieceType::Queen,
            Piece::WhiteKing => PieceType::King,
            Piece::BlackPawn => PieceType::Pawn,
            Piece::BlackKnight => PieceType::Knight,
            Piece::BlackBishop => PieceType::Bishop,
            Piece::BlackRook => PieceType::Rook,
            Piece::BlackQueen => PieceType::Queen,
            Piece::BlackKing => PieceType::King,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PieceType {
    None,
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

pub const PROMO_PIECES: [PieceType; 4] = [
    PieceType::Knight,
    PieceType::Bishop,
    PieceType::Rook,
    PieceType::Queen,
];

impl PieceType {
    fn long_name(&self) -> &str {
        match self {
            PieceType::None => "none",
            PieceType::Pawn => "pawn",
            PieceType::Knight => "knight",
            PieceType::Bishop => "bishop",
            PieceType::Rook => "rook",
            PieceType::Queen => "queen",
            PieceType::King => "king",
        }
    }

    fn short_name(&self) -> &str {
        match self {
            PieceType::None => "-",
            PieceType::Pawn => "p",
            PieceType::Knight => "n",
            PieceType::Bishop => "b",
            PieceType::Rook => "r",
            PieceType::Queen => "q",
            PieceType::King => "k",
        }
    }
}

impl fmt::Display for PieceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(width) = f.width() {
            if width == 1 {
                write!(f, "{}", self.short_name())
            } else {
                write!(f, "{}", self.long_name())
            }
        } else {
            write!(f, "{}", self.long_name())
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct CastlingRights {
    pub white_queenside: bool,
    pub white_kingside: bool,
    pub black_queenside: bool,
    pub black_kingside: bool,
}

impl fmt::Display for CastlingRights {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut white = Vec::new();
        let mut black = Vec::new();
        if self.white_kingside {
            white.push("kingside")
        }
        if self.white_queenside {
            white.push("queenside")
        }
        if self.black_kingside {
            black.push("kingside")
        }
        if self.black_queenside {
            black.push("queenside")
        }
        if white.len() == 0 {
            white.push("none")
        }
        if black.len() == 0 {
            black.push("none")
        }
        let white_string = white.join(" + ");
        let black_string = black.join(" + ");

        write!(f, "White: {}, Black: {}", white_string, black_string)
    }
}

impl CastlingRights {
    pub fn none() -> Self {
        Self {
            white_kingside: false,
            white_queenside: false,
            black_kingside: false,
            black_queenside: false,
        }
    }
}

pub struct Position {
    // Array of pieces
    pub(crate) board: Board,

    // Bitboards for each piece type
    // TODO: should we switch to a scheme where the bitboards give all of each piece type
    // (i.e. white pawns and black pawns are all on one bitboard), and then we have a
    // white_pieces bb and black_pieces bb maintained separately? To get white_pawns, you would
    // do (pawns & white_pieces)
    pub(crate) no_piece: Bitboard,
    pub(crate) white_pawns: Bitboard,
    pub(crate) white_knights: Bitboard,
    pub(crate) white_bishops: Bitboard,
    pub(crate) white_rooks: Bitboard,
    pub(crate) white_queens: Bitboard,
    pub(crate) white_king: Bitboard,
    pub(crate) black_pawns: Bitboard,
    pub(crate) black_knights: Bitboard,
    pub(crate) black_bishops: Bitboard,
    pub(crate) black_rooks: Bitboard,
    pub(crate) black_queens: Bitboard,
    pub(crate) black_king: Bitboard,
    // Bitboards for each color
    pub(crate) white_pieces: Bitboard,
    pub(crate) black_pieces: Bitboard,

    // Piece counts
    pub(crate) white_piece_count: u8,
    pub(crate) black_piece_count: u8,

    // Other state
    turn: Player,
    pub(crate) castling_rights: CastlingRights,
    pub(crate) ep_square: Option<Square>,
    pub(crate) half_move_clock: u32,
    pub(crate) move_number: u32,
}

impl Position {
    // CHECKING

    /// Returns a `Bitboard` of possible attacks to a square with a given occupancy.
    /// Includes pieces from both players.
    pub fn attackers_to(&self, sq: Square, occupied: Bitboard) -> Bitboard {
        (Bitboard(pawn_attacks_from(sq, Player::Black))
            & self.piece_bb(Player::White, PieceType::Pawn))
            | (Bitboard(pawn_attacks_from(sq, Player::White)))
                & self.piece_bb(Player::Black, PieceType::Pawn)
            | (knight_moves(sq) & self.piece_bb_both_players(PieceType::Knight))
            | (rook_moves(occupied, sq)
                & (self.white_rooks | self.black_rooks | self.white_queens | self.black_queens))
            | (bishop_moves(occupied, sq)
                & (self.white_bishops | self.black_bishops | self.white_queens | self.black_queens))
            | (king_moves(sq) & (self.white_king | self.black_king))
    }

    /// Returns the combined Bitboard of both players for a given piece.
    #[inline(always)]
    pub fn piece_bb_both_players(&self, piece: PieceType) -> Bitboard {
        match piece {
            PieceType::None => Bitboard(0),
            PieceType::Pawn => self.white_pawns & self.black_pawns,
            PieceType::Knight => self.white_knights & self.black_knights,
            PieceType::Bishop => self.white_bishops & self.black_bishops,
            PieceType::Rook => self.white_rooks & self.black_rooks,
            PieceType::Queen => self.white_queens & self.black_queens,
            PieceType::King => self.white_king & self.black_king,
        }
    }

    #[inline]
    pub fn turn(&self) -> Player {
        self.turn
    }

    #[inline]
    pub fn occupied(&self) -> Bitboard {
        !self.no_piece
    }

    #[inline]
    pub fn get_occupied_player(&self, player: Player) -> Bitboard {
        match player {
            Player::White => self.white_pieces,
            Player::Black => self.black_pieces,
        }
    }

    #[inline]
    pub fn piece_bb(&self, player: Player, piece_type: PieceType) -> Bitboard {
        match player {
            Player::White => match piece_type {
                PieceType::None => Bitboard::ALL,
                PieceType::Pawn => self.white_pawns,
                PieceType::Knight => self.white_knights,
                PieceType::Bishop => self.white_bishops,
                PieceType::Rook => self.white_rooks,
                PieceType::Queen => self.white_queens,
                PieceType::King => self.white_king,
            },
            Player::Black => match piece_type {
                PieceType::None => Bitboard::ALL,
                PieceType::Pawn => self.black_pawns,
                PieceType::Knight => self.black_knights,
                PieceType::Bishop => self.black_bishops,
                PieceType::Rook => self.black_rooks,
                PieceType::Queen => self.black_queens,
                PieceType::King => self.black_king,
            },
        }
    }

    /// Returns the `Piece` at the given `Square`
    #[inline]
    pub fn piece_at_sq(&self, sq: Square) -> Piece {
        self.board.piece_at_sq(sq)
    }

    /// Return the en passant square for the current position (usually `None` except
    /// after a double pawn push.
    #[inline]
    pub fn ep_square(&self) -> Option<Square> {
        self.ep_square
    }

    /// Check if the castle path is impeded for the current player. Does not assume
    /// the current player has the ability to castle, whether by having castling-rights
    /// or having the rook and king be on the correct squares. Also does not check legality
    /// (i.e. ensuring none of the king squares are in check).
    #[inline]
    pub fn castle_impeded(&self, castle_type: CastleType) -> bool {
        let path = Bitboard(CASTLING_PATH[self.turn as usize][castle_type as usize]);
        (path & self.occupied()).is_not_empty()
    }

    /// Check if the given player can castle to the given side.
    #[inline]
    pub fn can_castle(&self, player: Player, side: CastleType) -> bool {
        if player == Player::White {
            if side == CastleType::Kingside {
                self.castling_rights.white_kingside
            } else {
                self.castling_rights.white_queenside
            }
        } else {
            if side == CastleType::Kingside {
                self.castling_rights.black_kingside
            } else {
                self.castling_rights.black_queenside
            }
        }
    }

    #[inline]
    pub fn castling_rook_square(&self, side: CastleType) -> Square {
        Square(CASTLING_ROOK_START[self.turn() as usize][side as usize])
    }

    /// Returns the king square for the given player.
    #[inline]
    pub fn king_sq(&self, player: Player) -> Square {
        self.piece_bb(player, PieceType::King).to_square()
    }
}

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Piece::None => write!(f, "."),
            Piece::WhitePawn => write!(f, "P"),
            Piece::WhiteKnight => write!(f, "N"),
            Piece::WhiteBishop => write!(f, "B"),
            Piece::WhiteRook => write!(f, "R"),
            Piece::WhiteQueen => write!(f, "Q"),
            Piece::WhiteKing => write!(f, "K"),
            Piece::BlackPawn => write!(f, "p"),
            Piece::BlackKnight => write!(f, "n"),
            Piece::BlackBishop => write!(f, "b"),
            Piece::BlackRook => write!(f, "r"),
            Piece::BlackQueen => write!(f, "q"),
            Piece::BlackKing => write!(f, "k"),
        }
    }
}

/// For whatever rank the bit (inner value of a `SQ`) is, returns the
/// corresponding rank as a u64.
#[inline(always)]
pub fn rank_bb(s: u8) -> u64 {
    RANK_BB[rank_idx_of_sq(s) as usize]
}

/// For whatever rank the bit (inner value of a `SQ`) is, returns the
/// corresponding `Rank` index.
#[inline(always)]
pub fn rank_idx_of_sq(s: u8) -> u8 {
    (s >> 3) as u8
}

/// For whatever file the bit (inner value of a `SQ`) is, returns the
/// corresponding file as a u64.
#[inline(always)]
pub fn file_bb(s: u8) -> u64 {
    FILE_BB[file_of_sq(s) as usize]
}

/// For whatever file the bit (inner value of a `Square`) is, returns the
/// corresponding file.
// TODO: make this return a dedicated `File` enum
#[inline(always)]
pub fn file_of_sq(s: u8) -> u8 {
    s & 0b0000_0111
}

/// Given a square (u8) that is valid, returns the bitboard representation
/// of that square.
///
/// # Safety
///
/// If the input is greater than 63, an empty u64 will be returned.
#[inline]
pub fn u8_to_u64(s: u8) -> u64 {
    debug_assert!(s < 64);
    (1 as u64).wrapping_shl(s as u32)
}

/// Types of castling.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CastleType {
    Kingside = 0,
    Queenside = 1,
}
