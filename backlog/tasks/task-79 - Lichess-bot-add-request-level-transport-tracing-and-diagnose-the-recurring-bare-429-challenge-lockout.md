---
id: TASK-79
title: >-
  Lichess bot: add request-level transport tracing and diagnose the recurring
  bare-429 challenge lockout
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-23 00:08'
updated_date: '2026-07-23 01:16'
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
Rework for review attempt 1.

1. REV-1-01: restructure `check_status` so the 401 and 404 arms read their response body once and log it through `body_snippet` under the request's trace id at `debug`, matching the catch-all arm, before returning `Error::Unauthorized` / `Error::NotFound`. A token-scoped restriction can surface as a 401, and this task exists because a discarded body destroyed the only evidence of which limit fired.
2. Cover the 401 body log with a test in the style of the existing tracing tests, using the loopback `serve_repeating` helper and the `records_mentioning` / `expect_record` capture helpers.
3. Re-run the repository-required checks.
4. REV-1-02: the finding asks for a live capture of the lockout (the unexplained-429 warning plus its correlated header lines) to state whether the restriction is endpoint-scoped or token-scoped. That capture requires the operator's Lichess token and a reproduction of the lockout in live play; no token is available to this session and no static evidence settles endpoint-vs-token scope. Resolve REV-1-01, then hand the task to Needs Human with the exact capture procedure rather than guessing the scope or deferring the finding to a follow-up.
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

Rework — review attempt 1
=========================

Resolved REV-1-01 — a 401 or 404 body is no longer discarded
------------------------------------------------------------

`check_status` merged the `401` and `404` arms so both read the response body
once and trace it under the request id at `debug`, through a new shared
`log_body` helper the catch-all arm now uses too. Behaviour to callers is
unchanged: the arms still return `Error::Unauthorized` and `Error::NotFound`,
whose typed meaning is the point of having them. What changes is that the body
survives to the log, which matters because a token-scoped restriction can arrive
as a 401 and the body is the only place the server names it.

Covered by `a_401_traces_its_body_even_though_the_error_drops_it`, which serves
`401 Unauthorized` with a non-empty body from `serve_repeating` and asserts the
body text reaches a `Debug` record on the transport target. The 404 arm shares
the same code path.

Verified incidentally against the live API while capturing the evidence below:
`POST /api/bot/game/zzzzzzzz/abort` returned `404 {"error":"No such game"}`, and
the body was logged rather than dropped.

Resolved REV-1-02 — the 429s are endpoint-specific, not global per token
------------------------------------------------------------------------

Captured live. The lockout was still in force roughly 85 minutes after the bot
had last run, so it reproduced on the first attempt with no waiting.

Under one token, in one 65-second run at `RUST_LOG=info,lichess::transport=debug`:

    req#1 -> GET  /api/account                     200 in 1530ms
    req#2 -> GET  /api/stream/event                200 in  100ms   (stream open, held 70s)
    req#3 -> GET  /api/bot/online?nb=50            200 in  949ms
    req#4 -> POST /api/challenge/humaia-strong     429 in   15ms
    req#5 -> GET  /api/bot/online?nb=50            200 in  350ms
    req#6 -> POST /api/challenge/turochamp-1ply    429 in   48ms

Every GET succeeds while every challenge POST is refused, interleaved, on the
same token and the same connection pool. **The restriction is scoped to
challenge creation, not to the token.** A limiter that governed the token would
have refused `req#3` and `req#5` between the two refusals; it did not.

It is not per-opponent either. Three distinct opponents were refused
(`humaia-strong`, `turochamp-1ply`, and, in a direct probe, `maia1`, `maia9` and
`leelaalien`), and the two bot ids the run drew were ones the bot had never
challenged.

It is not writes-under-this-token generally. Three non-challenge-creation POSTs
passed the limiter and reached handler logic:

    POST /api/bot/game/zzzzzzzz/abort   -> 404 {"error":"No such game"}
    POST /api/bot/game/zzzzzzzz/resign  -> 404 {"error":"No such game"}
    POST /api/challenge/zzzzzzzz/cancel -> 404 {"error":"Not found"}

Note the last one: even a POST under `/api/challenge/` is allowed, so the
limiter sits on challenge *creation* specifically rather than on the URL prefix.

What Lichess actually says, now that the body is preserved:

    req#4 POST /api/challenge/humaia-strong was rate limited with no rate-limit
    envelope; body: {"error":"Too many requests. Try again later."}

