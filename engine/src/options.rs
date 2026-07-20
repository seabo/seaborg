//! The authoritative runtime engine configuration.
//!
//! [`EngineConfig`] is the single owner of every engine resource setting the UCI layer can change.
//! Its bounds constants are the one source the handshake advertisement, the command parser's
//! validation, and the config's own validation all read from, so the engine can never advertise a
//! range it would refuse to apply, nor apply one it never advertised. That truthfulness is the
//! reason the settings live here rather than as ad hoc locals in the driver.

use std::fmt;

/// A requested configuration value fell outside the bounds the engine advertises for it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionError {
    /// A `Hash` size in MiB outside the advertised bounds.
    HashOutOfRange(usize),
    /// A worker count outside the supported `Threads` bounds.
    ThreadsOutOfRange(usize),
}

impl fmt::Display for OptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HashOutOfRange(mb) => write!(
                f,
                "Hash must be between {} and {} MB, got {mb}",
                EngineConfig::HASH_MIN_MB,
                EngineConfig::HASH_MAX_MB,
            ),
            Self::ThreadsOutOfRange(n) => write!(
                f,
                "Threads must be between {} and {}, got {n}",
                EngineConfig::THREADS_MIN,
                EngineConfig::THREADS_MAX,
            ),
        }
    }
}

/// The authoritative runtime configuration of the engine.
///
/// One instance is owned by the UCI driver and is the sole record of what the running engine
/// applies. A resource that maps to an allocation — the transposition table — is described here but
/// owned elsewhere; the driver keeps the two in step by applying a validated change to this config
/// and to the resource together, at a boundary where no search is using the old allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineConfig {
    /// Transposition-table size in MiB. Mirrors the live table the search shares.
    hash_mb: usize,
    /// The number of search workers a `go` should run. One today; the field exists so a later Lazy
    /// SMP search can be configured without another ownership rewrite.
    threads: usize,
    /// Whether UCI `debug` mode is on. A mode flag, not a resource: changing it allocates nothing
    /// and never needs a quiescent boundary.
    debug: bool,
}

impl EngineConfig {
    /// Default transposition-table size in MiB, and the advertised `Hash` default.
    pub const HASH_DEFAULT_MB: usize = 16;
    /// Smallest transposition-table size the engine accepts, in MiB.
    pub const HASH_MIN_MB: usize = 1;
    /// Largest transposition-table size the engine accepts, in MiB.
    pub const HASH_MAX_MB: usize = 1024;

    /// Default worker count.
    pub const THREADS_DEFAULT: usize = 1;
    /// Smallest supported worker count.
    pub const THREADS_MIN: usize = 1;
    /// Largest supported worker count.
    ///
    /// One while the search runs a single worker. `Threads` is not advertised at this bound because
    /// a single-valued range promises no configurability; the ceiling is raised, and the option
    /// advertised, when a real multi-worker search consumes [`EngineConfig::threads`].
    pub const THREADS_MAX: usize = 1;

    /// A fresh configuration at the advertised defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// The configured transposition-table size in MiB.
    pub fn hash_mb(&self) -> usize {
        self.hash_mb
    }

    /// The configured worker count.
    pub fn threads(&self) -> usize {
        self.threads
    }

    /// Whether debug mode is on.
    pub fn debug(&self) -> bool {
        self.debug
    }

    /// Check a requested hash size against the advertised bounds without applying it.
    ///
    /// This is the one validation the parser and [`EngineConfig::set_hash_mb`] both call, so a
    /// value the handshake would reject can never be accepted anywhere else.
    pub fn validate_hash_mb(mb: usize) -> Result<usize, OptionError> {
        if (Self::HASH_MIN_MB..=Self::HASH_MAX_MB).contains(&mb) {
            Ok(mb)
        } else {
            Err(OptionError::HashOutOfRange(mb))
        }
    }

    /// Check a requested worker count against the supported bounds without applying it.
    pub fn validate_threads(n: usize) -> Result<usize, OptionError> {
        if (Self::THREADS_MIN..=Self::THREADS_MAX).contains(&n) {
            Ok(n)
        } else {
            Err(OptionError::ThreadsOutOfRange(n))
        }
    }

    /// Record a new hash size, rejecting an out-of-range request without changing anything.
    ///
    /// Applying the change to the live allocation is the caller's separate step, done only once the
    /// search using the old table has stopped; this merely updates the authoritative record after
    /// validating it.
    pub fn set_hash_mb(&mut self, mb: usize) -> Result<(), OptionError> {
        self.hash_mb = Self::validate_hash_mb(mb)?;
        Ok(())
    }

    /// Record a new worker count, rejecting an out-of-range request without changing anything.
    pub fn set_threads(&mut self, n: usize) -> Result<(), OptionError> {
        self.threads = Self::validate_threads(n)?;
        Ok(())
    }

