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
    All, BishopType, BlackType, Captures, Evasions, Generate, KingType, KnightType, NonEvasions,
    PawnType, PieceTrait, PlayerTrait, QueenType, Quiet, QuietChecks, RookType, WhiteType,
};
