---
id: TASK-78
title: >-
  Lichess bot: stop blocking-retrying challenge-create 429s and honor Lichess
  rate-limit bodies
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-22 21:41'
updated_date: '2026-07-22 21:54'
labels: []
dependencies: []
priority: high
type: bug
ordinal: 133000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Matchmaking effectively stalls in production: the bot issues roughly one challenge every 15 minutes and plays almost no games, where the Python lichess-bot reference plays back-to-back games up to the Lichess daily ceiling.

Observed production log (2026-07-22): "challenging bot cheng-4" at 15:37:08 followed by "rate limited by Lichess (HTTP 429)" at 15:52:24, then the same pattern for likeawizard-bot and variantsbot. The 15m16s gap is self-inflicted, not a Lichess-mandated wait.

Root causes found by comparing against the reference implementation (lichess-bot lib/lichess.py and lib/matchmaking.py) and lila source:

1. Every request, including POST /api/challenge/{user}, goes through `with_rate_limit_retry` in lichess/src/transport.rs. With RATE_LIMIT_BASE=60s, doubling, and RATE_LIMIT_MAX_ATTEMPTS=5, a persistent 429 sleeps 60+120+240+480 = 900s in the matchmaking thread before surfacing the error. The reference never retries a 4xx at all: its backoff decorator uses `giveup=is_final`, which gives up for any status below 500.

2. `check_status` in transport.rs maps 429 using only the Retry-After header and discards the response body. Lichess does not send Retry-After for these limits; it puts the authoritative wait in the body as {"error": ..., "ratelimit": {"key": "bot.vsBot.day", "seconds": N}}. The bot therefore cannot tell which limit fired, nor how long to wait (often hours), and substitutes a 60s guess.

3. There is no per-endpoint cooldown. The reference records a non-blocking deadline per endpoint (rate_limit_timers[path_template]) and simply skips seeking until it expires; Seaborg instead sleeps in-thread and, because record_attempt was stamped 15 minutes earlier, immediately re-challenges into the same wall one second later.

4. run.rs calls `record_challenge_failed(&target)` on a 429, putting that opponent into the 1-hour decline backoff. A 429 with key bot.vsBot.day is an account-wide limit on this bot, not a signal about the opponent, so this steadily destroys the eligible-opponent pool. Lichess signals an opponent-side daily limit with HTTP 400 carrying the same ratelimit body, which the bot currently treats as a generic recoverable error.

Relevant Lichess limits (verified against lila source):
- modules/bot/src/main/BotLimit.scala: Max(100) bot-vs-bot games started per rolling day, charged on StartGame, key "bot.vsBot.day". Challenger over the cap gets 429; opponent over the cap gets 400. Both carry the ratelimit body with a `seconds` field.
- modules/web/src/main/Limiters.scala: challenge.create.user is composite, 25/minute and 200/day, cost 1 per bot target; challenge.create.ip is 500 per 10 minutes.

Note also that anchors/seaborg-lichess.toml sets matchmaking mode = "rated", and rated challenges are frequently rejected by other bots at creation; each rejection spends a challenge.create.user credit and currently triggers the 1-hour per-opponent backoff.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A 429 or 400 response body from Lichess carrying a `ratelimit` object is parsed, and its `key` and `seconds` values are available to the caller rather than discarded
- [ ] #2 POST /api/challenge/{user} is never retried in-transport on a 429; the error surfaces to the matchmaking caller on the first response
- [ ] #3 A challenge-create rate limit records a non-blocking cooldown deadline that suppresses further seek attempts until it expires, instead of sleeping inside the matchmaking thread
- [ ] #4 The cooldown duration comes from the Lichess-supplied `ratelimit.seconds` when present, falling back to Retry-After and then to an exponential backoff floor when it is not
- [ ] #5 A 429 carrying key `bot.vsBot.day` is treated as an account-wide limit and does not place the challenged opponent into the per-opponent decline backoff
- [ ] #6 A 400 carrying key `bot.vsBot.day` is recognised as an opponent-side daily limit and places only that opponent into a backoff for the supplied duration
- [ ] #7 A rate-limit log line identifies which limit fired and how long the bot will wait, so a production log is diagnosable without reproducing the run
- [ ] #8 A successful challenge creation resets any accumulated challenge-endpoint backoff to its floor
- [ ] #9 Unit tests cover: no in-transport retry on a challenge-create 429, ratelimit-body parsing for both the 429 and 400 shapes, cooldown suppression of seeking, and that a bot.vsBot.day 429 leaves the opponent eligible
- [ ] #10 Rate-limit handling for other endpoints (streams, moves, accept/decline) is unchanged unless a divergence from the reference is documented
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. error.rs: give the rate-limit errors the information Lichess actually sends. Extend `Error::RateLimited` with the limit key alongside the resolved wait, and add `Error::OpponentRateLimited` for the HTTP 400 shape that reports the *challenged* account is at its daily cap. Both stay recoverable.

