---
id: TASK-79
title: >-
  Lichess bot: add request-level transport tracing and diagnose the recurring
  bare-429 challenge lockout
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-23 00:08'
updated_date: '2026-07-23 00:23'
labels: []
dependencies: []
priority: high
type: bug
ordinal: 134000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
In a live session the bot plays roughly one game and then locks itself out of matchmaking: every `POST /api/challenge/{user}` returns HTTP 429 and the local challenge backoff escalates 60 -> 120 -> 240 -> 480 -> 600s without ever clearing, because `Matchmaker` only resets `challenge_backoff` on a successful challenge. The reference `lichess-bot` Python client does not exhibit this.

Three facts are already readable from the production log without new instrumentation:

1. The warning emitted at `lichess/src/run.rs` omits the `[key]` bracket, so `Error::RateLimited.key` was `None`: the 429 body carried no `{"ratelimit":{...}}` envelope. This is therefore NOT `bot.vsBot.day` or the per-day challenge cap that TASK-78 addressed.
2. The reported wait matched the local doubling sequence exactly, so `retry_after` was also `None`: no `Retry-After` header either.
3. A bare 429 with neither an envelope nor a `Retry-After` is Lichess generic per-token API limiter rather than an endpoint-specific cap.

Diagnosis is currently blocked because the bot has no visibility into its own HTTP traffic. `lichess/src/transport.rs` and `lichess/src/client.rs` contain zero log statements, so no request the bot makes is observable. Worse, `check_status` reads the 429 response body, attempts `parse_rate_limit`, and discards the body entirely when it does not match the envelope shape - destroying the single most diagnostic artifact available.

The leading hypothesis to confirm or kill is that the limiter is global per token rather than per endpoint. `lichess-bot` backs off all API traffic when it sees a generic 429; Seaborg pauses only challenge creation, while the account event stream, per-game streams, move submissions, and a `GET /api/bot/online?nb=50` issued on every matchmaking seek continue unabated - continually re-arming the limiter so the challenge cooldown never elapses into a clear window.

The deliverable of this task is the instrumentation plus a written diagnosis; the behavioural fix may follow as a separate task once the trace identifies the true cause.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every HTTP request issued by HttpTransport is logged with method, request path, and a per-request identifier that correlates the request with its response
- [ ] #2 Every HTTP response is logged with status code and elapsed duration, correlated to its request by identifier
- [ ] #3 A non-2xx response body is preserved and logged verbatim (subject to an existing-style length cap) rather than discarded, including a 429 body that does not parse as a rate-limit envelope
- [ ] #4 A 429 response logs its full set of response headers so Lichess limiter hints beyond Retry-After become observable
- [ ] #5 Stream lifecycle events (open, close, and reconnect) are logged, so stream churn is visible as a source of request volume
- [ ] #6 Request-level tracing is off at the default Info level and is enabled by RUST_LOG targeting the transport module, so ordinary operation is not made noisy
- [ ] #7 Tracing verbosity choices are covered by tests that assert on emitted records without requiring network access
- [ ] #8 The task records a written diagnosis stating whether the observed 429s are endpoint-specific or global per token, with the evidence supporting that conclusion
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a per-request trace context to `HttpTransport`: a process-wide `AtomicU64` counter yields a request id; a small `RequestTrace` value carries id, HTTP method, and path so the request line and its response line correlate. Log the request at `debug!` (module target `lichess::transport`, so `RUST_LOG=lichess::transport=debug` enables it and the default `Info` stays quiet).

2. Log every response at `debug!` with the correlating id, status code, and elapsed wall time measured from just before the request is sent.

3. Thread the trace context into `read_response`/`check_status`, which are currently free functions with no request context. Restructure `check_status` so a non-2xx body is read exactly once into a value that is always available, instead of the current 429 path that reads the body, tries `parse_rate_limit`, and drops it when the shape does not match.

4. Emit the unexplained-429 body at `warn!` verbatim (reusing the existing `MAX_ERROR_BODY_CHARS` cap) when a 429 carries no `ratelimit` envelope. This is the diagnostic artifact the bug turns on and it is rare by construction, so it belongs above the `debug` tracing threshold alongside the existing rate-limit warning in `run.rs`.

5. Dump the complete response header set of any 429 at `debug!`, so Lichess limiter hints beyond `Retry-After` become observable.

6. Log stream lifecycle in `open_stream`: the open at `debug!`, and the close by wrapping the returned line iterator in a guard that logs on `Drop`, which covers normal exhaustion and early abandonment alike. Add the pending wait duration to the two existing reconnect warnings in `run.rs` and `game.rs` so reconnect churn is measurable rather than merely visible.

