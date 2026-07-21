//! Challenge-acceptance decisions.
//!
//! [`classify`] compares an incoming challenge against the configured policy,
//! producing an accept-or-decline decision with a Lichess decline reason when it
//! declines. The concurrency cap and human-slot reservation are applied
//! elsewhere, when a slot is actually claimed, since those depend on the other
//! challenges pending at the same moment.

use crate::config::ChallengePolicy;
use crate::event::{Challenge, TimeControl};

/// The outcome of evaluating a challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Accept the challenge.
    Accept,
    /// Decline the challenge, reporting `reason` to Lichess.
    Decline(DeclineReason),
}

/// A Lichess decline reason.
///
/// These map to the fixed set of reasons the decline endpoint accepts; each
/// serializes to the exact string Lichess expects via [`DeclineReason::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclineReason {
    /// No more specific reason applies, or the bot is at its game cap.
    Generic,
    /// The bot is not accepting challenges right now — used for anti-flood limits
    /// such as an opponent already at their per-account simultaneous-game cap.
    Later,
    /// The bot does not play this variant.
    Variant,
    /// The bot does not play standard chess (it is a variant-only bot). Reported
    /// instead of `Variant` when the declined challenge is standard, matching the
    /// distinct reason Lichess offers.
    Standard,
    /// The bot does not play this time control category, for a reason not captured
    /// by the finer too-fast/too-slow reasons (e.g. an out-of-range increment or a
    /// correspondence/unlimited control).
    TimeControl,
    /// The challenge's clock is faster than the bot accepts.
    TooFast,
    /// The challenge's clock is slower than the bot accepts.
    TooSlow,
    /// The bot does not play rated games under the current policy.
    Rated,
    /// The bot does not play casual games under the current policy.
    Casual,
    /// The bot does not accept challenges from other bots.
    NoBot,
    /// The bot only accepts challenges from other bots.
    OnlyBot,
}

impl DeclineReason {
    /// The wire string Lichess expects in the `reason` form field.
    pub fn as_str(self) -> &'static str {
        match self {
            DeclineReason::Generic => "generic",
            DeclineReason::Later => "later",
            DeclineReason::Variant => "variant",
            DeclineReason::Standard => "standard",
            DeclineReason::TimeControl => "timeControl",
            DeclineReason::TooFast => "tooFast",
            DeclineReason::TooSlow => "tooSlow",
            DeclineReason::Rated => "rated",
            DeclineReason::Casual => "casual",
            DeclineReason::NoBot => "noBot",
            DeclineReason::OnlyBot => "onlyBot",
        }
    }
}

