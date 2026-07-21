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
    /// Proactive matchmaking: whether and how to challenge other bots when idle.
    pub matchmaking: MatchmakingConfig,
    /// The most games to play at once. Challenges that would exceed this are
    /// declined until a game finishes.
    pub max_concurrent_games: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            challenge: ChallengePolicy::default(),
            engine: EngineSettings::default(),
            matchmaking: MatchmakingConfig::default(),
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
        self.matchmaking.validate(self.max_concurrent_games)?;
        Ok(())
    }
}

/// Which of rated/casual to seek when issuing a matchmaking challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchmakingMode {
    /// Always challenge for a rated game.
    Rated,
    /// Always challenge for a casual game.
    Casual,
    /// Alternate between rated and casual across successive challenges.
    Random,
}

/// Configuration for proactive matchmaking.
///
/// When [`enabled`](MatchmakingConfig::enabled) is false (the default) the bot is
/// purely reactive and none of the other fields have any effect: it never issues
/// an outgoing challenge and behaves exactly as a build without matchmaking. The
/// remaining fields describe who to challenge, with what time control, and how
/// often, and only come into play once matchmaking is turned on.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MatchmakingConfig {
    /// Master switch. Off by default so existing reactive behaviour is unchanged.
    pub enabled: bool,
    /// Variant keys to draw from when composing a challenge (e.g. `standard`).
    pub variants: Vec<String>,
    /// Initial clock times, in seconds, to draw from.
    pub initial_seconds: Vec<u32>,
    /// Clock increments, in seconds, to draw from.
    pub increment_seconds: Vec<u32>,
    /// Inclusive lower bound on a candidate opponent's rating for the chosen
    /// time control's speed.
    pub min_rating: u32,
    /// Inclusive upper bound on a candidate opponent's rating for the chosen
    /// time control's speed.
    pub max_rating: u32,
    /// Whether issued challenges are rated, casual, or alternating.
    pub mode: MatchmakingMode,
    /// How long the bot must be idle (no games in progress) before it starts
    /// seeking an opponent, in seconds.
    pub idle_timeout_seconds: u64,
    /// Minimum gap between successive outgoing challenges, in seconds, so a run
    /// of declines or cancellations does not spam the pool.
    pub min_challenge_interval_seconds: u64,
    /// Concurrent-game slots held back for human challengers. Both outgoing
    /// matchmaking and incoming *bot* acceptances treat the cap as
    /// `max_concurrent_games - reserved_human_slots`, so this many slots stay
    /// reachable by a human even when bot games and challenges would otherwise
    /// fill the board. Human challenges may still use the full
    /// `max_concurrent_games`. It lives here rather than under `[challenge]`
    /// because it began as a matchmaking-only reservation; it now also applies to
    /// the acceptance side and takes effect whether or not matchmaking is enabled.
    pub reserved_human_slots: u32,
    /// Account ids never to challenge, however eligible they otherwise look.
    pub block_list: Vec<String>,
    /// After a bot declines a challenge, how long to avoid re-challenging that
    /// same bot, in seconds.
    pub decline_backoff_seconds: u64,
}

impl Default for MatchmakingConfig {
    fn default() -> Self {
        MatchmakingConfig {
            enabled: false,
            variants: vec!["standard".to_string()],
            // 1+0, 3+2, 5+3: a spread of bullet and blitz controls other bots
            // commonly accept.
            initial_seconds: vec![60, 180, 300],
            increment_seconds: vec![0, 2, 3],
            min_rating: 0,
            max_rating: 4000,
            // Casual by default: rated challenges to bots are frequently rejected
            // at creation (many bots, such as the Maia family, accept only casual
            // games), so a rated default would fail out of the box for most
            // opponents. Rated and random remain available for accounts that want
            // them.
            mode: MatchmakingMode::Casual,
            idle_timeout_seconds: 30,
            min_challenge_interval_seconds: 30,
            reserved_human_slots: 0,
            block_list: Vec::new(),
            decline_backoff_seconds: 3600,
        }
    }
}

