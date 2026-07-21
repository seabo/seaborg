//! NNUE network representation, its on-disk serialization boundary, and the
//! incrementally-maintained first-layer accumulator.
//!
//! The network file is the sole contract that carries trained weights across the
//! language boundary between the Python trainer (which writes files) and the Rust
//! engine (which reads them). The [`format`] submodule owns that boundary: the
//! versioned `SBNN` binary format, a validated in-memory [`Network`], and a
//! loader that refuses any file it cannot interpret exactly rather than misreading
//! one.
//!
//! The [`accumulator`] submodule owns the engine-side integration: the 768-input
//! perspective-doubled feature encoding ([`feature_index`]) and the
//! [`Accumulator`] that maintains both perspectives' first-layer activations
//! incrementally as a [`chess::position::PieceDeltaSink`], borrowing the
//! feature-transformer weights straight from a loaded [`Network`].
//!
//! The forward pass — combining the two perspectives, applying the activation, and
//! reading out a score — is deliberately *not* here yet. The format itself
//! (feature set, topology, quantization scales, header layout, and the rejection
//! rules) is fixed by the NNUE design contract in `docs/nnue-design-contract.md`.

mod accumulator;
mod format;

pub use accumulator::{feature_index, Accumulator};
pub use format::{
    BuildError, LoadError, Network, Parameters, ACTIVATION_CRELU, FEATURE_SET_PERSPECTIVE_768,
    FORMAT_VERSION, HEADER_LEN, INPUT_DIM, MAGIC, OUTPUT_DIM,
};
