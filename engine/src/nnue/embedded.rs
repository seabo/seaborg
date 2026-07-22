//! The network carried inside the executable, and the identity of whichever
//! evaluator a process ends up running.
//!
//! A binary that has to be pointed at a network file before it plays at full
//! strength is a binary that plays below full strength whenever somebody forgets.
//! So the promoted network is committed at `engine/nets/default.sbnn` and linked
//! into the executable with `include_bytes!`, and every engine constructed here
//! starts out evaluating with it. The bytes are parsed by the same
//! [`Network::read`] that reads an operator-supplied file: there is exactly one
//! loader, so a corrupt or foreign baked file is rejected by the same rules
//! rather than trusted because it shipped with the build.
//!
//! Embedding sits behind the default-on `embedded-net` Cargo feature. Turning it
//! off yields a binary with no built-in network, which evaluates with the
//! hand-crafted evaluation — useful for measuring what the network is worth, and
//! for building where the ~400 KB of weights is unwanted.
//!
//! Which evaluator is live is not inferable from the outside — the same binary
//! plays very differently depending on it — so [`ActiveEvaluator`] names it
//! precisely enough to attribute a measurement: the network's provenance, its
//! hidden width, and the parameter hash from its header.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
// Only the embedding path caches a parsed network; without the feature there is nothing to cache.
#[cfg(feature = "embedded-net")]
use std::sync::OnceLock;

use super::Network;

/// The committed default network's bytes, linked into the executable.
///
/// Not public: nothing outside this module should interpret these bytes by any
/// route other than [`built_in_network`], which validates them.
#[cfg(feature = "embedded-net")]
const BAKED_BYTES: &[u8] = include_bytes!("../../nets/default.sbnn");

/// Provenance name of the committed default network.
///
/// This is the identifier under which the network was promoted by the training
/// loop, and it is what a measurement is attributed to; the on-disk file is
/// called `default.sbnn` precisely so that re-baking is a content change rather
/// than a rename, which makes this constant the only record of *which* network
/// a given build carries. Re-baking must update it in the same commit.
#[cfg(feature = "embedded-net")]
pub const BUILT_IN_NETWORK_ID: &str = "gen-000";

/// The built-in default network, or `None` in a build without one.
///
/// Parsed once per process and shared: the weights are read-only and several
/// engines (UCI driver, Lichess games, self-play workers) may want them at once,
/// so they are behind an [`Arc`] rather than copied per construction.
///
/// A baked file that fails validation yields `None`, which degrades the build to
/// the hand-crafted evaluation and says so in the evaluator report rather than
/// evaluating through half-read weights. That state is a build defect, and the
/// unit tests below fail on it.
pub fn built_in_network() -> Option<Arc<Network>> {
    #[cfg(feature = "embedded-net")]
    {
        static BUILT_IN: OnceLock<Option<Arc<Network>>> = OnceLock::new();
        BUILT_IN
            .get_or_init(|| Network::read(&mut &BAKED_BYTES[..]).ok().map(Arc::new))
            .clone()
    }
    #[cfg(not(feature = "embedded-net"))]
    {
        None
    }
}

/// Where a network the engine is evaluating with came from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkOrigin {
    /// The network linked into this executable, named by its provenance id.
    BuiltIn(&'static str),
    /// A network loaded at runtime from this path.
    File(PathBuf),
}

/// The evaluator a process is currently using, in enough detail to attribute a
/// game or a benchmark to it.
///
/// The hidden width and parameter hash come from the loaded network itself, so a
/// report cannot claim a network the engine is not actually evaluating with. The
/// parameter hash is the discriminating field: two builds can carry networks of
/// the same width that play quite differently.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActiveEvaluator {
    /// No network is selected; leaves are scored by the hand-crafted evaluation.
    HandCrafted,
    /// Leaves are scored by this network's quantized forward pass.
    Network {
        /// Where the network came from.
        origin: NetworkOrigin,
        /// Feature-transformer output width per perspective.
        hidden_width: u32,
        /// FNV-1a hash of the parameter blob, as recorded in the file header.
        param_hash: u64,
    },
}

