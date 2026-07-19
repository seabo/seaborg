//! Bot configuration loaded from TOML.
//!
//! Every field has a default, so an absent or partial configuration file still
//! produces a complete, working configuration. The defaults are conservative:
//! standard chess only, humans but not other bots, and a single game at a time.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

/// File name looked up in the current directory when no `--config` path is
/// given. A missing default file is not an error; the built-in defaults apply.
pub const DEFAULT_CONFIG_FILE: &str = "seaborg-lichess.toml";

/// Top-level bot configuration.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Which incoming challenges to accept.
    pub challenge: ChallengePolicy,
    /// Engine tuning applied to each game.
    pub engine: EngineSettings,
    /// The most games to play at once. Challenges that would exceed this are
    /// declined until a game finishes.
    pub max_concurrent_games: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            challenge: ChallengePolicy::default(),
            engine: EngineSettings::default(),
            max_concurrent_games: 1,
        }
    }
}

impl Config {
    /// Load configuration, preferring an explicit path.
    ///
    /// With `Some(path)`, the file must exist and parse; a missing explicit path
    /// is an error because the user asked for a specific file. With `None`, the
    /// default file is used if present and the built-in defaults apply if it is
    /// absent.
    pub fn load(path: Option<&Path>) -> Result<Config> {
        match path {
            Some(path) => {
                if !path.exists() {
                    return Err(Error::Config(format!(
                        "config file not found: {}",
                        path.display()
                    )));
                }
                Config::read_file(path)
            }
            None => {
                let default = PathBuf::from(DEFAULT_CONFIG_FILE);
                if default.exists() {
                    Config::read_file(&default)
                } else {
                    Ok(Config::default())
                }
            }
        }
    }

    /// Parse a configuration file that is known to exist.
    fn read_file(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("reading {}: {e}", path.display())))?;
        Config::parse(&text)
    }

    /// Parse configuration from TOML text. Exposed for tests and for callers
    /// that already hold the file contents.
    pub fn parse(text: &str) -> Result<Config> {
        let config: Config =
            toml::from_str(text).map_err(|e| Error::Config(format!("parsing TOML: {e}")))?;
        config.validate()?;
        Ok(config)
    }

    /// Reject configurations that are syntactically valid but nonsensical, so a
    /// typo surfaces at startup rather than silently declining every challenge.
    fn validate(&self) -> Result<()> {
        let c = &self.challenge;
        if c.min_initial_seconds > c.max_initial_seconds {
            return Err(Error::Config(
                "challenge.min_initial_seconds exceeds max_initial_seconds".into(),
            ));
        }
        if c.min_increment_seconds > c.max_increment_seconds {
            return Err(Error::Config(
                "challenge.min_increment_seconds exceeds max_increment_seconds".into(),
            ));
        }
        if c.min_rating > c.max_rating {
            return Err(Error::Config(
                "challenge.min_rating exceeds max_rating".into(),
            ));
        }
        Ok(())
    }
}

/// The rules deciding whether to accept an incoming challenge.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChallengePolicy {
    /// Accepted variant keys (Lichess `variant.key` values, e.g. `standard`).
    pub variants: Vec<String>,
    /// Whether to accept rated challenges.
    pub accept_rated: bool,
    /// Whether to accept casual challenges.
    pub accept_casual: bool,
    /// Whether to accept challenges from other bots.
    pub accept_bots: bool,
    /// Whether to accept challenges from human accounts.
    pub accept_humans: bool,
    /// Inclusive lower bound on the clock's initial time, in seconds.
    pub min_initial_seconds: u32,
    /// Inclusive upper bound on the clock's initial time, in seconds.
    pub max_initial_seconds: u32,
    /// Inclusive lower bound on the clock's increment, in seconds.
    pub min_increment_seconds: u32,
    /// Inclusive upper bound on the clock's increment, in seconds.
    pub max_increment_seconds: u32,
    /// Inclusive lower bound on the opponent's rating in the relevant pool.
    pub min_rating: u32,
    /// Inclusive upper bound on the opponent's rating in the relevant pool.
    pub max_rating: u32,
    /// Whether to accept challenges without a real-time clock (correspondence
    /// or unlimited). The engine is built for clocked play, so this is off by
    /// default.
    pub accept_unlimited: bool,
}

