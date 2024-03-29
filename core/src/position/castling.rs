use crate::position::Square;
use bitflags::bitflags;
use std::fmt;

bitflags! {
    pub struct CastlingRights: u8 {
        const WHITE_QUEENSIDE = 0b0001;
        const WHITE_KINGSIDE  = 0b0010;
        const BLACK_QUEENSIDE = 0b0100;
        const BLACK_KINGSIDE  = 0b1000;
    }
}

impl CastlingRights {
    /// Build a new `CastlingRights` struct with the passed values.
    pub fn new(wk: bool, wq: bool, bk: bool, bq: bool) -> Self {
        let mut cr = Self::empty();
        cr.set_wk(wk);
        cr.set_wq(wq);
        cr.set_bk(bk);
        cr.set_bq(bq);
        cr
    }

    /// Build a new `CastlingRights` struct with no castling rights.
    pub fn none() -> Self {
        CastlingRights::new(false, false, false, false)
    }

    /// Used for `debug_assert!` calls to ensure that `self` is
    /// between 0 and 15 (since the underlying type is u8).
    pub fn is_okay(&self) -> bool {
        self.bits() < 16
    }

    #[inline(always)]
    pub fn white_kingside(&self) -> bool {
        self.contains(Self::WHITE_KINGSIDE)
    }

    #[inline(always)]
    pub fn white_queenside(&self) -> bool {
        self.contains(Self::WHITE_QUEENSIDE)
    }

    #[inline(always)]
    pub fn black_kingside(&self) -> bool {
        self.contains(Self::BLACK_KINGSIDE)
    }

    #[inline(always)]
    pub fn black_queenside(&self) -> bool {
        self.contains(Self::BLACK_QUEENSIDE)
    }

    #[inline(always)]
    pub fn set_wk(&mut self, value: bool) {
        self.set(Self::WHITE_KINGSIDE, value);
    }

    #[inline(always)]
    pub fn set_wq(&mut self, value: bool) {
        self.set(Self::WHITE_QUEENSIDE, value);
    }

    #[inline(always)]
    pub fn set_bk(&mut self, value: bool) {
        self.set(Self::BLACK_KINGSIDE, value);
    }

    #[inline(always)]
    pub fn set_bq(&mut self, value: bool) {
        self.set(Self::BLACK_QUEENSIDE, value);
    }

    pub fn update(&self, from: Square) -> Self {
        let mut new_cr = self.clone();
        let flags_to_turn_off = match from {
            Square::A1 => Self::WHITE_QUEENSIDE,
            Square::E1 => Self::WHITE_QUEENSIDE | Self::WHITE_KINGSIDE,
            Square::H1 => Self::WHITE_KINGSIDE,
            Square::A8 => Self::BLACK_QUEENSIDE,
            Square::E8 => Self::BLACK_QUEENSIDE | Self::BLACK_KINGSIDE,
            Square::H8 => Self::BLACK_KINGSIDE,
            _ => Self::empty(),
        };

        new_cr &= !flags_to_turn_off;
        new_cr
    }
}

impl fmt::Display for CastlingRights {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            write!(f, "-")
        } else {
            let mut white = Vec::new();
            let mut black = Vec::new();
            if self.white_kingside() {
                white.push("K")
            }
            if self.white_queenside() {
                white.push("Q")
            }
            if self.black_kingside() {
                black.push("k")
            }
            if self.black_queenside() {
                black.push("q")
            }
            write!(f, "{}{}", white.join(""), black.join(""))
        }
    }
}

/// Types of castling.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CastleType {
    Kingside = 0,
    Queenside = 1,
}
