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
    pub fn new(wk: bool, wq: bool, bk: bool, bq: bool) -> Self {
        let mut cr = Self::empty();
        cr.set_wk(wk);
        cr.set_wq(wq);
        cr.set_bk(bk);
        cr.set_bq(bq);
        cr
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
        let mut white = Vec::new();
        let mut black = Vec::new();
        if self.white_kingside() {
            white.push("kingside")
        }
        if self.white_queenside() {
            white.push("queenside")
        }
        if self.black_kingside() {
            black.push("kingside")
        }
        if self.black_queenside() {
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

/// Types of castling.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CastleType {
    Kingside = 0,
    Queenside = 1,
}
