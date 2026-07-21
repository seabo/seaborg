//! Proactive matchmaking: seeking games against other bots while idle.
//!
//! The reactive side of the bot waits for incoming challenges. Matchmaking is the
//! other direction: when the bot has been idle long enough it composes a challenge
//! from configured pools and sends it to an eligible online bot. All of the
//! decision logic lives here as pure methods on [`Matchmaker`] that take the
//! current time explicitly, so the timing, eligibility, and backoff rules can be
//! tested without a clock or the network. The event loop supplies the wall clock
//! and performs the actual HTTP calls.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::config::{MatchmakingConfig, MatchmakingMode};
use crate::error::{Error, Result};

/// A Lichess "speed" category, derived from a time control. The category selects
/// which of an opponent's ratings a rating bound is compared against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    /// Estimated duration at most 29 seconds.
    UltraBullet,
    /// Estimated duration at most 179 seconds.
    Bullet,
    /// Estimated duration at most 479 seconds.
    Blitz,
    /// Estimated duration at most 1499 seconds.
    Rapid,
    /// Estimated duration of 1500 seconds or more.
    Classical,
}

impl Speed {
    /// Classify a clock into a speed the way Lichess does: from an estimated game
    /// duration of the initial time plus forty increments.
    pub fn classify(initial_seconds: u32, increment_seconds: u32) -> Speed {
        let estimated = initial_seconds.saturating_add(increment_seconds.saturating_mul(40));
        match estimated {
            0..=29 => Speed::UltraBullet,
            30..=179 => Speed::Bullet,
            180..=479 => Speed::Blitz,
            480..=1499 => Speed::Rapid,
            _ => Speed::Classical,
        }
    }

    /// The `perfs` key Lichess uses for this speed.
    fn perf_key(self) -> &'static str {
        match self {
            Speed::UltraBullet => "ultraBullet",
            Speed::Bullet => "bullet",
            Speed::Blitz => "blitz",
            Speed::Rapid => "rapid",
            Speed::Classical => "classical",
        }
    }
}

/// A candidate opponent from `GET /api/bot/online`.
///
/// Only the fields matchmaking needs are modeled; unknown fields are ignored so an
/// API addition does not break parsing.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BotInfo {
    /// The account id, used to address a challenge and to match block-list and
    /// decline-backoff entries.
    pub id: String,
    /// The account title, if any. `BOT` marks a bot account.
    #[serde(default)]
    pub title: Option<String>,
    /// Ratings per speed, keyed by Lichess `perfs` key (e.g. `blitz`).
    #[serde(default)]
    pub perfs: HashMap<String, Perf>,
}

/// One entry of an account's `perfs` map.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Perf {
    /// The rating in this speed, if published.
    #[serde(default)]
    pub rating: Option<u32>,
}

impl BotInfo {
    /// Whether this account is a bot.
    fn is_bot(&self) -> bool {
        self.title.as_deref() == Some("BOT")
    }

    /// This account's rating in `speed`, if it has one.
    fn rating_for(&self, speed: Speed) -> Option<u32> {
        self.perfs.get(speed.perf_key()).and_then(|p| p.rating)
    }
}

/// Parse a single `GET /api/bot/online` NDJSON line into a [`BotInfo`].
///
/// Blank keepalive lines carry no bot and parse to `None`; a non-blank line that
/// is not valid JSON is an error.
pub fn parse_bot_line(line: &str) -> Result<Option<BotInfo>> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(line)
        .map(Some)
        .map_err(|e| Error::Decode(format!("bot online line: {e}")))
}

/// A composed challenge ready to be sent to a chosen opponent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeSpec {
    /// The variant key to challenge in.
    pub variant: String,
    /// The clock's initial time in seconds.
    pub initial_seconds: u32,
    /// The clock's increment in seconds.
    pub increment_seconds: u32,
    /// Whether the challenge is rated.
    pub rated: bool,
}

impl ChallengeSpec {
    /// The speed category of this spec's clock.
    fn speed(&self) -> Speed {
        Speed::classify(self.initial_seconds, self.increment_seconds)
    }
}

/// Whether the bot should seek a game right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Do nothing this tick.
    Idle,
    /// Compose and issue a challenge.
    Seek,
}