7. Test with a capture logger installed once per test process behind a `Once`, recording level, target, and message. Tests filter captured records by their own unique request path so parallel execution cannot cross-contaminate, and assert both that request/response lines are `Debug` (absent at `Info`) and that an unexplained 429 body reaches `Warn`. No network: the existing loopback `serve_repeating` helper backs these.

8. Record the diagnosis in the task. Static evidence bounds the conclusion; naming what a live trace must show to settle it is part of the deliverable.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation
--------------

`lichess/src/transport.rs` gains a `RequestTrace` (process-wide `AtomicU64` id, method, path, start `Instant`). Every entry point opens one and logs `req#N -> METHOD /path`; `check_status` closes it with `req#N <- METHOD /path STATUS in Nms`, or, when the request produced no status at all (connection, TLS, timeout), `failed after Nms: <error>`. A retried request takes a fresh id per attempt, deliberately: each attempt is its own round trip, and collapsing them would hide the repetition a rate-limit investigation is looking for.

`check_status` no longer discards a 429 body that fails to match the `ratelimit` envelope. The body is read once, always available, and an unmatched one is logged verbatim at `warn` (capped by the existing `MAX_ERROR_BODY_CHARS`). All response headers of a 429 are dumped at `debug`, guarded by `log_enabled!` so the header walk is skipped when the level is off. A `body_snippet` helper distinguishes an absent body from an empty one, which are different findings.

Streams return a `TracedStream` wrapper that logs the open and, from `Drop`, the close with elapsed time and lines carried. `Drop` rather than iterator exhaustion, because a stream is at least as often abandoned (shutdown, mid-body error) as drained, and a close that only fired for clean endings would miss exactly the failures worth seeing. Both reconnect loops (`run.rs`, `game.rs`) now report the pending wait, making reconnect rate measurable.

Logging configuration was a blocking defect found by smoke-testing the deliverable rather than assuming it worked. `simple_logger`s `env()` accepts only a bare level word and silently discards the `target=level` form, so the intended `RUST_LOG=lichess::transport=debug` did nothing; the only way to reach the trace was `RUST_LOG=debug`, which buried it under roughly a dozen `Debug` lines per request from `ureq`, `ureq_proto`, and `rustls`. New `src/logging.rs` parses `RUST_LOG` directives and holds those three dependencies at `Info` unless `RUST_LOG` names one explicitly (registration order resolves the tie, so an explicit directive wins).

Diagnosis
---------

The task was created on the hypothesis that the 429s came from Lichess generic per-token limiter, re-armed by the bot other traffic while only challenge creation paused. **The evidence refutes that hypothesis**, and it should not be carried into the follow-up fix.

The reported timeline issues three challenges, at 23:36:26, 23:40:26, and 23:48:26 — gaps of four and eight minutes. Every one is refused. Reading `Matchmaker::choose` against `seek_matchmaking_game`, the cooldown check returns `Action::Idle` before the `Action::Seek` branch, and `seek_matchmaking_game` returns before calling `online_bots`. So while the challenge cooldown runs, matchmaking issues **no HTTP at all** — not even the bot-list GET. The log also shows `0 slots` after the game ended, so no game streams were open either. Total account traffic across the 23:40 to 23:48 window was one long-lived event-stream connection plus one challenge POST.

One request in eight minutes cannot trip a volume-based limiter. Whatever refuses these challenges is a persistent per-account restriction on challenge creation that is already in force when the request arrives, not a burst limiter the bot keeps re-arming. That also explains the never-escapes symptom without appeal to request volume: `challenge_backoff` resets only on a successful challenge, so a restriction outlasting the 600s cap leaves the bot re-probing every ten minutes forever.

Two facts remain unestablished, and both are answered by the body and headers this change now preserves:

1. What Lichess actually says. The refusal carries no `ratelimit` envelope and no `Retry-After`, and the body was being discarded, so the stated reason has never been read. It is now logged verbatim at `warn`.
2. Whether the restriction is scoped to the challenge endpoint or to the token. The full 429 header dump settles this; only `Retry-After` and the body envelope were ever inspected.

To settle it, run with `RUST_LOG=info,lichess::transport=debug` until the lockout reproduces and capture the `was rate limited with no rate-limit envelope` warning together with the `header` lines sharing its `req#` id. The corrective change belongs in a follow-up task once that body is known; guessing a fix ahead of it would repeat the mistake this task exists to correct.

Verification of the tracing itself was done against the live API with an invalid token: `RUST_LOG=lichess::transport=debug` produced exactly the correlated pair (`req#1 -> GET /api/account`, `req#1 <- GET /api/account 401 in 80ms`) and the default level produced nothing.
<!-- SECTION:NOTES:END -->
