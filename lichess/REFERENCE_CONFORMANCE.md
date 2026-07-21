# Lichess bot: reference-conformance divergences

Seaborg's Lichess bot is modelled on the reference [lichess-bot]. Where the two
deliberately differ, the difference is recorded here with a one-line rationale,
so a later reader does not re-file a considered choice as a bug. Each entry is a
divergence we have decided to keep, not a gap to close.

[lichess-bot]: https://github.com/lichess-bot-devs/lichess-bot

## Deliberate divergences

- **Idle/interval units are seconds, not minutes.** `matchmaking.idle_timeout_seconds`
  and `min_challenge_interval_seconds` are seconds; the reference expresses the
  equivalent knobs in minutes. Seconds give finer control and match the rest of
  the config, whose durations are all in seconds.

- **Opponent selection is uniform-random over the eligible pool, not
  rating-weighted.** The reference weights selection toward nearer-rated bots.
  Seaborg draws uniformly among candidates that already pass the rating bounds,
  block list, and decline backoff. Uniform selection is enough to stop the bot
  fixating on one opponent (the property that matters) without a weighting scheme
  to tune; the PRNG is seeded from the wall clock so runs differ, and is seedable
  for tests.

- **Allow/block lists and per-account limits match by account id,
  case-insensitively.** The reference matches on display username. Seaborg matches
  on the Lichess account id (the lowercase username), comparing case-insensitively
  so a display-cased config entry still matches. A blocked or non-allow-listed
  challenger is declined with reason `generic` (Lichess has no dedicated
  "you're blocked" reason).

## Adopted from the reference

These reference behaviours are implemented, noted here so the correspondence is
explicit:

- Cancelling an outstanding matchmaking challenge that goes unanswered, by its
  tracked id (`challengeCancel`).
- Finer time-control decline reasons `tooFast`/`tooSlow`, and `standard` for a
  variant-only bot declining a standard challenge.
- A mode mismatch is declined with the mode the bot *does* accept (a rated
  challenge a casual-only bot refuses reports `casual`, and vice versa).
- Incoming allow list / block list, and a per-account simultaneous-game limit
  (`challenge.max_games_per_user`) declined with reason `later`.

## Reference knobs not implemented

Intentionally left out for now; low value or better handled elsewhere. Listed so
their absence is a known choice rather than an oversight:

- `recent_bot_challenge` throttle (anti-spam window per bot). The per-opponent
  decline backoff already spaces re-challenges.
- `bullet_requires_increment`. Not modelled; the clock bounds cover the common
  case.
- `rating_difference` gating on outgoing challenges beyond the absolute
  min/max rating bounds.