impl MatchmakingConfig {
    /// Reject a matchmaking configuration that could never issue a sensible
    /// challenge, so a mistake surfaces at startup rather than as a bot that
    /// silently never seeks a game.
    ///
    /// The pool and slot checks only apply when matchmaking is enabled: a
    /// disabled section is inert, so an empty pool there is harmless and left
    /// alone. The rating-bound ordering is always checked because an inverted
    /// bound is a mistake regardless.
    fn validate(&self, max_concurrent_games: u32) -> Result<()> {
        if self.min_rating > self.max_rating {
            return Err(Error::Config(
                "matchmaking.min_rating exceeds max_rating".into(),
            ));
        }
        if !self.enabled {
            return Ok(());
        }
        if self.variants.is_empty() {
            return Err(Error::Config(
                "matchmaking.variants is empty but matchmaking is enabled".into(),
            ));
        }
        if self.initial_seconds.is_empty() {
            return Err(Error::Config(
                "matchmaking.initial_seconds is empty but matchmaking is enabled".into(),
            ));
        }
        if self.increment_seconds.is_empty() {
            return Err(Error::Config(
                "matchmaking.increment_seconds is empty but matchmaking is enabled".into(),
            ));
        }
        // Every game slot reserved for humans is a slot matchmaking can never
        // use, so reserving the whole cap would leave matchmaking permanently
        // unable to start a game.
        if self.reserved_human_slots >= max_concurrent_games {
            return Err(Error::Config(
                "matchmaking.reserved_human_slots leaves no slot for matchmaking games".into(),
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
    /// When several acceptable challenges are pending at once and a game slot is
    /// scarce, accept human challengers before bots. Off by default, which keeps
    /// pending challenges in arrival order. Independent of
    /// [`MatchmakingConfig::reserved_human_slots`], which holds slots open for
    /// humans regardless of this ordering.
    pub prefer_human_challenges: bool,
    /// When non-empty, only these accounts may challenge the bot; every other
    /// challenger is declined before any other rule is consulted. Entries are
    /// account ids (the lowercase form of a username), matched case-insensitively.
    /// Empty (the default) imposes no allow list.
    pub allow_list: Vec<String>,
    /// Accounts that may never challenge the bot, declined before any other rule
    /// is consulted even if an allow list would otherwise admit them. Entries are
    /// account ids, matched case-insensitively. Empty by default.
    pub block_list: Vec<String>,
    /// Most simultaneous games one challenger may hold against the bot at once. A
    /// further challenge from an account already at this limit is declined with
    /// reason `later` rather than occupying another slot, so a single opponent
    /// cannot monopolise the board. `0` (the default) imposes no per-account
    /// limit; the overall [`Config::max_concurrent_games`] cap still applies.
    pub max_games_per_user: u32,
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
            prefer_human_challenges: false,
            allow_list: Vec::new(),
            block_list: Vec::new(),
            max_games_per_user: 0,
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
    fn matchmaking_is_disabled_by_default() {
        let m = &Config::default().matchmaking;
        assert!(!m.enabled);
        assert_eq!(m.mode, MatchmakingMode::Casual);
        assert_eq!(m.variants, vec!["standard".to_string()]);
        assert!(m.block_list.is_empty());
    }

    #[test]
    fn matchmaking_section_parses_and_rejects_unknown_keys() {
        let config = Config::parse(
            r#"
                max_concurrent_games = 4
                [matchmaking]
                enabled = true
                mode = "rated"
                initial_seconds = [60, 120]
                increment_seconds = [0]
                block_list = ["evilbot"]
                reserved_human_slots = 1
            "#,
        )
        .unwrap();
        assert!(config.matchmaking.enabled);
        assert_eq!(config.matchmaking.mode, MatchmakingMode::Rated);
        assert_eq!(config.matchmaking.initial_seconds, vec![60, 120]);
        assert_eq!(config.matchmaking.block_list, vec!["evilbot".to_string()]);

        let err = Config::parse("[matchmaking]\nno_such_key = 1").unwrap_err();
        assert!(matches!(err, Error::Config(_)));
    }

    #[test]
    fn enabled_matchmaking_with_empty_pool_is_rejected() {
        let err = Config::parse(
            r#"
                [matchmaking]
                enabled = true
                variants = []
            "#,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Config(_)));
    }

    #[test]
    fn disabled_matchmaking_tolerates_empty_pools() {
        // A section left disabled is inert, so an empty pool is not an error.
        let config = Config::parse(
            r#"
                [matchmaking]
                enabled = false
                variants = []
                initial_seconds = []
            "#,
        )
        .unwrap();
        assert!(!config.matchmaking.enabled);
    }

    #[test]
    fn reserving_every_slot_for_humans_is_rejected() {
        // With one game slot and one reserved for humans, matchmaking could never
        // start a game, so enabling it that way is a configuration error.
        let err = Config::parse(
            r#"
                max_concurrent_games = 1
                [matchmaking]
                enabled = true
                reserved_human_slots = 1
            "#,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Config(_)));
    }

    #[test]
    fn inverted_matchmaking_rating_bounds_are_rejected_even_when_disabled() {
        let err = Config::parse(
            r#"
                [matchmaking]
                min_rating = 2000
                max_rating = 1000
            "#,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Config(_)));
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
