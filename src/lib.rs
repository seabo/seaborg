#[macro_use]
mod macros;

pub mod precalc;

pub mod bb;
pub mod eval;
pub mod mov;
pub mod movegen;
pub mod movelist;
pub mod position;
pub mod search;
pub mod tables;

mod bit_twiddles;
mod masks;
mod mono_traits;
