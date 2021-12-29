#[macro_use]
mod macros;

pub mod precalc;

pub mod bb;
pub mod mov;
pub mod movegen;
pub mod movelist;
pub mod position;
pub mod search;

mod bit_twiddles;
mod masks;
mod mono_traits;
