//! NNUE network representation and its on-disk serialization boundary.
//!
//! The network file is the sole contract that carries trained weights across
//! the language boundary between the Python trainer (which writes files) and the
//! Rust engine (which reads them). This module owns that boundary: the versioned
//! `SBNN` binary format, a validated in-memory [`Network`], and a loader that
//! refuses any file it cannot interpret exactly rather than misreading one.
//!
//! Inference and the incremental accumulator are deliberately *not* here — this
//! module is purely serialization and its guarantees, so it can be built and
//! reviewed on its own. The format itself (feature set, topology, quantization
//! scales, header layout, and the rejection rules) is fixed by the NNUE design
//! contract in `docs/nnue-design-contract.md`.

mod format;

pub use format::{
    BuildError, LoadError, Network, Parameters, ACTIVATION_CRELU, FEATURE_SET_PERSPECTIVE_768,
    FORMAT_VERSION, HEADER_LEN, INPUT_DIM, MAGIC, OUTPUT_DIM,
};
