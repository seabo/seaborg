---
id: TASK-79
title: >-
  Lichess bot: add request-level transport tracing and diagnose the recurring
  bare-429 challenge lockout
status: Changes Requested
assignee:
  - '@claude'
created_date: '2026-07-23 00:08'
updated_date: '2026-07-23 00:59'
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

## Comments

<!-- COMMENTS:BEGIN -->
created: 2026-07-23 00:25
---
Implementation handoff
Branch: task-79-lichess-transport-tracing
Worktree: /Users/seabo/seaborg-worktrees/task-79-lichess-transport-tracing
Base: bada986c932ba144835cc941d29263f64f2a22f6
Implementation target: c9f1ec33b3bf42adeb335d79b9855c3252fe459a
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 660 passed / 0 failed / 2 ignored
- live smoke against lichess.org with an invalid token: RUST_LOG=lichess::transport=debug emitted the correlated pair (req#1 -> GET /api/account, req#1 <- GET /api/account 401 in 80ms); default level emitted nothing; RUST_LOG=info,ureq=debug still produced ureq output, confirming an explicit directive overrides the dependency default
Known failures: none

Reviewer notes:

Scope grew by one file beyond the transport, and deliberately. The tracing was written first and did not work from the operator side: simple_logger env() parses only a bare level word and silently drops the target=level form, so the acceptance criterion asking for RUST_LOG targeting the transport module could not be met without src/logging.rs. Raising the level globally instead surfaced the trace but buried it under roughly a dozen Debug lines per request from ureq/ureq_proto/rustls, so those are held at Info unless RUST_LOG names them. This was caught by running the binary rather than trusting the unit tests, which passed throughout.

Acceptance criterion 8 asks for a written diagnosis. It is recorded in the implementation notes and reaches a conclusion that contradicts the hypothesis the task was created on: the timeline shows one challenge request in an eight-minute window with no other account traffic, which rules out the volume-based global limiter theory. It does not identify the actual limit, because the response body that would name it has never been read; the change makes it readable. Whether that satisfies the criterion, or whether the criterion should hold until a live capture exists, is a reviewer call I should not make for myself.
---

author: @claude
created: 2026-07-23 00:59
---
Review attempt: 1
Reviewed branch: task-79-lichess-transport-tracing
Reviewed implementation: c9f1ec33b3bf42adeb335d79b9855c3252fe459a
Verdict: changes_requested

Scope and target are sound. c9f1ec3 descends from the recorded base bada986, is
an ancestor of the branch tip, and the single later commit (d8d0899) touches only
the task file. The worktree is clean. The tracing itself is well built: the
correlated request id, the `Drop`-based stream close, the failed-request line for
requests that never get a status, and the `logging.rs` directive parser all hold
up. The `simple_logger` claim the design rests on was checked against the crate
source rather than taken on trust: `with_module_level` sorts by descending target
length with a stable sort, so registering operator directives ahead of the
dependency pins does make an explicit `RUST_LOG=ureq=trace` win the tie, as the
module comment states. AC #1, #2, #4, #5, #6 and #7 are proven.

Two criteria are not met.

REV-1-01 [P2] A 401 or 404 response body is still discarded
Location: lichess/src/transport.rs:442-446
Impact: AC #3 requires a non-2xx response body to be preserved and logged
verbatim rather than discarded. The `429` arm and the catch-all `other` arm now
do that, but the `401` and `404` arms return their typed error without ever
reading the body, so it is dropped exactly as the 429 body used to be. This
matters beyond the letter of the criterion: a token-scoped restriction can
surface as a 401, and this task exists because a discarded body destroyed the
only evidence of which limit fired. The two arms that still discard theirs leave
the same blind spot in place for two statuses.
Reproduction: Serve `401 Unauthorized` (or `404 Not Found`) with a non-empty body
from the existing `serve_repeating` helper and drive any transport entry point at
`RUST_LOG=lichess::transport=debug`; the body text appears in no record. The
`records_mentioning` / `expect_record` helpers added in this change support the
assertion directly.
Expected: Both arms log the body through `body_snippet` under the request's trace
id, at the same `debug` level the `other` arm uses, before returning
`Error::Unauthorized` / `Error::NotFound`. Cover at least one of them with a test
in the style of the existing tracing tests.

REV-1-02 [P2] The diagnosis does not answer the question AC #8 asks
Location: backlog task TASK-79, Implementation Notes, "Diagnosis"
Impact: AC #8 requires a written diagnosis stating whether the observed 429s are
endpoint-specific or global per token, with supporting evidence. The recorded
diagnosis is good work and refutes the hypothesis the task was created on — one
challenge POST in an eight-minute window with no other account traffic rules out
a volume-based limiter the bot keeps re-arming — but it stops short of the stated
question, and says so: its unestablished fact 2 is "whether the restriction is
scoped to the challenge endpoint or to the token". The criterion is therefore
unproven by the task's own account. The implementer correctly declined to rule on
this and referred it to review; it was put to the human operator, who chose to
hold the criterion open rather than accept the bounded diagnosis or reword it.
Reproduction: Read the "Diagnosis" section against AC #8; the conclusion offered
is about volume versus persistence, not about endpoint versus token scope.
Expected: Run the bot with `RUST_LOG=info,lichess::transport=debug` until the
lockout reproduces, capture the "was rate limited with no rate-limit envelope"
warning together with the `header` lines sharing its `req#` id, and record in the
notes what those show about scope, with the captured lines as the evidence. No
code change is implied by this finding beyond REV-1-01.

Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no
  warnings; re-run with a clean CARGO_TARGET_DIR to confirm the result was not a
  cache artifact, still clean
- cargo test --workspace: pass, 660 passed / 0 failed / 2 ignored, including all
  seven new tracing tests
- git merge-base --is-ancestor c9f1ec3 HEAD: pass
- git diff --stat c9f1ec3 HEAD: task file only, 28 insertions / 2 deletions
- diff audit: no new #[allow], no comment citing a task ID, criterion, or finding
  ID; no unrelated changes. src/logging.rs is out of the transport but justified,
  since AC #6 is unreachable without it.
---
<!-- COMMENTS:END -->