    /// Turn debug mode on or off.
    pub fn set_debug(&mut self, on: bool) {
        self.debug = on;
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            hash_mb: Self::HASH_DEFAULT_MB,
            threads: Self::THREADS_DEFAULT,
            debug: false,
        }
    }
}

impl fmt::Display for EngineConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "hash {} MB, threads {}, debug {}",
            self.hash_mb,
            self.threads,
            if self.debug { "on" } else { "off" },
        )
    }
}

/// The UCI `option` advertisements for exactly the options this build implements.
///
/// This renders the handshake's option lines from the same bounds [`EngineConfig`] validates
/// against, so the advertised range is by construction the range the engine accepts. `Threads` is
/// deliberately absent: the search runs a single worker, and advertising a worker count would
/// promise Lazy SMP parallelism the engine does not yet provide. It joins this list when a real
/// multi-worker search consumes [`EngineConfig::threads`].
pub fn advertised_uci_options() -> String {
    format!(
        "option name Hash type spin default {} min {} max {}",
        EngineConfig::HASH_DEFAULT_MB,
        EngineConfig::HASH_MIN_MB,
        EngineConfig::HASH_MAX_MB,
    )
}

/// Possible options which can be set via the UCI protocol.
#[derive(Clone, Debug)]
pub enum EngineOpt {
    /// The size in MiB of the hash table.
    Hash(usize),
    /// Whether debug mode is turned on.
    DebugMode(bool),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_advertised_defaults() {
        let config = EngineConfig::new();
        assert_eq!(config.hash_mb(), EngineConfig::HASH_DEFAULT_MB);
        assert_eq!(config.threads(), EngineConfig::THREADS_DEFAULT);
        assert!(!config.debug());
        // The default must sit inside the advertised bounds, or the handshake would offer a value
        // the engine rejects on the next `setoption`.
        assert!(EngineConfig::validate_hash_mb(config.hash_mb()).is_ok());
    }

    #[test]
    fn advertisement_and_validation_share_one_range() {
        let advert = advertised_uci_options();
        assert_eq!(
            advert,
            format!(
                "option name Hash type spin default {} min {} max {}",
                EngineConfig::HASH_DEFAULT_MB,
                EngineConfig::HASH_MIN_MB,
                EngineConfig::HASH_MAX_MB,
            )
        );
        // The advertised bounds are exactly the acceptance boundaries: just inside is valid, just
        // outside is not.
        assert!(EngineConfig::validate_hash_mb(EngineConfig::HASH_MIN_MB).is_ok());
        assert!(EngineConfig::validate_hash_mb(EngineConfig::HASH_MAX_MB).is_ok());
        assert!(EngineConfig::validate_hash_mb(EngineConfig::HASH_MIN_MB - 1).is_err());
        assert!(EngineConfig::validate_hash_mb(EngineConfig::HASH_MAX_MB + 1).is_err());

        // Threads is unadvertised precisely because its range is a single value.
        assert!(!advert.contains("Threads"));
        assert_eq!(EngineConfig::THREADS_MIN, EngineConfig::THREADS_MAX);
    }

    #[test]
    fn valid_values_apply_and_invalid_values_are_rejected_intact() {
        let mut config = EngineConfig::new();

        assert!(config.set_hash_mb(256).is_ok());
        assert_eq!(config.hash_mb(), 256);

        // A rejected value leaves the previously accepted one in place.
        assert_eq!(config.set_hash_mb(0), Err(OptionError::HashOutOfRange(0)));
        assert_eq!(
            config.set_hash_mb(EngineConfig::HASH_MAX_MB + 1),
            Err(OptionError::HashOutOfRange(EngineConfig::HASH_MAX_MB + 1)),
        );
        assert_eq!(config.hash_mb(), 256);
    }

    #[test]
    fn repeated_changes_take_the_latest_value() {
        let mut config = EngineConfig::new();
        for mb in [1, 1024, 32, 16] {
            assert!(config.set_hash_mb(mb).is_ok());
            assert_eq!(config.hash_mb(), mb);
        }
    }

    #[test]
    fn threads_validation_tracks_the_supported_bound() {
        let mut config = EngineConfig::new();
        assert!(config.set_threads(EngineConfig::THREADS_MIN).is_ok());
        assert_eq!(
            config.set_threads(EngineConfig::THREADS_MAX + 1),
            Err(OptionError::ThreadsOutOfRange(
                EngineConfig::THREADS_MAX + 1
            )),
        );
    }

    #[test]
    fn debug_toggles_without_affecting_resources() {
        let mut config = EngineConfig::new();
        let hash = config.hash_mb();
        config.set_debug(true);
        assert!(config.debug());
        // Debug is a mode flag: toggling it must not disturb the resource settings.
        assert_eq!(config.hash_mb(), hash);
        config.set_debug(false);
        assert!(!config.debug());
    }
}
