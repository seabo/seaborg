#![feature(maybe_uninit_slice)]

#[macro_use]
mod macros;

mod bit_twiddles;
mod masks;
mod precalc;

pub mod bb;
pub mod init;
pub mod mono_traits;
pub mod mov;
pub mod movegen;
pub mod movelist;
pub mod position;

pub use mono_traits::{
    AllGenType, BishopType, BlackType, CapturesGenType, EvasionsGenType, GenTypeTrait, KingType,
    KnightType, NonEvasionsGenType, PawnType, PieceTrait, PlayerTrait, QueenType,
    QuietChecksGenType, QuietsGenType, RookType, WhiteType,
};
