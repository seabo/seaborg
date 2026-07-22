---
id: TASK-78
title: >-
  Lichess bot: stop blocking-retrying challenge-create 429s and honor Lichess
  rate-limit bodies
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-22 21:41'
updated_date: '2026-07-22 22:56'
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
Rework for REV-1-01. The original change is otherwise accepted; this attempt
only bounds the in-transport wait it inadvertently unbounded.

Approach: the retry loop is willing to spend a bounded amount of time, and a
server-stated wait longer than the ceiling it already applies to its own backoff
cannot clear within the attempt budget. Sleeping such a wait blocks the calling
thread for a retry that is certain to be refused again. So the loop gives up on
it immediately and hands the caller the full stated duration to act on, which is
what the non-retrying challenge path already does.

1. transport.rs: give the free `with_rate_limit_retry` a `max_wait` bound, passed
   `RATE_LIMIT_MAX` by the `HttpTransport` method. When the server states a wait
   longer than `max_wait`, return the rate-limit error instead of sleeping. A
   stated wait within the bound is still slept, exactly as before, so short
   limits keep clearing transparently.
2. transport.rs: the error must carry the true stated duration, not a clamped
   one, so `check_status` is left alone and the matchmaker keeps setting its
   cooldown from Lichess's real remaining time.
3. transport.rs: correct the `with_rate_limit_retry` doc comment, which still
   describes the wait as coming from `Retry-After` only, and state the bound.
4. Tests: a retrying entry point against a 429 carrying the `bot.vsBot.day`
   envelope makes exactly one request and surfaces the full 7200s; a stated wait
   within the bound is still retried and slept for the stated time; the existing
   no-header backoff, attempt-budget, and shutdown tests still hold.
5. Confirm the non-retrying challenge path, the matchmaker cooldown, and every
   other behavior the previous attempt established are untouched, then run the
   required checks.
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

Rework attempt 2 (REV-1-01).

Resolved REV-1-01. The body parsing added in attempt 1 sits in `check_status`,
shared by every entry point, so it also changed the retry policy of the
endpoints that still retry in-transport: `with_rate_limit_retry` slept the
server-stated wait with no bound, since only the local `Backoff` was capped by
`RATE_LIMIT_MAX`. A 429 carrying the `bot.vsBot.day` envelope would have slept
4 x 7200s inside one call, against 900s from the local backoff before the
change. The accept endpoint can return that response — the cap is charged on
game start, and accepting a bot's challenge starts a game — and accepts run on
the event-consumer loop.

Behavior changed: `with_rate_limit_retry` takes a `max_wait` bound, given
`RATE_LIMIT_MAX`. A server-stated wait longer than that ends the loop instead
of being slept, and the error carries the full stated duration so the caller
can act on the real remaining time. A stated wait within the bound is slept
exactly as before, so short limits still clear transparently, and the
worst-case in-transport block for streams, moves, and accept/decline is back to
what it was at the base commit. `check_status` is untouched, so the matchmaker
still sets its cooldown from Lichess's true duration.

Also corrected the `with_rate_limit_retry` doc comment, which still described
the wait as coming from `Retry-After` alone, and the module doc, which
described the non-retrying entry point as the only protection against a
day-long wait.

Verification: three new tests, and the guard was removed locally to confirm
what each one actually catches.

- `a_wait_longer_than_the_bound_is_surfaced_rather_than_slept` asserts one
  attempt, no sleep, and the full 7200s reaching the caller. Without the guard
  it fails on the attempt count, 5 against the expected 1 — in production those
  five attempts are the four 7200s sleeps. This is the regression test.
- `a_retrying_post_also_refuses_to_sleep_out_a_day_long_limit` drives
  `post_empty` against a loopback server returning the real `bot.vsBot.day`
  body and asserts a single request, covering the same guard through a real
  retrying entry point rather than the free function. It was not run against
  the unguarded code: doing so would sleep out the four real 7200s waits.
- `a_wait_at_the_bound_is_still_slept` passes with or without the guard, as
  expected. It is a boundary characterization test, not a regression test: it
  pins the stated wait equal to the bound on the retrying side, so a later
  tightening of the bound cannot silently stop short limits from being waited
  out.
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

author: @claude
created: 2026-07-22 22:46
---
Review attempt: 1
Reviewed branch: task-78-lichess-challenge-rate-limit
Reviewed implementation: 964498b09b2157e889b9402126226fadb8268bc8
Verdict: changes_requested

REV-1-01 [P1] Body-supplied rate-limit wait now drives the in-transport retry sleep on every other endpoint, unbounded
Location: lichess/src/transport.rs:276-292 (429 arm of `check_status`) feeding lichess/src/transport.rs:236-249 (`with_rate_limit_retry`)
Impact: `check_status` is shared by every entry point, so the new body parsing changes the retry policy of `get`, `post_empty`, `post_form`, and `open_stream`, not just the non-retrying challenge path. `with_rate_limit_retry` sleeps `retry_after.unwrap_or_else(|| backoff.next_delay())`, and `retry_after` is never clamped: only the local `Backoff` is bounded by `RATE_LIMIT_MAX`. Before this change a 429 that carried the rate-limit envelope but no `Retry-After` header — the exact shape the task establishes Lichess sends for `bot.vsBot.day` — slept the local backoff, 60+120+240+480 = 900s worst case. After it, the same response sleeps the server-stated duration on every attempt: with the `seconds: 7200` body used in the new tests, `RATE_LIMIT_MAX_ATTEMPTS = 5` gives 4 x 7200s = 8 hours inside one call.

