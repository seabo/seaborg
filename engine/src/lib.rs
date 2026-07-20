//! Game and search behaviour layered on the `chess` domain crate: evaluation,
//! move ordering, search, the transposition table, time control, UCI, and the
//! loopback browser UI.
//!
//! # Supported API
//!
//! The modules declared `pub` below, together with the [`launch`] entry point
//! and [`EngineInfo`] re-exported here at the crate root, are the surface other
//! workspace crates, the `seaborg` binary, and the benchmarks are expected to
//! use:
//!
//! - [`launch`] / [`EngineInfo`] — start the UCI loop for a named engine build.
//! - [`ui`] — serve the loopback browser UI.
//! - [`search`] — the search driver and its limits.
//! - [`selfplay`] — self-play data generation for network training.
//! - [`eval`] — static position evaluation.
//! - [`tt`] — the shared transposition table.
//! - [`score`] — search score representation.
//! - [`time`] — time-control models.
//! - [`options`] — configurable UCI engine options.
//! - [`perft`] — move-generation correctness/performance counting.
//!
//! Everything else is an implementation detail. Modules such as the game tree,
//! history and killer heuristics, move ordering, the principal-variation table,
//! static exchange evaluation, search tracing, and the UCI parser are private
//! so they can be reorganised without the change reading as a workspace-wide API
//! break. Promote a module to `pub` only when a supported consumer genuinely
//! needs it and you intend to keep it stable.

// Supported public API.
pub mod eval;
pub mod options;
pub mod perft;
pub mod score;
pub mod search;
pub mod selfplay;
pub mod time;
pub mod tt;
pub mod ui;

// Implementation detail: reachable throughout the crate but not part of the
// supported API, so kept private to the crate.
mod engine;
mod game;
mod history;
mod info;
mod killer;
mod ordering;
mod pv_table;
mod see;
mod trace;
mod uci;

// The UCI entry point is the engine's front door; re-export it at the crate root
// so callers write `engine::launch` rather than reaching into a same-named
// submodule.
pub use engine::{launch, EngineInfo};
