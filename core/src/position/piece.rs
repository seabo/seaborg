use super::Player;
use std::fmt;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Piece {
    None = 0,
    WhitePawn = 1,
    WhiteKnight = 2,
    WhiteBishop = 3,
    WhiteRook = 4,
    WhiteQueen = 5,
    WhiteKing = 6,
    BlackPawn = 7,
    BlackKnight = 8,
    BlackBishop = 9,
    BlackRook = 10,
    BlackQueen = 11,
    BlackKing = 12,
}

impl Piece {
    /// Returns the type of the given piece.
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

    /// Returns the player of the given piece.
    ///
    /// # Panics
    ///
    /// Panics if the given `Piece` is `Piece::None`. This function
    /// should only be used when the `Piece` is guaranteed to not be
    /// `Piece::None`.
    pub fn player(&self) -> Player {
        match *self {
            Piece::None => panic!(),
            Piece::WhitePawn => Player::White,
            Piece::WhiteKnight => Player::White,
            Piece::WhiteBishop => Player::White,
            Piece::WhiteRook => Player::White,
            Piece::WhiteQueen => Player::White,
            Piece::WhiteKing => Player::White,
            Piece::BlackPawn => Player::Black,
            Piece::BlackKnight => Player::Black,
            Piece::BlackBishop => Player::Black,
            Piece::BlackRook => Player::Black,
            Piece::BlackQueen => Player::Black,
            Piece::BlackKing => Player::Black,
        }
    }

    /// Return a `Piece` from a `Player` and a `PieceType`.
    pub fn make(player: Player, piece_type: PieceType) -> Self {
        match player {
            Player::White => match piece_type {
                PieceType::None => Piece::None,
                PieceType::Pawn => Piece::WhitePawn,
                PieceType::Knight => Piece::WhiteKnight,
                PieceType::Bishop => Piece::WhiteBishop,
                PieceType::Rook => Piece::WhiteRook,
                PieceType::Queen => Piece::WhiteQueen,
                PieceType::King => Piece::WhiteKing,
            },
            Player::Black => match piece_type {
                PieceType::None => Piece::None,
                PieceType::Pawn => Piece::BlackPawn,
                PieceType::Knight => Piece::BlackKnight,
                PieceType::Bishop => Piece::BlackBishop,
                PieceType::Rook => Piece::BlackRook,
                PieceType::Queen => Piece::BlackQueen,
                PieceType::King => Piece::BlackKing,
            },
        }
    }

    /// Returns a tuple containing the `Player` and `PieceType` of the `Piece`.
    pub fn player_piece(&self) -> (Player, PieceType) {
        (self.player(), self.type_of())
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PieceType {
    None = 0,
    Pawn = 1,
    Knight = 2,
    Bishop = 3,
    Rook = 4,
    Queen = 5,
    King = 6,
}

pub const PROMO_PIECES: [PieceType; 4] = [
    PieceType::Knight,
    PieceType::Bishop,
    PieceType::Rook,
    PieceType::Queen,
];

impl PieceType {
    pub fn is_none(&self) -> bool {
        *self == PieceType::None
    }

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

impl fmt::Display for Piece {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Piece::None => write!(f, " "),
            Piece::WhitePawn => write!(f, "???"),
            Piece::WhiteKnight => write!(f, "???"),
            Piece::WhiteBishop => write!(f, "???"),
            Piece::WhiteRook => write!(f, "???"),
            Piece::WhiteQueen => write!(f, "???"),
            Piece::WhiteKing => write!(f, "???"),
            Piece::BlackPawn => write!(f, "??????"),
            Piece::BlackKnight => write!(f, "???"),
            Piece::BlackBishop => write!(f, "???"),
            Piece::BlackRook => write!(f, "???"),
            Piece::BlackQueen => write!(f, "???"),
            Piece::BlackKing => write!(f, "???"),
        }
    }
}
