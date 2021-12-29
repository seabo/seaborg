use crate::position::Square;
use std::fmt;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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
    /// Return a `CastlingRights` struct representing no castling rights
    /// for either player.
    pub fn none() -> Self {
        Self {
            white_kingside: false,
            white_queenside: false,
            black_kingside: false,
            black_queenside: false,
        }
    }

    /// Return a new `CastlingRights` struct based on the current one, and
    /// a `Square` representing the origin an arbitrary move. This is the only
    /// information needed assuming it is a legal move, because loss of castling
    /// rights only occurs when either the King or Rook is moved from its starting
    /// square.
    pub fn update(&self, from: Square) -> Self {
        let mut new_castling_rights = self.clone();

        new_castling_rights.white_queenside =
            self.white_queenside && (from != Square::E1) && (from != Square::A1);
        new_castling_rights.white_kingside =
            self.white_kingside && (from != Square::E1) && (from != Square::H1);
        new_castling_rights.black_queenside =
            self.black_queenside && (from != Square::E8) && (from != Square::A8);
        new_castling_rights.black_kingside =
            self.black_kingside && (from != Square::E8) && (from != Square::H8);

        new_castling_rights
    }
}

/// Types of castling.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CastleType {
    Kingside = 0,
    Queenside = 1,
}