impl ActiveEvaluator {
    /// Describes `network` as loaded from `origin`.
    pub fn of(network: &Network, origin: NetworkOrigin) -> Self {
        Self::Network {
            origin,
            hidden_width: network.hidden_width(),
            param_hash: network.param_hash(),
        }
    }

    /// Describes the evaluator an engine holding `network` is running.
    ///
    /// `None` is the hand-crafted evaluation, which is what a `SearchEngine`
    /// with no network selected uses.
    pub fn of_built_in(network: Option<&Network>) -> Self {
        match network {
            None => Self::HandCrafted,
            Some(network) => Self::of(network, NetworkOrigin::BuiltIn(built_in_id())),
        }
    }
}

/// The provenance id to report for the built-in network.
fn built_in_id() -> &'static str {
    #[cfg(feature = "embedded-net")]
    {
        BUILT_IN_NETWORK_ID
    }
    // A build with no embedded network has no built-in network to describe, so this is unreachable
    // through [`built_in_network`]. It stays a truthful string rather than a panic, because the
    // evaluator report exists to tell an operator what is running and must never be the thing that
    // takes the engine down.
    #[cfg(not(feature = "embedded-net"))]
    {
        "unnamed"
    }
}

impl fmt::Display for ActiveEvaluator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HandCrafted => write!(f, "hand-crafted evaluation"),
            Self::Network {
                origin,
                hidden_width,
                param_hash,
            } => {
                match origin {
                    NetworkOrigin::BuiltIn(id) => write!(f, "NNUE built-in {id}")?,
                    NetworkOrigin::File(path) => write!(f, "NNUE file {}", path.display())?,
                }
                write!(
                    f,
                    " (hidden width {hidden_width}, parameter hash {param_hash:#018x})"
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "embedded-net")]
    #[test]
    fn the_baked_bytes_parse_through_the_one_loader_with_the_expected_architecture() {
        // Exactly the path an `EvalFile` load takes, so a baked file that the
        // runtime loader would reject fails here rather than in a game.
        let network = Network::read(&mut &BAKED_BYTES[..])
            .expect("the committed default network is a valid SBNN file");
        // The architecture this build evaluates: 768 perspective-doubled inputs
        // into a 256-wide feature transformer, quantized at the contract scales.
        assert_eq!(network.hidden_width(), 256);
        assert_eq!(network.qa(), 255);
        assert_eq!(network.qb(), 64);
        assert_eq!(network.scale(), 400);
        // Pins the identity of the promoted network: re-baking a different one
        // without updating `BUILT_IN_NETWORK_ID` and this hash together is the
        // mistake that makes a benchmark unattributable.
        assert_eq!(network.param_hash(), 0xdaf8_6bb3_d50c_ec6b);
    }

    #[cfg(feature = "embedded-net")]
    #[test]
    fn the_built_in_network_is_available_and_shared() {
        let first = built_in_network().expect("this build embeds a network");
        let second = built_in_network().expect("this build embeds a network");
        // Parsed once and handed out by reference-count, not re-parsed per call.
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.hidden_width(), 256);
    }

    #[cfg(not(feature = "embedded-net"))]
    #[test]
    fn a_build_without_the_feature_has_no_built_in_network() {
        assert!(built_in_network().is_none());
    }

    #[test]
    fn the_report_names_the_evaluator_precisely() {
        assert_eq!(
            ActiveEvaluator::HandCrafted.to_string(),
            "hand-crafted evaluation"
        );
        let described = ActiveEvaluator::Network {
            origin: NetworkOrigin::BuiltIn("gen-007"),
            hidden_width: 256,
            param_hash: 0x0123_4567_89ab_cdef,
        };
        assert_eq!(
            described.to_string(),
            "NNUE built-in gen-007 (hidden width 256, parameter hash 0x0123456789abcdef)"
        );
        let from_file = ActiveEvaluator::Network {
            origin: NetworkOrigin::File(PathBuf::from("/nets/candidate.sbnn")),
            hidden_width: 128,
            param_hash: 0xdead_beef,
        };
        assert_eq!(
            from_file.to_string(),
            "NNUE file /nets/candidate.sbnn (hidden width 128, parameter hash 0x00000000deadbeef)"
        );
    }
}
