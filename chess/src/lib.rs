//! Chess-domain representation and rules: bitboards, pieces and squares,
//! positions and FEN, move representation and generation, precomputed tables,
//! and global initialization.
//!
//! This crate is deliberately named `chess` rather than `core`: a crate named
//! `core` makes imports such as `use core::position::Position` read like
//! standard-library paths and hides which project owns the type. `use
//! chess::position::Position` is unambiguous.
//!
//! The `pub` modules below are the board-domain API the engine, the `seaborg`
//! binary, the Lichess client, and the benchmarks build against. Low-level
//! helpers (bit twiddling, masks, precomputation, and the declarative macros)
//! are private, and `position` keeps its own facade over its implementation
//! submodules.

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
