//! Opening diversification: varied starting positions for self-play games.
//!
//! If every game began from the initial position, a reproducible node-budget
//! search would play the same moves every time and the generated data would be
//! a single game repeated. To spread the games across the opening tree, each
//! game starts from a position reached by playing a few uniformly-random legal
//! moves from the initial position.
//!
//! The randomness is generated internally, from a seed, and no game record or
//! position list is ever read from disk or the network. This is deliberate: the
//! wider project trains its evaluation purely from its own self-play, so pulling
//! openings from an external game database would breach that purity. A random
//! walk over the engine's own legal moves needs no such input.
//!
//! A game's opening depends only on its index and the run seed, never on which
//! worker happened to play it, so a whole run's data is reproducible on a given
//! build.

use chess::mono_traits::{All, Legal};
use chess::movelist::BasicMoveList;
use chess::position::Position;

/// How far, and from what seed, opening positions are diversified.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct OpeningConfig {
    /// Number of random legal plies played from the initial position before the
    /// game proper begins. Zero reproduces the plain initial position, which is
    /// the behaviour when diversification is switched off.
    pub plies: usize,
    /// Seed for the random walk. Two runs with the same seed and ply count
    /// generate the same set of openings.
    pub seed: u64,
}

impl Default for OpeningConfig {
    fn default() -> Self {
        // Eight random plies reach a broad spread of openings while staying
        // shallow enough that the positions are still sensible to play on from.
        Self {
            plies: 8,
            seed: 0x05EE_D0DD_F1CE_u64,
        }
    }
}

impl OpeningConfig {
    /// The starting position for the game with index `game_index`.
    ///
    /// The walk is seeded from the run seed and the index alone, so the same
    /// index always yields the same opening regardless of scheduling. The result
    /// is guaranteed to have at least one legal move: a random line that stumbles
    /// into checkmate or stalemate is stepped back until the game can continue,
    /// so the caller never receives a start that is already over.
    pub fn start_for(&self, game_index: usize) -> Position {
        let mut position = Position::start_pos();
        if self.plies == 0 {
            return position;
        }

        // Fold the index into the seed so each game draws an independent stream.
        let stream_seed = self
            .seed
            .wrapping_add((game_index as u64).wrapping_mul(GOLDEN_GAMMA));
        let mut rng = SplitMix64::new(stream_seed);

        for _ in 0..self.plies {
            let moves = position.generate::<BasicMoveList, All, Legal>();
            if moves.is_empty() {
                break;
            }
            let choice = rng.below(moves.len());
            position.make_move(&moves[choice]);
        }

        // A random walk can end in mate or stalemate; back off any terminal tail
        // so the returned position always has a move to make.
        while position.generate::<BasicMoveList, All, Legal>().is_empty() {
            if position.unmake_move().is_none() {
                break;
            }
        }

        position
    }
}

/// The odd increment SplitMix64 adds to its state each step (the fractional bits
/// of the golden ratio scaled to 64 bits). Reused to scatter the game index into
/// the per-game seed.
const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// A tiny deterministic pseudo-random generator.
///
/// The opening walk needs randomness that is byte-for-byte reproducible for a
/// given seed on every platform and toolchain, so a run's data can be
/// regenerated exactly. A fixed, self-contained algorithm guarantees that;
/// a general-purpose RNG whose stream is allowed to change between library
/// versions would not. SplitMix64 is a well-known such algorithm, small enough
/// to state in full here.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN_GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `0..n`. `n` must be non-zero. The modulo reduction is very
    /// slightly biased for an `n` that does not divide `2^64`, which is
    /// immaterial for choosing among a few dozen legal moves.
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chess::init::init_globals;

    fn config(plies: usize, seed: u64) -> OpeningConfig {
        init_globals();
        OpeningConfig { plies, seed }
    }

    #[test]
    fn zero_plies_is_the_initial_position() {
        let opening = config(0, 1);
        assert_eq!(opening.start_for(0), Position::start_pos());
        assert_eq!(opening.start_for(7), Position::start_pos());
    }

    #[test]
    fn the_same_index_reproduces_the_same_opening() {
        let opening = config(8, 42);
        // Two calls, and two configs built independently, must agree: the
        // opening is a pure function of seed and index.
        assert_eq!(opening.start_for(3), opening.start_for(3));
        assert_eq!(opening.start_for(3), config(8, 42).start_for(3));
    }

    #[test]
    fn different_indices_diversify_the_start() {
        let opening = config(8, 42);
        // Across a handful of games the openings must not collapse to one
        // position; a broad distribution is the whole point.
        let starts: std::collections::HashSet<String> =
            (0..16).map(|i| opening.start_for(i).to_fen()).collect();
        assert!(
            starts.len() >= 12,
            "expected diverse openings, got {} distinct of 16",
            starts.len()
        );
        // None of them is the bare initial position.
        assert!(!starts.contains(&Position::start_pos().to_fen()));
    }

    #[test]
    fn a_different_seed_gives_a_different_walk() {
        assert_ne!(config(8, 1).start_for(0), config(8, 2).start_for(0));
    }

    #[test]
    fn the_start_always_has_a_legal_move() {
        // Even with a deep walk that can wander into mate, every returned start
        // must be playable, so the game loop never receives a finished game.
        let opening = config(40, 7);
        for i in 0..64 {
            let start = opening.start_for(i);
            let moves = start.generate::<BasicMoveList, All, Legal>();
            assert!(
                !moves.is_empty(),
                "opening {i} produced a terminal start: {}",
                start.to_fen()
            );
        }
    }
}
