use crate::bb::Bitboard;
use num_enum::TryFromPrimitive;
use std::convert::TryFrom;
use std::fmt;

#[derive(Copy, Clone)]
pub enum Color {
    White,
    Black,
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Color::White => write!(f, "White"),
            Color::Black => write!(f, "Black"),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, TryFromPrimitive)]
#[repr(u8)]
pub enum Square {
    A1 = 0,
    B1,
    C1,
    D1,
    E1,
    F1,
    G1,
    H1,
    A2,
    B2,
    C2,
    D2,
    E2,
    F2,
    G2,
    H2,
    A3,
    B3,
    C3,
    D3,
    E3,
    F3,
    G3,
    H3,
    A4,
    B4,
    C4,
    D4,
    E4,
    F4,
    G4,
    H4,
    A5,
    B5,
    C5,
    D5,
    E5,
    F5,
    G5,
    H5,
    A6,
    B6,
    C6,
    D6,
    E6,
    F6,
    G6,
    H6,
    A7,
    B7,
    C7,
    D7,
    E7,
    F7,
    G7,
    H7,
    A8,
    B8,
    C8,
    D8,
    E8,
    F8,
    G8,
    H8,
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Square::A1 => write!(f, "a1"),
            Square::B1 => write!(f, "b1"),
            Square::C1 => write!(f, "c1"),
            Square::D1 => write!(f, "d1"),
            Square::E1 => write!(f, "e1"),
            Square::F1 => write!(f, "f1"),
            Square::G1 => write!(f, "g1"),
            Square::H1 => write!(f, "h1"),

            Square::A2 => write!(f, "a2"),
            Square::B2 => write!(f, "b2"),
            Square::C2 => write!(f, "c2"),
            Square::D2 => write!(f, "d2"),
            Square::E2 => write!(f, "e2"),
            Square::F2 => write!(f, "f2"),
            Square::G2 => write!(f, "g2"),
            Square::H2 => write!(f, "h2"),

            Square::A3 => write!(f, "a3"),
            Square::B3 => write!(f, "b3"),
            Square::C3 => write!(f, "c3"),
            Square::D3 => write!(f, "d3"),
            Square::E3 => write!(f, "e3"),
            Square::F3 => write!(f, "f3"),
            Square::G3 => write!(f, "g3"),
            Square::H3 => write!(f, "h3"),

            Square::A4 => write!(f, "a4"),
            Square::B4 => write!(f, "b4"),
            Square::C4 => write!(f, "c4"),
            Square::D4 => write!(f, "d4"),
            Square::E4 => write!(f, "e4"),
            Square::F4 => write!(f, "f4"),
            Square::G4 => write!(f, "g4"),
            Square::H4 => write!(f, "h4"),

            Square::A5 => write!(f, "a5"),
            Square::B5 => write!(f, "b5"),
            Square::C5 => write!(f, "c5"),
            Square::D5 => write!(f, "d5"),
            Square::E5 => write!(f, "e5"),
            Square::F5 => write!(f, "f5"),
            Square::G5 => write!(f, "g5"),
            Square::H5 => write!(f, "h5"),

            Square::A6 => write!(f, "a6"),
            Square::B6 => write!(f, "b6"),
            Square::C6 => write!(f, "c6"),
            Square::D6 => write!(f, "d6"),
            Square::E6 => write!(f, "e6"),
            Square::F6 => write!(f, "f6"),
            Square::G6 => write!(f, "g6"),
            Square::H6 => write!(f, "h6"),

            Square::A7 => write!(f, "a7"),
            Square::B7 => write!(f, "b7"),
            Square::C7 => write!(f, "c7"),
            Square::D7 => write!(f, "d7"),
            Square::E7 => write!(f, "e7"),
            Square::F7 => write!(f, "f7"),
            Square::G7 => write!(f, "g7"),
            Square::H7 => write!(f, "h7"),

            Square::A8 => write!(f, "a8"),
            Square::B8 => write!(f, "b8"),
            Square::C8 => write!(f, "c8"),
            Square::D8 => write!(f, "d8"),
            Square::E8 => write!(f, "e8"),
            Square::F8 => write!(f, "f8"),
            Square::G8 => write!(f, "g8"),
            Square::H8 => write!(f, "h8"),
        }
    }
}

impl Square {
    pub fn to_idx(&self) -> Option<u32> {
        Some(*self as u32)
    }

    pub fn to_bb(&self) -> Option<Bitboard> {
        match self.to_idx() {
            Some(idx) => Some(Bitboard::from_sq_idx(idx as u8)),
            None => None,
        }
    }

    pub fn from_idx(idx: u16) -> Self {
        Self::try_from(idx as u8).expect(&format!(
            "can only create a Square from an index in 0-63; found {}",
            idx
        ))
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
}

#[derive(Copy, Clone)]
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

#[derive(Copy, Clone)]
pub enum PieceType {
    None,
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

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
    pub(crate) turn: Color,
    pub(crate) castling_rights: CastlingRights,
    pub(crate) ep_square: Option<Square>,
    pub(crate) half_move_clock: u32,
    pub(crate) move_number: u32,
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