/// Decide whether `challenge` is one the bot is willing to play, ignoring how
/// many games are already in progress.
///
/// This is the policy-suitability half of the decision: the allow/block lists,
/// then variant, time control, rated/casual, opponent kind, and rating. The
/// concurrency-cap, per-account-limit, and human-reservation half is applied
/// separately when a slot is actually claimed, because that depends on the other
/// challenges pending at the same moment and on which slots are held for humans.
/// The list overrides come first, then the remaining checks run from the most
/// specific decline reason to the least so the challenger gets the most useful
/// explanation.
pub fn classify(challenge: &Challenge, policy: &ChallengePolicy) -> Decision {
    // Explicit allow/block lists are consulted first: a blocked account is refused
    // whatever it sends, and once an allow list is configured only listed accounts
    // are considered at all. Matched by account id (a lowercase username), so the
    // comparison is case-insensitive to tolerate a display-cased config entry.
    let challenger_id = challenge.challenger.id.as_str();
    if policy
        .block_list
        .iter()
        .any(|b| b.eq_ignore_ascii_case(challenger_id))
    {
        return Decision::Decline(DeclineReason::Generic);
    }
    if !policy.allow_list.is_empty()
        && !policy
            .allow_list
            .iter()
            .any(|a| a.eq_ignore_ascii_case(challenger_id))
    {
        return Decision::Decline(DeclineReason::Generic);
    }

    if !policy.allows_variant(&challenge.variant.key) {
        // A standard challenge a variant-only bot cannot play gets the distinct
        // `standard` reason; any other unsupported variant gets `variant`.
        let reason = if challenge.variant.key == "standard" {
            DeclineReason::Standard
        } else {
            DeclineReason::Variant
        };
        return Decision::Decline(reason);
    }

    match &challenge.time_control {
        TimeControl::Clock { limit, increment } => {
            // Report which side of the clock bounds the challenge fell outside, so
            // the challenger can adjust: too fast below the minimum initial time,
            // too slow above the maximum. An out-of-range increment is not a
            // fast/slow judgement, so it keeps the generic time-control reason.
            if *limit < policy.min_initial_seconds {
                return Decision::Decline(DeclineReason::TooFast);
            }
            if *limit > policy.max_initial_seconds {
                return Decision::Decline(DeclineReason::TooSlow);
            }
            if *increment < policy.min_increment_seconds
                || *increment > policy.max_increment_seconds
            {
                return Decision::Decline(DeclineReason::TimeControl);
            }
        }
        TimeControl::Correspondence { .. } | TimeControl::Unlimited => {
            if !policy.accept_unlimited {
                return Decision::Decline(DeclineReason::TimeControl);
            }
        }
    }

    // A mode mismatch names the mode the bot *does* offer instead of the one it
    // refuses, so the challenger can re-send in a mode that would be accepted: a
    // rated challenge a casual-only bot declines reports `casual`, and the reverse
    // reports `rated`.
    if challenge.rated && !policy.accept_rated {
        return Decision::Decline(DeclineReason::Casual);
    }
    if !challenge.rated && !policy.accept_casual {
        return Decision::Decline(DeclineReason::Rated);
    }

    if challenge.challenger.is_bot() {
        if !policy.accept_bots {
            return Decision::Decline(DeclineReason::NoBot);
        }
    } else if !policy.accept_humans {
        return Decision::Decline(DeclineReason::OnlyBot);
    }

    if let Some(rating) = challenge.challenger.rating {
        if rating < policy.min_rating || rating > policy.max_rating {
            return Decision::Decline(DeclineReason::Generic);
        }
    }

    Decision::Accept
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ChallengePolicy;
    use crate::event::{Challenge, Challenger, TimeControl, Variant};

    /// Build a challenge that the default policy accepts, so each test can vary
    /// one attribute and observe the resulting decision.
    fn acceptable() -> Challenge {
        Challenge {
            id: "id".to_string(),
            rated: false,
            variant: Variant {
                key: "standard".to_string(),
            },
            time_control: TimeControl::Clock {
                limit: 300,
                increment: 3,
            },
            challenger: Challenger {
                id: "alice".to_string(),
                name: "alice".to_string(),
                rating: Some(1500),
                title: None,
            },
            direction: None,
        }
    }

    fn evaluate_default(challenge: &Challenge) -> Decision {
        classify(challenge, &ChallengePolicy::default())
    }

    #[test]
    fn baseline_challenge_is_accepted() {
        assert_eq!(evaluate_default(&acceptable()), Decision::Accept);
    }

    #[test]
    fn wrong_variant_is_declined() {
        let mut c = acceptable();
        c.variant.key = "chess960".to_string();
        assert_eq!(
            evaluate_default(&c),
            Decision::Decline(DeclineReason::Variant)
        );
    }

    #[test]
    fn a_clock_below_the_minimum_is_too_fast() {
        let mut c = acceptable();
        c.time_control = TimeControl::Clock {
            limit: 5,
            increment: 0,
        };
        assert_eq!(
            evaluate_default(&c),
            Decision::Decline(DeclineReason::TooFast)
        );
    }

    #[test]
    fn a_clock_above_the_maximum_is_too_slow() {
        let mut c = acceptable();
        c.time_control = TimeControl::Clock {
            limit: 5400, // 90 minutes, above the 1800s default maximum
            increment: 0,
        };
        assert_eq!(
            evaluate_default(&c),
            Decision::Decline(DeclineReason::TooSlow)
        );
    }

    #[test]
    fn an_out_of_range_increment_keeps_the_generic_time_control_reason() {
        // The initial time is in range, so this is neither too fast nor too slow;
        // only the increment is out of bounds, which is not a fast/slow judgement.
        let mut c = acceptable();
        c.time_control = TimeControl::Clock {
            limit: 300,
            increment: 120, // above the 60s default maximum increment
        };
        assert_eq!(
            evaluate_default(&c),
            Decision::Decline(DeclineReason::TimeControl)
        );
    }

    #[test]
    fn a_standard_challenge_a_variant_only_bot_declines_reports_standard() {
        // A bot configured for chess960 only, sent a standard challenge, reports
        // the distinct `standard` reason rather than `variant`.
        let policy = ChallengePolicy {
            variants: vec!["chess960".to_string()],
            ..ChallengePolicy::default()
        };
        assert_eq!(
            classify(&acceptable(), &policy),
            Decision::Decline(DeclineReason::Standard)
        );
    }

    #[test]
    fn unlimited_is_declined_by_default_but_allowed_when_opted_in() {
        let mut c = acceptable();
        c.time_control = TimeControl::Unlimited;
        assert_eq!(
            evaluate_default(&c),
            Decision::Decline(DeclineReason::TimeControl)
        );

        let policy = ChallengePolicy {
            accept_unlimited: true,
            ..ChallengePolicy::default()
        };
        assert_eq!(classify(&c, &policy), Decision::Accept);
    }

    #[test]
    fn a_mode_mismatch_offers_the_mode_the_bot_accepts() {
        // A casual-only bot declines a rated challenge with `casual` (the mode it
        // would play), and a rated-only bot declines a casual challenge with
        // `rated`, so the challenger can re-send in the accepted mode.
        let mut rated = acceptable();
        rated.rated = true;
        let casual_only = ChallengePolicy {
            accept_rated: false,
            ..ChallengePolicy::default()
        };
        assert_eq!(
            classify(&rated, &casual_only),
            Decision::Decline(DeclineReason::Casual)
        );

        let rated_only = ChallengePolicy {
            accept_casual: false,
            ..ChallengePolicy::default()
        };
        assert_eq!(
            classify(&acceptable(), &rated_only),
            Decision::Decline(DeclineReason::Rated)
        );
    }

    #[test]
    fn a_blocked_challenger_is_declined_before_any_other_rule() {
        // Blocking wins even over a challenge that would otherwise be accepted, and
        // the match is case-insensitive against the account id.
        let policy = ChallengePolicy {
            block_list: vec!["Alice".to_string()],
            ..ChallengePolicy::default()
        };
        assert_eq!(
            classify(&acceptable(), &policy),
            Decision::Decline(DeclineReason::Generic)
        );
    }

    #[test]
    fn an_allow_list_admits_only_listed_challengers() {
        let policy = ChallengePolicy {
            allow_list: vec!["bob".to_string()],
            ..ChallengePolicy::default()
        };
        // `alice` is not listed, so even an otherwise-acceptable challenge is
        // declined; a listed challenger is admitted.
        assert_eq!(
            classify(&acceptable(), &policy),
            Decision::Decline(DeclineReason::Generic)
        );
        let mut bob = acceptable();
        bob.challenger.id = "bob".to_string();
        assert_eq!(classify(&bob, &policy), Decision::Accept);
    }

    #[test]
    fn bot_challenger_is_declined_by_default() {
        let mut c = acceptable();
        c.challenger.title = Some("BOT".to_string());
        assert_eq!(
            evaluate_default(&c),
            Decision::Decline(DeclineReason::NoBot)
        );

        let policy = ChallengePolicy {
            accept_bots: true,
            ..ChallengePolicy::default()
        };
        assert_eq!(classify(&c, &policy), Decision::Accept);
    }

    #[test]
    fn human_challenger_is_declined_when_only_bots_allowed() {
        let policy = ChallengePolicy {
            accept_humans: false,
            accept_bots: true,
            ..ChallengePolicy::default()
        };
        assert_eq!(
            classify(&acceptable(), &policy),
            Decision::Decline(DeclineReason::OnlyBot)
        );
    }

    #[test]
    fn rating_outside_bounds_is_declined() {
        let mut c = acceptable();
        c.challenger.rating = Some(3500);
        let policy = ChallengePolicy {
            max_rating: 2000,
            ..ChallengePolicy::default()
        };
        assert_eq!(
            classify(&c, &policy),
            Decision::Decline(DeclineReason::Generic)
        );
    }
}