/// An outgoing challenge that has been issued and not yet resolved.
#[derive(Debug, Clone)]
struct Outstanding {
    /// When the challenge was sent, used to expire a challenge the opponent never
    /// answers so the bot eventually tries someone else.
    issued: Instant,
    /// The Lichess challenge id, kept so the challenge can be cancelled if it goes
    /// unanswered past the interval (a correspondence or kept-alive challenge does
    /// not auto-expire and would otherwise linger).
    challenge_id: String,
}

/// Matchmaking state and decisions for one bot session.
///
/// Holds the timing state (when the bot last became idle, last tried to seek, and
/// whether an issued challenge is still pending) and the per-opponent decline
/// backoff. Every method that depends on the clock takes `now` so the caller owns
/// the time source.
pub struct Matchmaker {
    config: MatchmakingConfig,
    /// The concurrency cap from the top-level config, needed to compute how many
    /// slots matchmaking may use once human-reserved slots are held back.
    max_concurrent_games: u32,
    /// This bot's own account id, excluded from its own opponent search.
    own_id: String,
    /// The most recent instant at which the bot was not idle (a game was in
    /// progress), from which the idle timeout is measured.
    idle_since: Instant,
    /// When the bot last attempted to seek a game, whether or not it found one.
    /// Gates the minimum interval between attempts.
    last_attempt: Option<Instant>,
    /// An issued challenge awaiting acceptance or decline, if any.
    outstanding: Option<Outstanding>,
    /// The id of an outstanding challenge that has just been abandoned because it
    /// went unanswered, waiting for the caller to cancel it on Lichess. Set by
    /// [`Matchmaker::choose`] and drained by [`Matchmaker::take_challenge_to_cancel`].
    to_cancel: Option<String>,
    /// Bot id -> the instant a decline was recorded; a re-challenge is suppressed
    /// until the configured backoff elapses.
    declined_at: HashMap<String, Instant>,
    /// Rotation cursor over the variant pool.
    variant_cursor: usize,
    /// Rotation cursor over the initial-time pool.
    initial_cursor: usize,
    /// Rotation cursor over the increment pool.
    increment_cursor: usize,
    /// Toggles rated/casual for [`MatchmakingMode::Random`].
    rated_toggle: bool,
    /// State for the internal PRNG that randomises opponent selection, so the bot
    /// does not fixate on the first eligible bot each interval. Seeded from system
    /// entropy in production via [`Matchmaker::with_seed`] and left at a fixed
    /// default otherwise, which keeps tests deterministic.
    rng_state: u64,
}

impl Matchmaker {
    /// Build a matchmaker for the given configuration, concurrency cap, and own
    /// account id. `now` seeds the idle clock so the bot must be idle for the full
    /// timeout after startup before its first challenge.
    pub fn new(
        config: MatchmakingConfig,
        max_concurrent_games: u32,
        own_id: impl Into<String>,
        now: Instant,
    ) -> Matchmaker {
        Matchmaker {
            config,
            max_concurrent_games,
            own_id: own_id.into(),
            idle_since: now,
            last_attempt: None,
            outstanding: None,
            to_cancel: None,
            declined_at: HashMap::new(),
            variant_cursor: 0,
            initial_cursor: 0,
            increment_cursor: 0,
            rated_toggle: false,
            // A fixed default so `new` alone is deterministic (tests rely on this);
            // production overrides it with system entropy via `with_seed`.
            rng_state: 0,
        }
    }

    /// Seed the opponent-selection PRNG, the injectable seam that makes random
    /// selection vary between runs while remaining reproducible from a fixed seed.
    ///
    /// Production seeds this from system entropy so successive bot sessions do not
    /// challenge opponents in the same order; tests pass a fixed seed to assert both
    /// that selection spreads across eligible candidates and that eligibility
    /// filtering still holds.
    pub fn with_seed(mut self, seed: u64) -> Matchmaker {
        self.rng_state = seed;
        self
    }

    /// A disabled matchmaker that never seeks a game, for the reactive-only path
    /// and for tests that do not exercise matchmaking.
    pub fn disabled() -> Matchmaker {
        // The clock seed is irrelevant: a disabled matchmaker returns `Idle`
        // before consulting any timing state. The default config is already
        // disabled.
        Matchmaker::new(
            MatchmakingConfig::default(),
            1,
            String::new(),
            Instant::now(),
        )
    }