The full header dump names nothing further. There is no `Retry-After`, no
`X-RateLimit-*` of any kind, and no `ratelimit` envelope — the response carries
`server`, `date`, `content-type`, `content-length: 47`, `connection`, `vary`,
`x-oauth-scopes: bot:play`, `x-accepted-oauth-scopes`, three CORS headers,
`strict-transport-security`, `x-frame-options`, and `permissions-policy`, and
that is the whole set. So the generic body is genuinely all Lichess offers, and
the code that reads `Retry-After` and the envelope was reading the only two
places that could ever have carried an answer, both of which are absent here.

Authorization is not the issue: `x-accepted-oauth-scopes` lists `bot:play`,
which the token holds, and an unscoped request would be a 401 rather than a 429.

This supersedes the "two facts remain unestablished" paragraph in the original
diagnosis above; both are now established. It also refines rather than
contradicts that diagnosis: the earlier conclusion that this is a persistent
restriction rather than a burst limiter the bot keeps re-arming is confirmed
independently, since the first probe came after roughly 85 minutes of zero
challenge traffic and was refused immediately.

Consequences for the follow-up fix, which remains out of scope here:

1. `Matchmaker` resets `challenge_backoff` only on a successful challenge, so a
   restriction outlasting the 600s cap leaves the bot probing every ten minutes
   indefinitely. That is the observed never-clears symptom and it is real.
2. Backing off all API traffic on a generic 429, as `lichess-bot` does, would
   not have helped and is the wrong fix: the other traffic was never limited.
   The hypothesis the task was created on is dead, and the fix should not
   inherit it.
3. What is still unknown is why challenge creation is restricted for this
   account and over what window it clears — a question about the account rather
   than about the bots code, and one the API declines to answer in any header.
   A follow-up should treat a bare 429 on challenge creation as an
   indefinite-duration condition rather than something a doubling backoff will
   outlast.
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

author: @claude
created: 2026-07-23 01:16
---
Implementation handoff
Branch: task-79-lichess-transport-tracing
Worktree: /Users/seabo/seaborg-worktrees/task-79-lichess-transport-tracing
Base: bada986c932ba144835cc941d29263f64f2a22f6
Implementation target: dd1a8320c0ec5c1b6a5cd4372d3e00f9e2f31bd2
Resolved findings: REV-1-01, REV-1-02
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, no warnings
- cargo test --workspace: pass, 661 passed / 0 failed / 2 ignored (one new test)
- live capture against lichess.org with the operator-supplied token: the lockout was still in force and reproduced on the first request; see the diagnosis in the notes
Known failures: none

REV-1-01 resolution: the 401 and 404 arms of check_status are merged and now read
their body once and trace it under the request id at debug, via a new shared
log_body helper that the catch-all arm also uses. The typed Error::Unauthorized
and Error::NotFound returns are unchanged. New test
a_401_traces_its_body_even_though_the_error_drops_it serves a 401 with a
non-empty body from serve_repeating and asserts the body text reaches a Debug
record on the transport target; the 404 arm shares the path. The 404 body log was
also seen live (POST /api/bot/game/zzzzzzzz/abort -> 404 {"error":"No such game"}).

REV-1-02 resolution: no code change, and the finding said none was implied. The
operator supplied a token, so the capture the finding asked for was made rather
than deferred. Answer to AC #8: the 429s are endpoint-specific, not global per
token. In one 65-second run, GET /api/account, GET /api/stream/event and two
GET /api/bot/online?nb=50 all returned 200 while two POST /api/challenge/{user}
requests interleaved between them returned 429 — same token, same run. Three
further opponents were refused by direct probe, so it is not per-opponent, and
three non-challenge-creation POSTs (including POST /api/challenge/{id}/cancel)
passed the limiter to reach 404 handler logic, so it is not writes-in-general
either. The preserved body reads {"error":"Too many requests. Try again later."}
and the full header dump names nothing beyond it: no Retry-After, no
X-RateLimit-* of any kind. Reproduction needed no waiting because the restriction
was still in force ~85 minutes after the bot last ran, which independently
confirms the earlier conclusion that this is a persistent restriction and not a
burst limiter the bot re-arms.

The notes append a "Rework — review attempt 1" section that explicitly supersedes
the "two facts remain unestablished" paragraph of the original diagnosis. The
hypothesis the task was created on — a global per-token limiter re-armed by the
bot other traffic — is refuted, and the reviewer should treat any follow-up that
inherits it as wrong.
---
<!-- COMMENTS:END -->
