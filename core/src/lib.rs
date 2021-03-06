#[macro_use]
mod macros;

mod bb;
mod bit_twiddles;
mod masks;
mod mono_traits;
mod movegen;
mod precalc;

pub mod init;
pub mod mov;
pub mod movelist;
pub mod position;

pub use mono_traits::{
    AllGenType, BishopType, BlackType, CapturesGenType, EvasionsGenType, GenTypeTrait, KingType,
    KnightType, NonEvasionsGenType, PawnType, PieceTrait, PlayerTrait, QueenType,
    QuietChecksGenType, QuietsGenType, RookType, WhiteType,
};
