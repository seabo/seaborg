#![feature(stmt_expr_attributes)]
#![feature(maybe_uninit_slice)]
#![feature(maybe_uninit_uninit_array)]

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
    All, Bishop, Black, Captures, Generate, King, Knight, Pawn, PieceTrait, Queen, Quiets, Rook,
    Side, White,
};