2. transport.rs: read the response body on 429 and on 400, and parse the `{"error":..., "ratelimit":{"key":..,"seconds":..}}` envelope Lichess returns for the bot-vs-bot daily limit. Prefer the body`s `seconds` over the `Retry-After` header (Lichess does not send the header for these limits). A 400 that carries a ratelimit key becomes `OpponentRateLimited`; a 400 without one keeps its current `Error::Http` mapping.

3. transport.rs: add a `post_form_once` operation to the `Transport` trait that performs the POST without the in-transport 429 retry, so the caller owns the wait. Implement it for `HttpTransport` and for each test double. Leave `with_rate_limit_retry` and its constants in place for every other endpoint.

4. client.rs: route `create_challenge` through `post_form_once`.

5. matchmaking.rs: move the challenge-endpoint wait into the matchmaker as a non-blocking deadline. Add a cooldown instant checked by `choose`, and an escalating fallback (60s floor doubling to a 600s cap) used only when Lichess does not supply a duration. `record_rate_limited` sets the deadline and returns the wait applied so the caller can log it; a successful create resets the fallback to its floor.

6. matchmaking.rs: change the per-opponent backoff map to store an expiry instant rather than a start instant, so an opponent-side limit can be honored for the exact duration Lichess supplies while a plain decline keeps the configured window. Add `record_opponent_rate_limited`.

7. run.rs: dispatch on the challenge-create error. A 429 is account-wide, so it sets the cooldown and must not put the opponent into backoff; a 400 opponent limit backs off only that opponent; every other recoverable error keeps the existing `record_challenge_failed` behavior. Log which limit fired and how long the bot will wait.

8. Tests: transport parses both body shapes and does not retry a challenge-create 429; the matchmaker stays idle until the cooldown expires, escalates only without a server duration, resets on success, and leaves the opponent eligible after an account-wide 429; run.rs drives a 429 challenge-create end to end and asserts no per-opponent backoff was applied.

9. Run `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace`.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the fix across four layers.

- `error.rs`: `RateLimited` now carries the limit key alongside the resolved wait, and a new `OpponentRateLimited` models the HTTP 400 Lichess uses to report that the *challenged* account is over a limit. Both are recoverable.
- `transport.rs`: the response body is read on 429 and on 400 and parsed for the `{"ratelimit":{"key":..,"seconds":..}}` envelope. The body`s duration outranks `Retry-After`, which Lichess does not send for these limits. A new `post_form_once` performs a POST without the in-transport 429 retry; `post_form` and every other endpoint keep the existing retry, which suits limits that clear in seconds.
- `client.rs`: `create_challenge` uses `post_form_once`.
- `matchmaking.rs`: the wait became a non-blocking deadline (`challenge_cooldown_until`) checked by `choose`, with an escalating fallback (60s doubling to a 600s cap) used only when Lichess states no duration, reset to its floor by a successful create. The per-opponent map now stores an expiry instant rather than a start instant, so an opponent-side limit can be honored for exactly the window Lichess gave while a plain decline keeps the configured one.
- `run.rs`: the challenge-create error is dispatched by kind. A 429 sets the cooldown and leaves the named bot eligible; a 400 opponent limit backs off only that bot; other recoverable errors keep the existing `record_challenge_failed` behavior. Both paths log which limit fired and the wait applied.

Measured against production: the reported 15m16s gap between a challenge and its logged 429 is exactly the 60+120+240+480 = 900s the transport slept, plus request time. That sleep is gone.

Behavioral note for review, outside the acceptance criteria: `anchors/seaborg-lichess.toml` sets matchmaking `mode = "rated"`, and rated challenges are frequently rejected by other bots at creation. Each rejection spends one of the 200/day `challenge.create.user` credits and triggers the hour-long per-opponent backoff, so the eligible pool drains even with this fix in place. Changing the deployed config was not in scope.

A second residual: cancelling a lapsed outgoing challenge still goes through the retrying `post_empty`. That path is a different Lichess route with its own limits, not the per-day challenge caps, so it was left on the existing policy.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-22 21:54
---
Implementation handoff
Branch: task-78-lichess-challenge-rate-limit
Worktree: /Users/seabo/seaborg-worktrees/task-78-lichess-challenge-rate-limit
Base: d52a6fbc50a6061d0c5476daf10fa328306c8165
Implementation target: 964498b09b2157e889b9402126226fadb8268bc8
Resolved findings: none
Verification:
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean, no warnings
- cargo test --workspace: 644 passed, 0 failed, 2 ignored
Known failures: none. One note: an early run of `cargo test -p lichess` on this branch saw `run::tests::incoming_challenge_is_handled_while_a_matchmaking_call_is_blocked` time out on its 5s channel deadline while a workspace compile was still saturating the machine (that run took 5.01s versus 0.62s normally). It then passed 6 further runs, including 3 with the machine deliberately CPU-loaded. The test predates this task and its timing is unrelated to the change.
---
<!-- COMMENTS:END -->