impl Default for ChallengePolicy {
    fn default() -> Self {
        ChallengePolicy {
            variants: vec!["standard".to_string()],
            accept_rated: true,
            accept_casual: true,
            accept_bots: false,
            accept_humans: true,
            // 1+0 up to 30+x: fast enough to play many games, slow enough that
            // the engine has time to move.
            min_initial_seconds: 60,
            max_initial_seconds: 1800,
            min_increment_seconds: 0,
            max_increment_seconds: 60,
            min_rating: 0,
            max_rating: 4000,
            accept_unlimited: false,
        }
    }
}

impl ChallengePolicy {
    /// Whether `variant_key` is on the accepted list.
    pub fn allows_variant(&self, variant_key: &str) -> bool {
        self.variants.iter().any(|v| v == variant_key)
    }
}

/// Engine tuning applied to each accepted game.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EngineSettings {
    /// Transposition table size in mebibytes.
    pub hash_mb: usize,
    /// Time subtracted from each move's budget to cover network and scheduling
    /// latency, in milliseconds. Prevents flagging when a move is computed just
    /// under the clock.
    pub move_overhead_ms: u32,
}

impl Default for EngineSettings {
    fn default() -> Self {
        EngineSettings {
            hash_mb: 64,
            move_overhead_ms: 100,
        }
    }
}

impl EngineSettings {
    /// Translate these settings into the engine's option type, the form the
    /// game runner will apply when it configures the engine for a game.
    pub fn engine_options(&self) -> Vec<engine::options::EngineOpt> {
        vec![engine::options::EngineOpt::Hash(self.hash_mb)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_accept_standard_casual_humans() {
        let config = Config::default();
        assert_eq!(config.max_concurrent_games, 1);
        assert!(config.challenge.allows_variant("standard"));
        assert!(!config.challenge.allows_variant("chess960"));
        assert!(config.challenge.accept_humans);
        assert!(!config.challenge.accept_bots);
        assert_eq!(config.engine.hash_mb, 64);
    }

    #[test]
    fn partial_toml_fills_missing_fields_from_defaults() {
        let config = Config::parse(
            r#"
                max_concurrent_games = 4
                [engine]
                hash_mb = 256
            "#,
        )
        .unwrap();
        assert_eq!(config.max_concurrent_games, 4);
        assert_eq!(config.engine.hash_mb, 256);
        // Untouched fields keep their defaults.
        assert_eq!(config.engine.move_overhead_ms, 100);
        assert_eq!(config.challenge.variants, vec!["standard".to_string()]);
    }

    #[test]
    fn unknown_field_is_rejected() {
        let err = Config::parse("no_such_key = 1").unwrap_err();
        assert!(matches!(err, Error::Config(_)));
    }

    #[test]
    fn inverted_bounds_are_rejected() {
        let err = Config::parse(
            r#"
                [challenge]
                min_rating = 2000
                max_rating = 1000
            "#,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Config(_)));
    }

    #[test]
    fn missing_explicit_config_path_is_an_error() {
        let err = Config::load(Some(Path::new("/no/such/seaborg-lichess.toml"))).unwrap_err();
        assert!(matches!(err, Error::Config(_)));
    }

    #[test]
    fn absent_default_config_yields_defaults() {
        // No path given and the default file is absent in this working dir.
        let config = Config::load(None).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn engine_options_carry_hash_size() {
        let settings = EngineSettings {
            hash_mb: 128,
            move_overhead_ms: 50,
        };
        let opts = settings.engine_options();
        assert!(matches!(
            opts.as_slice(),
            [engine::options::EngineOpt::Hash(128)]
        ));
    }
}