    /// Whether matchmaking is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// How many concurrent games matchmaking may occupy, after holding back the
    /// slots reserved for human challengers.
    fn matchmaking_cap(&self) -> u32 {
        self.max_concurrent_games
            .saturating_sub(self.config.reserved_human_slots)
    }

    /// Decide whether to seek a game this tick, given the wall clock and how many
    /// games are in progress.
    ///
    /// Returns [`Action::Seek`] only when matchmaking is enabled, a game slot is
    /// free after human reservations, no issued challenge is still pending, the
    /// configured idle timeout has passed since the last game started, and the
    /// minimum interval since the last attempt has elapsed. Because the cap is
    /// reduced by the reserved human slots, matchmaking may stack games up to that
    /// reduced cap while still leaving room for a human to challenge the bot.
    ///
    /// The pending-challenge check runs before the cap check so an unanswered
    /// challenge is abandoned (and offered for cancellation) even while the board
    /// is temporarily full, rather than lingering until a slot frees.
    pub fn choose(&mut self, now: Instant, active_games: u32) -> Action {
        if !self.config.enabled {
            return Action::Idle;
        }
        if let Some(outstanding) = &self.outstanding {
            // A challenge is still pending. Give up on it once a full interval has
            // passed with no game starting, so an unanswered challenge does not
            // block matchmaking forever.
            if now.duration_since(outstanding.issued) < self.min_interval() {
                return Action::Idle;
            }
            // Abandon it and remember its id so the caller can cancel it on Lichess
            // rather than leaving a zombie challenge outstanding.
            let abandoned = self.outstanding.take().expect("outstanding checked above");
            self.to_cancel = Some(abandoned.challenge_id);
        }
        if active_games >= self.matchmaking_cap() {
            return Action::Idle;
        }
        if now.duration_since(self.idle_since) < self.idle_timeout() {
            return Action::Idle;
        }
        if let Some(last) = self.last_attempt {
            if now.duration_since(last) < self.min_interval() {
                return Action::Idle;
            }
        }
        Action::Seek
    }

    /// Compose the next challenge from the configured pools, advancing the pool
    /// cursors so successive challenges vary. Rated/casual follows the configured
    /// mode, alternating when the mode is random.
    ///
    /// The pools are non-empty whenever matchmaking is enabled (enforced at config
    /// load), so the indexing here cannot go out of bounds on the enabled path.
    pub fn compose_spec(&mut self) -> ChallengeSpec {
        let variant = pick(&self.config.variants, &mut self.variant_cursor)
            .cloned()
            .unwrap_or_else(|| "standard".to_string());
        let initial_seconds = pick(&self.config.initial_seconds, &mut self.initial_cursor)
            .copied()
            .unwrap_or(60);
        let increment_seconds = pick(&self.config.increment_seconds, &mut self.increment_cursor)
            .copied()
            .unwrap_or(0);
        let rated = match self.config.mode {
            MatchmakingMode::Rated => true,
            MatchmakingMode::Casual => false,
            MatchmakingMode::Random => {
                let rated = self.rated_toggle;
                self.rated_toggle = !self.rated_toggle;
                rated
            }
        };
        ChallengeSpec {
            variant,
            initial_seconds,
            increment_seconds,
            rated,
        }
    }