The task's own research makes that response reachable on a retrying endpoint: `bot.vsBot.day` is charged on StartGame, and accepting a bot's challenge starts a game, so `POST /api/challenge/{id}/accept` (client.rs:54, `post_empty`, retrying) can return it. That call runs on the event-consumer loop (run.rs:651 `process_accept_queue`, driven by the `while !shutdown.is_requested()` loop at run.rs:608), so the loop would stop handling events, slot bookkeeping, and drain progress for hours. `Shutdown::sleep` still polls out at 200ms granularity, so shutdown is unaffected, but nothing else is. `make_move` (client.rs:164) and `cancel_challenge` (client.rs:76) are on the same policy.

This is the failure mode the task exists to remove — a self-inflicted multi-minute block in a thread with other work — reintroduced on the endpoints the fix did not touch, and made an order of magnitude worse. It also leaves AC #10 unproven: rate-limit handling for streams, moves, and accept/decline is materially changed (the wait source moved from the header/local backoff to the response body, uncapped), and neither the implementation notes nor a code comment documents that divergence. The implementation note about `cancel_challenge` reasons only about which Lichess limits apply to that route, not about the new unbounded body-sourced sleep.

Related: the doc comment on `with_rate_limit_retry` (transport.rs:227-232) still states "the wait is the server's `Retry-After` when present, else the next backoff step", which no longer describes the code — the wait can now come from the body and outrank the header.

Reproduction: no test covers a retrying endpoint against a 429 carrying the envelope. Reasoning from the code path is exact: `check_status` (transport.rs:276) returns `retry_after: Some(Duration::from_secs(7200))` for `BOT_VS_BOT_BODY` regardless of caller — the new test `a_non_retrying_post_surfaces_a_rate_limit_after_one_request` already asserts that mapping — and `with_rate_limit_retry` (transport.rs:247) passes that value straight to `sleep` on each of the first four attempts. A test mirroring `serve_repeating("429 Too Many Requests", "", BOT_VS_BOT_BODY)` against `post_empty` with an injected `sleep` recorder would observe the four 7200s waits.

Expected: the in-transport retry must stay bounded. Clamp the honored wait in `with_rate_limit_retry` to `RATE_LIMIT_MAX` (or use the body duration only on the non-retrying path and leave the retrying loop on header-or-backoff), so the worst-case blocking time for streams, moves, and accept/decline is unchanged from the base commit. Cover it with a test on a retrying entry point, and correct the stale `with_rate_limit_retry` doc comment. If keeping an unbounded server-supplied wait on those endpoints is deliberate, AC #10 requires the divergence to be documented with the reasoning stated in the code, not only in the task notes.

Verification (run on 964498b, worktree /Users/seabo/seaborg-worktrees/task-78-lichess-challenge-rate-limit):
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean, no warnings
- cargo clippy --workspace --all-targets --all-features -- -D warnings with a clean CARGO_TARGET_DIR: clean, no warnings (confirms the fast run was not a cache artifact)
- cargo test --workspace: 644 passed, 0 failed, 2 ignored
- git diff --stat 964498b..c633e08: task file only, so the target is immutable
- No #[allow] added by the diff; no comment cites a task ID, acceptance criterion, or finding ID.
- No benchmarks run: the diff touches only the lichess crate, no movegen or search hot path.

Accepted as correct on this target, for the next attempt's benefit: AC #1-#9 are implemented and evidenced. The non-retrying `post_form_once` seam, the body-outranks-header precedence, the 400/429 split into `OpponentRateLimited`/`RateLimited`, the non-blocking `challenge_cooldown_until` deadline, the deadline-valued per-opponent map, the escalate-only-without-a-stated-duration fallback with its reset on success, and the run.rs dispatch that keeps a 429's named opponent eligible are all covered by targeted tests that fail for the right reason. REV-1-01 is the only blocking finding.
---

author: @claude
created: 2026-07-22 22:56
---
Implementation handoff
Branch: task-78-lichess-challenge-rate-limit
Worktree: /Users/seabo/seaborg-worktrees/task-78-lichess-challenge-rate-limit
Base: d52a6fbc50a6061d0c5476daf10fa328306c8165
Implementation target: 2d82aea
Resolved findings: REV-1-01
Verification:
- cargo fmt --check: clean
- cargo clippy --workspace --all-targets --all-features -- -D warnings: clean, no warnings
- cargo clippy --workspace --all-targets --all-features -- -D warnings with a clean CARGO_TARGET_DIR: clean, no warnings
- cargo test --workspace: 647 passed, 0 failed, 2 ignored (644 at the previous target, plus the three new transport tests)
- Guard removed locally to confirm the regression test catches the reported defect: `a_wait_longer_than_the_bound_is_surfaced_rather_than_slept` fails on the attempt count, 5 against 1. Restored before committing; the committed tree is the one all checks above were run on.
Known failures: none.

Note for review: the rework is confined to lichess/src/transport.rs. The diff from the previous target 964498b is the `max_wait` bound on `with_rate_limit_retry` and its call site, three tests, the extra argument threaded through five existing test call sites, and two doc-comment corrections. error.rs, client.rs, game.rs, matchmaking.rs, and run.rs are unchanged from 964498b, which was already reviewed and accepted on AC #1-#9.

Scope note carried forward from attempt 1, unchanged and outside the acceptance criteria: anchors/seaborg-lichess.toml sets matchmaking mode = "rated", and rated challenges are frequently rejected by other bots at creation, so the eligible pool still drains through the per-opponent decline backoff even with this fix. Changing the deployed config was not in scope.

Reviewer independence: this attempt was implemented by the same session that raised REV-1-01, at the user's explicit authorization. It therefore needs an independent reviewer; the author of the finding must not approve the fix.
---
<!-- COMMENTS:END -->
