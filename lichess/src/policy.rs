//! Challenge-acceptance decisions.
//!
//! [`evaluate`] compares an incoming challenge against the configured policy and
//! the number of games already in progress, producing an accept-or-decline
//! decision with a Lichess decline reason when it declines.

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
    /// The bot does not play this variant.
    Variant,
    /// The bot does not play this time control category.
    TimeControl,
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
            DeclineReason::Variant => "variant",
            DeclineReason::TimeControl => "timeControl",
            DeclineReason::Rated => "rated",
            DeclineReason::Casual => "casual",
            DeclineReason::NoBot => "noBot",
            DeclineReason::OnlyBot => "onlyBot",
        }
    }
}

/// Decide whether to accept `challenge` given the `policy` and the current game
/// load.
///
/// `active_games` is the number of games already in progress and `max_games` the
/// configured cap; a challenge that would exceed the cap is declined so the bot
/// never takes on more games than it can play. The checks run from the most
/// specific decline reason to the least so the challenger gets the most useful
/// explanation.
pub fn evaluate(
    challenge: &Challenge,
    policy: &ChallengePolicy,
    active_games: u32,
    max_games: u32,
) -> Decision {
    if active_games >= max_games {
        return Decision::Decline(DeclineReason::Generic);
    }

    if !policy.allows_variant(&challenge.variant.key) {
        return Decision::Decline(DeclineReason::Variant);
    }

    match &challenge.time_control {
        TimeControl::Clock { limit, increment } => {
            if *limit < policy.min_initial_seconds
                || *limit > policy.max_initial_seconds
                || *increment < policy.min_increment_seconds
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

    if challenge.rated && !policy.accept_rated {
        return Decision::Decline(DeclineReason::Rated);
    }
    if !challenge.rated && !policy.accept_casual {
        return Decision::Decline(DeclineReason::Casual);
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
        evaluate(challenge, &ChallengePolicy::default(), 0, 1)
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
    fn out_of_range_time_control_is_declined() {
        let mut c = acceptable();
        c.time_control = TimeControl::Clock {
            limit: 5,
            increment: 0,
        };
        assert_eq!(
            evaluate_default(&c),
            Decision::Decline(DeclineReason::TimeControl)
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
        assert_eq!(evaluate(&c, &policy, 0, 1), Decision::Accept);
    }

    #[test]
    fn rated_is_declined_when_policy_forbids_it() {
        let mut c = acceptable();
        c.rated = true;
        let policy = ChallengePolicy {
            accept_rated: false,
            ..ChallengePolicy::default()
        };
        assert_eq!(
            evaluate(&c, &policy, 0, 1),
            Decision::Decline(DeclineReason::Rated)
        );
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
        assert_eq!(evaluate(&c, &policy, 0, 1), Decision::Accept);
    }

    #[test]
    fn human_challenger_is_declined_when_only_bots_allowed() {
        let policy = ChallengePolicy {
            accept_humans: false,
            accept_bots: true,
            ..ChallengePolicy::default()
        };
        assert_eq!(
            evaluate(&acceptable(), &policy, 0, 1),
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
            evaluate(&c, &policy, 0, 1),
            Decision::Decline(DeclineReason::Generic)
        );
    }

    #[test]
    fn game_cap_declines_before_any_other_check() {
        // At the cap, even an otherwise-acceptable challenge is declined.
        assert_eq!(
            evaluate(&acceptable(), &ChallengePolicy::default(), 1, 1),
            Decision::Decline(DeclineReason::Generic)
        );
    }
}