    /// Choose an eligible opponent for `spec` from the online bots, or `None` when
    /// none qualify.
    ///
    /// A candidate qualifies when it is a bot other than this account, is not on
    /// the block list, is not currently in decline backoff, and has a rating for
    /// the spec's speed within the configured bounds. A candidate with no rating
    /// for that speed is skipped, since its eligibility against the bounds cannot
    /// be confirmed. One qualifying candidate is chosen at random, so an unchanging
    /// online pool does not make the bot re-challenge the same opponent every
    /// interval until it declines; the choice draws on the seeded PRNG, so it is
    /// reproducible for a fixed seed.
    pub fn select_opponent<'a>(
        &mut self,
        spec: &ChallengeSpec,
        candidates: &'a [BotInfo],
        now: Instant,
    ) -> Option<&'a BotInfo> {
        let speed = spec.speed();
        let eligible: Vec<&'a BotInfo> = candidates
            .iter()
            .filter(|bot| {
                bot.is_bot()
                    && bot.id != self.own_id
                    && !self.is_blocked(&bot.id)
                    && !self.in_decline_backoff(&bot.id, now)
                    && bot
                        .rating_for(speed)
                        .is_some_and(|rating| self.rating_in_bounds(rating))
            })
            .collect();
        if eligible.is_empty() {
            return None;
        }
        let index = (self.next_rand() % eligible.len() as u64) as usize;
        Some(eligible[index])
    }

    /// Record that a challenge with id `challenge_id` was just issued: it starts
    /// the pending-challenge window and the minimum-interval clock, and remembers
    /// the id so an unanswered challenge can be cancelled when it is abandoned.
    pub fn record_issued(&mut self, now: Instant, challenge_id: impl Into<String>) {
        self.last_attempt = Some(now);
        self.outstanding = Some(Outstanding {
            issued: now,
            challenge_id: challenge_id.into(),
        });
    }

    /// Take the id of a challenge that was abandoned unanswered and needs to be
    /// cancelled on Lichess, if any. Returns `Some` at most once per abandonment;
    /// the caller performs the cancel outside the matchmaker lock.
    pub fn take_challenge_to_cancel(&mut self) -> Option<String> {
        self.to_cancel.take()
    }

    /// Record a seek attempt that found no opponent, so the next attempt still
    /// waits the minimum interval rather than re-querying every keepalive.
    pub fn record_attempt(&mut self, now: Instant) {
        self.last_attempt = Some(now);
    }

    /// Record that `bot_id` declined a challenge, starting its decline backoff and
    /// clearing any pending challenge.
    pub fn record_declined(&mut self, bot_id: &str, now: Instant) {
        self.outstanding = None;
        self.start_backoff(bot_id, now);
    }

    /// Record that an attempt to challenge `bot_id` failed before any game began —
    /// most often the challenge was rejected at creation (an HTTP error), rather
    /// than declined by the opponent.
    ///
    /// Without this, nothing marks a bot that just refused a challenge, so random
    /// selection could keep re-picking the same unreachable bot from a small pool
    /// and make no progress. Applying the same backoff a decline uses removes it
    /// from the eligible set so matchmaking moves on to a different opponent.
    pub fn record_challenge_failed(&mut self, bot_id: &str, now: Instant) {
        self.start_backoff(bot_id, now);
    }

    /// Put `bot_id` into the per-opponent backoff, so it is skipped until the
    /// configured window elapses. Opportunistically drops entries whose backoff
    /// has already passed, so the map does not grow without bound over a long
    /// session.
    fn start_backoff(&mut self, bot_id: &str, now: Instant) {
        let backoff = self.decline_backoff();
        self.declined_at
            .retain(|_, at| now.duration_since(*at) < backoff);
        self.declined_at.insert(bot_id.to_string(), now);
    }

    /// Record that a game started, clearing any pending challenge (the opponent
    /// accepted, or a human's challenge was accepted) and restarting the idle
    /// clock, so the bot waits the idle timeout after this game before seeking
    /// another and does not dogpile challenges as games start.
    pub fn record_game_started(&mut self, now: Instant) {
        self.outstanding = None;
        self.idle_since = now;
    }

    /// Whether `bot_id` is on the configured block list.
    fn is_blocked(&self, bot_id: &str) -> bool {
        self.config.block_list.iter().any(|b| b == bot_id)
    }

    /// Whether `bot_id` is still within its decline backoff window.
    fn in_decline_backoff(&self, bot_id: &str, now: Instant) -> bool {
        self.declined_at
            .get(bot_id)
            .is_some_and(|at| now.duration_since(*at) < self.decline_backoff())
    }

    /// Whether `rating` is within the configured opponent bounds.
    fn rating_in_bounds(&self, rating: u32) -> bool {
        rating >= self.config.min_rating && rating <= self.config.max_rating
    }

    fn idle_timeout(&self) -> Duration {
        Duration::from_secs(self.config.idle_timeout_seconds)
    }

    fn min_interval(&self) -> Duration {
        Duration::from_secs(self.config.min_challenge_interval_seconds)
    }

    fn decline_backoff(&self) -> Duration {
        Duration::from_secs(self.config.decline_backoff_seconds)
    }

    /// Draw the next pseudo-random value, advancing the PRNG. This is SplitMix64,
    /// a tiny well-distributed generator used here only to spread opponent
    /// selection; it is not cryptographic and pulls in no dependency.
    fn next_rand(&mut self) -> u64 {
        self.rng_state = self.rng_state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng_state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Return the pool element at `*cursor`, then advance the cursor with wraparound.
/// Returns `None` only for an empty pool.
fn pick<'a, T>(pool: &'a [T], cursor: &mut usize) -> Option<&'a T> {
    if pool.is_empty() {
        return None;
    }
    let index = *cursor % pool.len();
    *cursor = index + 1;
    Some(&pool[index])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_config() -> MatchmakingConfig {
        MatchmakingConfig {
            enabled: true,
            variants: vec!["standard".to_string()],
            initial_seconds: vec![300],
            increment_seconds: vec![0],
            min_rating: 1000,
            max_rating: 2000,
            mode: MatchmakingMode::Casual,
            idle_timeout_seconds: 30,
            min_challenge_interval_seconds: 60,
            reserved_human_slots: 0,
            block_list: Vec::new(),
            decline_backoff_seconds: 3600,
        }
    }

    fn bot(id: &str, speed: Speed, rating: u32) -> BotInfo {
        let mut perfs = HashMap::new();
        perfs.insert(
            speed.perf_key().to_string(),
            Perf {
                rating: Some(rating),
            },
        );
        BotInfo {
            id: id.to_string(),
            title: Some("BOT".to_string()),
            perfs,
        }
    }

    #[test]
    fn speed_classifies_from_estimated_duration() {
        assert_eq!(Speed::classify(15, 0), Speed::UltraBullet);
        assert_eq!(Speed::classify(60, 0), Speed::Bullet);
        assert_eq!(Speed::classify(180, 0), Speed::Blitz);
        assert_eq!(Speed::classify(300, 3), Speed::Blitz); // 300 + 120 = 420
        assert_eq!(Speed::classify(600, 0), Speed::Rapid);
        assert_eq!(Speed::classify(1800, 0), Speed::Classical);
    }

    #[test]
    fn disabled_matchmaker_never_seeks() {
        let mut mm = Matchmaker::disabled();
        assert!(!mm.is_enabled());
        let now = Instant::now();
        assert_eq!(
            mm.choose(now + Duration::from_secs(10_000), 0),
            Action::Idle
        );
    }

    #[test]
    fn seeks_only_after_idle_timeout_elapses() {
        let start = Instant::now();
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", start);
        // Not idle long enough yet.
        assert_eq!(mm.choose(start + Duration::from_secs(20), 0), Action::Idle);
        // Past the 30s idle timeout with a free slot: seek.
        assert_eq!(mm.choose(start + Duration::from_secs(31), 0), Action::Seek);
    }

    #[test]
    fn a_full_cap_blocks_and_a_game_start_restarts_the_idle_clock() {
        let start = Instant::now();
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", start);
        // A game is running: the single slot is full, so no seeking.
        assert_eq!(mm.choose(start + Duration::from_secs(40), 1), Action::Idle);
        // That game started at t=40, restarting the idle clock.
        mm.record_game_started(start + Duration::from_secs(40));
        // Idle again but only 20s since the game started: still too soon.
        assert_eq!(mm.choose(start + Duration::from_secs(60), 0), Action::Idle);
        // A full idle timeout after the game started: seek.
        assert_eq!(mm.choose(start + Duration::from_secs(71), 0), Action::Seek);
    }

    #[test]
    fn reserved_human_slots_hold_matchmaking_below_the_cap() {
        let config = MatchmakingConfig {
            reserved_human_slots: 1,
            ..enabled_config()
        };
        let start = Instant::now();
        // Two total slots, one reserved for humans: matchmaking may use one.
        let mut mm = Matchmaker::new(config, 2, "me", start);
        let idle = start + Duration::from_secs(31);
        // One game running already fills matchmaking's single usable slot, even
        // though a second raw slot is free — that one is reserved for humans.
        assert_eq!(mm.choose(idle, 1), Action::Idle);
        // With no games running, the matchmaking slot is free.
        assert_eq!(mm.choose(idle, 0), Action::Seek);
    }

    #[test]
    fn minimum_interval_spaces_attempts() {
        let start = Instant::now();
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", start);
        let first = start + Duration::from_secs(31);
        assert_eq!(mm.choose(first, 0), Action::Seek);
        mm.record_attempt(first);
        // 60s interval not yet elapsed.
        assert_eq!(mm.choose(first + Duration::from_secs(59), 0), Action::Idle);
        // Interval elapsed: seek again.
        assert_eq!(mm.choose(first + Duration::from_secs(61), 0), Action::Seek);
    }

    #[test]
    fn an_issued_challenge_blocks_until_it_lapses() {
        let start = Instant::now();
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", start);
        let issued = start + Duration::from_secs(31);
        assert_eq!(mm.choose(issued, 0), Action::Seek);
        mm.record_issued(issued, "c1");
        // Within the interval, the pending challenge blocks another seek.
        assert_eq!(mm.choose(issued + Duration::from_secs(30), 0), Action::Idle);
        // After the interval the pending challenge is abandoned and, the interval
        // since the last attempt having also elapsed, seeking resumes.
        assert_eq!(mm.choose(issued + Duration::from_secs(61), 0), Action::Seek);
    }

    #[test]
    fn game_start_clears_a_pending_challenge() {
        let start = Instant::now();
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", start);
        let issued = start + Duration::from_secs(31);
        mm.record_issued(issued, "c1");
        // The challenge is accepted and the game starts, restarting the idle clock.
        mm.record_game_started(issued);
        // While the game runs (active=1) the single slot is full.
        assert_eq!(mm.choose(issued + Duration::from_secs(1), 1), Action::Idle);
        // When it ends, a fresh idle timeout must pass before seeking again.
        assert_eq!(mm.choose(issued + Duration::from_secs(20), 0), Action::Idle);
        assert_eq!(mm.choose(issued + Duration::from_secs(61), 0), Action::Seek);
    }

    #[test]
    fn selects_an_eligible_bot_within_rating_bounds() {
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", Instant::now());
        let spec = ChallengeSpec {
            variant: "standard".to_string(),
            initial_seconds: 300,
            increment_seconds: 0,
            rated: false,
        };
        let candidates = vec![
            bot("tooweak", Speed::Blitz, 500),    // below min_rating
            bot("toostrong", Speed::Blitz, 2500), // above max_rating
            bot("justright", Speed::Blitz, 1500),
        ];
        // Only one candidate is within bounds, so the random pick must land on it.
        let chosen = mm.select_opponent(&spec, &candidates, Instant::now());
        assert_eq!(chosen.map(|b| b.id.as_str()), Some("justright"));
    }

    #[test]
    fn selection_skips_self_block_list_and_non_bots() {
        let config = MatchmakingConfig {
            block_list: vec!["blocked".to_string()],
            ..enabled_config()
        };
        let mut mm = Matchmaker::new(config, 1, "me", Instant::now());
        let spec = ChallengeSpec {
            variant: "standard".to_string(),
            initial_seconds: 300,
            increment_seconds: 0,
            rated: false,
        };
        let mut human = bot("human", Speed::Blitz, 1500);
        human.title = None;
        let candidates = vec![
            bot("me", Speed::Blitz, 1500),
            bot("blocked", Speed::Blitz, 1500),
            human,
            bot("ok", Speed::Blitz, 1500),
        ];
        // Self, the blocked bot, and the non-bot are all ineligible, leaving `ok`
        // as the only valid pick.
        let chosen = mm.select_opponent(&spec, &candidates, Instant::now());
        assert_eq!(chosen.map(|b| b.id.as_str()), Some("ok"));
    }

    #[test]
    fn a_candidate_without_a_rating_for_the_speed_is_skipped() {
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", Instant::now());
        let spec = ChallengeSpec {
            variant: "standard".to_string(),
            initial_seconds: 300, // blitz
            increment_seconds: 0,
            rated: false,
        };
        // The candidate has a bullet rating but none for blitz, the spec's speed.
        let only = vec![bot("bulletonly", Speed::Bullet, 1500)];
        assert!(mm.select_opponent(&spec, &only, Instant::now()).is_none());
    }

    #[test]
    fn a_declined_bot_is_skipped_until_the_backoff_elapses() {
        let start = Instant::now();
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", start);
        let spec = ChallengeSpec {
            variant: "standard".to_string(),
            initial_seconds: 300,
            increment_seconds: 0,
            rated: false,
        };
        let candidates = vec![bot("fussy", Speed::Blitz, 1500)];
        mm.record_declined("fussy", start);
        // Within the 3600s backoff, the decliner is skipped.
        assert!(mm
            .select_opponent(&spec, &candidates, start + Duration::from_secs(100))
            .is_none());
        // After the backoff, it is eligible again.
        assert_eq!(
            mm.select_opponent(&spec, &candidates, start + Duration::from_secs(3601))
                .map(|b| b.id.as_str()),
            Some("fussy")
        );
    }

    #[test]
    fn a_failed_challenge_makes_selection_move_to_another_bot() {
        // A bot whose challenge fails is put into backoff, so it is skipped until
        // the window elapses. With two bots, that leaves the other as the only
        // eligible pick — proving the failure moved selection on rather than
        // re-picking the unreachable bot.
        let start = Instant::now();
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", start);
        let spec = ChallengeSpec {
            variant: "standard".to_string(),
            initial_seconds: 300,
            increment_seconds: 0,
            rated: false,
        };
        let candidates = vec![
            bot("first", Speed::Blitz, 1500),
            bot("second", Speed::Blitz, 1500),
        ];
        let first_pick = mm
            .select_opponent(&spec, &candidates, start)
            .map(|b| b.id.clone())
            .expect("both bots are eligible, so one is chosen");
        mm.record_challenge_failed(&first_pick, start);
        // Within the backoff the failed bot is skipped, so the other one is chosen.
        let second_pick = mm
            .select_opponent(&spec, &candidates, start + Duration::from_secs(1))
            .map(|b| b.id.clone())
            .expect("the un-penalised bot is still eligible");
        assert_ne!(second_pick, first_pick);
        // Once the backoff elapses the penalised bot is eligible again, so a pick
        // is once more available from the full pool.
        assert!(mm
            .select_opponent(&spec, &candidates, start + Duration::from_secs(3601))
            .is_some());
    }

    #[test]
    fn random_mode_alternates_rated_and_casual() {
        let config = MatchmakingConfig {
            mode: MatchmakingMode::Random,
            ..enabled_config()
        };
        let mut mm = Matchmaker::new(config, 1, "me", Instant::now());
        assert!(!mm.compose_spec().rated);
        assert!(mm.compose_spec().rated);
        assert!(!mm.compose_spec().rated);
    }

    #[test]
    fn compose_cycles_through_the_pools() {
        let config = MatchmakingConfig {
            initial_seconds: vec![60, 180],
            increment_seconds: vec![0, 2],
            mode: MatchmakingMode::Casual,
            ..enabled_config()
        };
        let mut mm = Matchmaker::new(config, 1, "me", Instant::now());
        let first = mm.compose_spec();
        let second = mm.compose_spec();
        let third = mm.compose_spec();
        assert_eq!((first.initial_seconds, first.increment_seconds), (60, 0));
        assert_eq!((second.initial_seconds, second.increment_seconds), (180, 2));
        // The cursor wraps back to the pool start.
        assert_eq!((third.initial_seconds, third.increment_seconds), (60, 0));
    }

    #[test]
    fn selection_spreads_across_eligible_candidates() {
        // Over many draws against an unchanging eligible pool, selection must not
        // fixate on one bot. With a fixed seed the run is reproducible, so this is a
        // stable assertion about spread rather than a flaky one.
        let spec = ChallengeSpec {
            variant: "standard".to_string(),
            initial_seconds: 300,
            increment_seconds: 0,
            rated: false,
        };
        let candidates = vec![
            bot("alpha", Speed::Blitz, 1500),
            bot("bravo", Speed::Blitz, 1500),
            bot("charlie", Speed::Blitz, 1500),
        ];
        let now = Instant::now();
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", now).with_seed(0xC0FF_EE00);
        let mut chosen = std::collections::HashSet::new();
        for _ in 0..30 {
            let pick = mm
                .select_opponent(&spec, &candidates, now)
                .expect("every candidate is eligible");
            // Only ever picks an eligible candidate.
            assert!(candidates.iter().any(|c| c.id == pick.id));
            chosen.insert(pick.id.clone());
        }
        assert!(
            chosen.len() > 1,
            "selection must spread across eligible bots, got only {chosen:?}"
        );
    }

    #[test]
    fn the_seed_makes_selection_reproducible_and_injectable() {
        // Two matchmakers with the same seed pick the same sequence; a different
        // seed can pick a different first opponent. This is the seam tests use to
        // drive selection deterministically and production uses to vary it per run.
        let spec = ChallengeSpec {
            variant: "standard".to_string(),
            initial_seconds: 300,
            increment_seconds: 0,
            rated: false,
        };
        let candidates: Vec<BotInfo> = (0..8)
            .map(|i| bot(&format!("bot{i}"), Speed::Blitz, 1500))
            .collect();
        let now = Instant::now();
        let pick = |seed: u64| -> String {
            let mut mm = Matchmaker::new(enabled_config(), 1, "me", now).with_seed(seed);
            mm.select_opponent(&spec, &candidates, now)
                .unwrap()
                .id
                .clone()
        };
        assert_eq!(pick(42), pick(42), "same seed is reproducible");
        assert_ne!(
            pick(1),
            pick(2),
            "distinct seeds can select distinct opponents"
        );
    }

    #[test]
    fn an_abandoned_challenge_is_offered_for_cancellation_by_id() {
        // When an outstanding challenge lapses unanswered, `choose` abandons it and
        // surfaces its id exactly once so the caller can cancel it on Lichess.
        let start = Instant::now();
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", start);
        let issued = start + Duration::from_secs(31);
        assert_eq!(mm.choose(issued, 0), Action::Seek);
        mm.record_issued(issued, "zombie1");
        // Nothing to cancel while the challenge is still pending.
        assert_eq!(mm.choose(issued + Duration::from_secs(10), 0), Action::Idle);
        assert_eq!(mm.take_challenge_to_cancel(), None);
        // Past the interval the challenge is abandoned and its id offered once.
        assert_eq!(mm.choose(issued + Duration::from_secs(61), 0), Action::Seek);
        assert_eq!(mm.take_challenge_to_cancel().as_deref(), Some("zombie1"));
        assert_eq!(mm.take_challenge_to_cancel(), None);
    }

    #[test]
    fn a_lapsed_challenge_is_cancelled_even_while_the_cap_is_full() {
        // A full board must not delay cancelling an unanswered challenge: the
        // pending-challenge check runs before the cap check, so the id is offered
        // even though `choose` then idles for want of a free slot.
        let start = Instant::now();
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", start);
        let issued = start + Duration::from_secs(31);
        assert_eq!(mm.choose(issued, 0), Action::Seek);
        mm.record_issued(issued, "zombie2");
        // A game now occupies the only slot, so seeking idles, yet the lapsed
        // challenge is still abandoned and offered for cancellation.
        let later = issued + Duration::from_secs(61);
        assert_eq!(mm.choose(later, 1), Action::Idle);
        assert_eq!(mm.take_challenge_to_cancel().as_deref(), Some("zombie2"));
    }

    #[test]
    fn a_challenge_resolved_by_a_game_start_is_not_cancelled() {
        // A challenge the opponent accepted (a game started) must not be cancelled;
        // only an unanswered, abandoned one is.
        let start = Instant::now();
        let mut mm = Matchmaker::new(enabled_config(), 1, "me", start);
        let issued = start + Duration::from_secs(31);
        mm.record_issued(issued, "accepted1");
        mm.record_game_started(issued);
        assert_eq!(mm.take_challenge_to_cancel(), None);
    }

    #[test]
    fn parse_bot_line_reads_id_title_and_perfs() {
        let line =
            r#"{"id":"maia","username":"maia","title":"BOT","perfs":{"blitz":{"rating":1700}}}"#;
        let bot = parse_bot_line(line).unwrap().unwrap();
        assert_eq!(bot.id, "maia");
        assert!(bot.is_bot());
        assert_eq!(bot.rating_for(Speed::Blitz), Some(1700));
        assert_eq!(bot.rating_for(Speed::Bullet), None);
    }

    #[test]
    fn parse_bot_line_treats_blank_as_keepalive() {
        assert_eq!(parse_bot_line("   ").unwrap(), None);
        assert!(parse_bot_line("{not json").is_err());
    }
}
