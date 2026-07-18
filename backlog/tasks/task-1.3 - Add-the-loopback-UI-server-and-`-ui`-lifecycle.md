---
id: TASK-1.3
title: Add the loopback UI server and `--ui` lifecycle
status: Done
assignee:
  - '@codex'
created_date: '2026-07-17 15:40'
updated_date: '2026-07-18 14:26'
labels: []
dependencies:
  - TASK-1.2
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
parent_task_id: TASK-1
type: task
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Host the game controller through a deliberately narrow local HTTP interface, serve embedded application assets, stream snapshots and search information, and integrate startup and shutdown with the Seaborg CLI.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 `seaborg --ui` binds to 127.0.0.1 on an available port, prints the URL, and opens it only after the listener is ready
- [x] #2 `--ui-port` selects a fixed port and `--no-open` suppresses browser launch, with clear errors for bind or launch failures
- [x] #3 Embedded application assets and current state are available over GET, commands use bounded POST endpoints, and updates stream through a reconnectable Server-Sent Events endpoint
- [x] #4 Mutating requests require the process session token and unexpected Host or Origin values are rejected
- [x] #5 Responses set appropriate content types, no-store state caching, and a restrictive Content Security Policy
- [x] #6 `--ui`, `--uci`, and `--dev` cannot be selected together
- [x] #7 Protocol tests cover startup, state retrieval, command validation, SSE reconnection, request limits, and shutdown
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Rework for review attempt 1 (target 7b7225a).

Blocking findings:
1. REV-1-01 - bound the accept loop. Add MAX_CONNECTIONS with an atomic counter and an RAII guard released when a connection thread exits; refuse over-cap connections with 503 too_many_connections. Spawn via thread::Builder so a failed spawn is an io::Result handled like a failed accept (refuse and keep serving) instead of a panic that unwinds UiServer::run. Refusals are written with a short write timeout so the accept thread cannot be stalled by the refused peer.
2. REV-1-02 - give the drain path an absolute deadline. Replace the per-read DRAIN_TIMEOUT socket timeout with a DRAIN_DEADLINE loop that recomputes the remaining time before every read, keeping the existing MAX_DRAIN byte cap, mirroring http::apply_deadline on the request path.

Cheap non-blocking observations, fixed in place rather than deferred:
3. server.rs check_origin_headers - correct the doc comment. A missing Origin on GET is not covered by the token requirement; the real reasons are that top-level navigations legitimately omit Origin and that the DoS surface is now bounded by the connection cap. Behaviour is unchanged: requiring Origin on GET would break normal browser navigation.
4. session.rs shutdown - take the published lock before notify_all so a stream that has read running but not yet parked cannot miss the wakeup and stay parked up to KEEPALIVE_INTERVAL.
5. wire.rs write_score - special-case INF_P/INF_N ahead of the mate branches, as Score's Display does, so an infinite score cannot render as a sign-inverted mate.
6. json.rs - reject non-hex \u escapes in hex4 and replace number() with a strict JSON number scanner, so 01, 1., -.5 and \u+041 are rejected as the module's strict contract states.
7. http.rs read_line - distinguish EOF from a terminating blank line by returning Option, so a request truncated after a complete header line is rejected as malformed rather than served as complete.

Verification: regression test per fix, cargo fmt --check, cargo clippy -p engine --all-targets, cargo test --workspace, repeated ui:: runs for flakes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added engine::ui: a hand-rolled loopback HTTP/1.1 server over the TASK-1.2 GameController, with no new dependencies. Chose std::net over an HTTP crate because the workspace has no async runtime or serialization crates, GameController is blocking and single-owner, and the required surface is a fixed route set; TASK-21 also targets a smaller dependency graph. Confirmed with the user before implementing, along with serving a placeholder client (TASK-1.4 owns the real board).

Module layout: http.rs (bounded HTTP/1.1 subset), json.rs (owned JSON reader and writer), wire.rs (browser adapter, the sibling of engine::info for UCI), session.rs (shared state and publish/subscribe), server.rs (routing, security, SSE, lifecycle), assets/ (embedded HTML, JS, CSS), tests.rs (end-to-end protocol tests over real sockets).

Design notes. One driver thread polls the controller every 10ms and publishes a serialized snapshot; streams wait only on the published snapshot, so a slow browser never blocks the engine. Events carry a monotonic event id distinct from the game revision, because search progress changes the snapshot without advancing the revision. The session token is substituted into the served page and required on mutations; Host and Origin are validated against this server's own loopback authority, which defeats DNS rebinding.

Self-review before handoff found and fixed seven issues, each with a regression test:
- A Last-Event-ID above this session's counter was trusted, so a tab left open across a server restart received no state at all (event ids restart at zero per process). Reproduced live; such an id is now treated as a fresh connection.
- A failed accept retried with no backoff, spinning the loop at full CPU under descriptor exhaustion.
- The read timeout applied per syscall rather than to the whole request, so a dribbling client could hold a thread for hours. Requests now have a 15s deadline; verified a silent client gets 408 after 15s.
- handle_command's catch-all arm would have made any POST route added later silently reset the game.
- The Host allowlist was case-sensitive though HTTP hosts are not.
- A rejected oversized request was answered and then closed with data still unread, so the kernel sent RST and the client lost the 413. Rejected requests are now drained within a bound.
- A panicking server thread exited 0; the CLI now reports and exits 1.

Verification: cargo fmt --check passed; cargo test --workspace passed 182 tests with zero failures; the 67 ui tests passed on 5 consecutive runs with no flakes; cargo clippy -p engine --all-targets produced zero warnings in the new code; git diff --check passed. Also exercised the real binary: played a 6-ply game against the engine, and confirmed the token, Host, Origin, content-type, method, path-traversal, size-limit, SSE streaming, and SSE reconnection behaviours over curl.

Review attempt 1 rework (base target 7b7225a).

Resolved REV-1-01. The accept loop called thread::spawn per connection with no cap, on the accept-loop thread; thread::spawn panics when the OS refuses a thread, so the panic unwound UiServer::run and the process exited 1 mid-game. Connections now acquire a ConnectionPermit from a MAX_CONNECTIONS (64) pool via compare-and-swap, released on any thread exit including a panic unwind, and over-cap peers receive 503 too_many_connections. Spawning moved to thread::Builder so a failed spawn is stepped over like a failed accept; the closure owns the stream and permit, so a rejected spawn drops both. Refusals are written on the accept thread under a short write timeout plus a 100ms poll-bounded drain, so a refused peer can neither stall the loop nor lose its 503 to an RST. Verified live: the reviewer's repro killed the process at 4106 bare connections; the rebuilt binary absorbed 5000, answered 503, logged no panic, and returned to normal service and gameplay once they closed.

Resolved REV-1-02. DRAIN_TIMEOUT was installed as a per-read socket timeout, so a client sending one byte per 2s kept the drain productive to the 1 MiB cap. drain_rejected_request now recomputes the time remaining before every read against an absolute DRAIN_DEADLINE, mirroring http::apply_deadline. Verified live: the reviewer observed a connection held for the full 60.2s after 40 of 20000 bytes; it now closes after 4.5s having accepted 3, with the 413 still delivered. The trade this makes is deliberate and documented: a client that keeps sending past the deadline forfeits its response to the reset, because the thread matters more than the courtesy. A client that stops sending still receives the 413.

Non-blocking observations, all fixed here rather than deferred, so no follow-ups were filed.
- check_origin_headers: comment only, behaviour unchanged. The stated rationale for accepting a missing Origin did not hold for GET. Requiring Origin on GET would break top-level navigation, so the comment now gives the reasons that do hold (no CORS headers, nosniff, exact content types, frame-ancestors none) and points at MAX_CONNECTIONS for the residual DoS surface the reviewer identified.
- Session::shutdown now stores running under published before notify_all, closing the lost-wakeup window against wait_for_update.
- write_score special-cases INF_P/INF_N ahead of the mate branches, as Score's Display does. app.js switches on the score tag rather than assuming anything untagged is centipawns.
- json hex4 requires four actual hex digits, and number was replaced with a JSON-grammar scanner.
- http read_line returns Option so EOF is distinguishable from the blank line ending the headers.

Regression tests: six new tests, each confirmed to FAIL against the previous implementation before being kept (drain deadline, connection cap, wire infinities, json escapes, json numbers, http truncation).

One deliberate omission for the reviewer to weigh. The shutdown lost-wakeup has no regression test. The window is a few instructions between wait_for_update reading running and wait_timeout parking, and the waiter holds the published lock across all of it, so it is not reachable from outside the type. A 400-round stress test was written and discarded because it passed with the bug present, which would have been false assurance. The fix is justified by locking discipline and the reasoning is recorded at the call site.

Verification: cargo fmt --check passed; cargo clippy -p engine --all-targets reported zero warnings in engine/src/ui and src/cmdline.rs (the remainder are pre-existing in core/engine); cargo test --workspace --no-fail-fast passed 188 tests, 0 failures, 1 pre-existing ignored; cargo test -p engine ui:: passed 73/73 on 5 consecutive runs with no flakes; git diff --check clean.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 12:31
---
Implementation handoff
Branch: task-1.3-ui-server
Worktree: /Users/seabo/seaborg-worktrees/task-1.3-ui-server
Base: 8ceb480cdfd3af94de0bd82849aa027bb1c99519
Implementation target: 7b7225a396534484dc856e33059e2d41310f54d7
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: passed
- cargo test --workspace --no-fail-fast: passed, 182 tests, 0 failures
- cargo test -p engine ui:: (5 consecutive runs): passed 67/67 each run, no flakes
- cargo clippy -p engine --all-targets: 0 warnings in engine/src/ui and src/cmdline.rs
- git diff --check: passed
- Manual: seaborg --ui played a 6-ply game; verified token, Host, Origin, content-type, method, path-traversal, request-size, SSE streaming and reconnection, --ui-port, --no-open, and mode exclusivity
Known failures: none

Reviewer notes. The dependency-free std::net server and the placeholder client were both confirmed with the user before implementation; TASK-1.4 owns the real board. Two deliberate scope boundaries worth confirming: GET /api/state and /api/events need no token (AC #4 scopes the token to mutations, and cross-origin reads are blocked by Host/Origin plus the absence of CORS headers), and there is no cap on concurrent connections, which is a local-process denial-of-service only and fits TASK-1.5 integration hardening rather than this task.
---

author: @codex
created: 2026-07-18 13:32
---
Review attempt: 1
Reviewed branch: task-1.3-ui-server
Reviewed implementation: 7b7225a396534484dc856e33059e2d41310f54d7
Verdict: changes_requested

All seven acceptance criteria are met and were independently proven (see Verification).
Both findings below are resource-ownership defects in the new server: neither breaks an
acceptance criterion, but each was reproduced deterministically against the built binary,
and each contradicts design intent already stated in this patch's own comments.

REV-1-01 [P1] An unbounded thread-per-connection kills the whole process
Location: engine/src/ui/server.rs:225
Impact: `thread::spawn` panics on EAGAIN and is called on the accept-loop thread, so the
  panic unwinds `UiServer::run`; `serving.join()` (src/cmdline.rs:100) then reports and
  exits 1. The listener has no connection cap, no pool, and no bound on concurrent SSE
  streams, so the engine process is lost mid-game. No protocol bytes are required: the
  `check_origin_headers` and `authorized` gates both run after the thread already exists.
  This is the same failure mode the accept arm deliberately guards (server.rs:219-222
  reasons explicitly about descriptor exhaustion); the adjacent spawn is unprotected, which
  reads as an oversight rather than a considered tradeoff. It is also reachable without an
  attacker: a low per-process thread limit turns ordinary stream churn into a hard exit.
Reproduction: with `seaborg --ui --ui-port 8742 --no-open`, open bare TCP connections and
  send nothing at all. At 4106 connections on this machine:
    panicked at .../thread/functions.rs:131:29:
    failed to spawn thread: Os { code: 35, kind: WouldBlock, ... }
    the Seaborg UI server stopped unexpectedly
  A follow-up connect then gets ECONNREFUSED; the process is gone.
Expected: a failed spawn is handled like a failed accept - drop or refuse the connection and
  keep serving. A cap on concurrent connections/streams would bound the condition at source.

REV-1-02 [P2] The drain path has no overall deadline, so one client pins a thread indefinitely
Location: engine/src/ui/server.rs:283-286
Impact: `DRAIN_TIMEOUT` is installed as a per-read socket timeout, not a deadline, so any
  client delivering at least one byte per 2s keeps `io::copy` productive up to `MAX_DRAIN`
  (1 MiB) - on the order of weeks on a single thread. This is exactly the anti-pattern
  `http::apply_deadline` (http.rs:165-178) was written to prevent on the request path
  ("a client that dribbles bytes cannot hold a connection thread indefinitely"); the drain
  path never calls it. It also multiplies REV-1-01 by making each pinned thread cheap and
  long-lived rather than bounded by REQUEST_DEADLINE.
Reproduction: send `POST /api/move` with `Content-Length: 20000` (over MAX_BODY, so 413 plus
  drain), then one `x` byte every 1.5s. Observed: 413 returned, then the connection held open
  for the full 60.2s of dripping, having sent 40 of 20000 bytes, and closing only when the
  client stopped.
Expected: bound the drain by an absolute deadline as the request path does, in addition to
  the existing MAX_DRAIN byte cap.

Non-blocking observations (no action required for this task; do not file follow-ups without
human approval):
- server.rs:294-308 - GET routes accept a missing `Origin`. The doc comment's rationale
  ("the token requirement covers separately") does not hold for GET, since no GET route
  requires a token; a cross-origin `<img src=".../api/events">` passes and pins a thread.
  Disclosure was refuted (nosniff + content types + `frame-ancestors 'none'` hold), so the
  impact is confined to the DoS surface above, but the stated reasoning is incorrect.
- session.rs:68-71 - `shutdown()` calls `notify_all()` without holding `published`, while
  `wait_for_update` reads `running` under it. A lost wakeup leaves an SSE thread parked up to
  KEEPALIVE_INTERVAL (15s) past shutdown. Bounded; both existing tests sleep 20ms first, so
  the window is not covered.
- wire.rs:85-108 - `write_score` does not special-case `Score::INF_P`/`INF_N` before the mate
  branches, though the comment says it mirrors `Score`'s `Display`, which does. INF_P would
  render as `{"kind":"mate","moves":-4949}` - a sign inversion. No live path found
  (search.rs:734 asserts `best_value > Score::INF_N` and terminal positions are never
  searched), so this is latent only.
- json.rs:255-262 / 179-195 - `from_str_radix` accepts `\u+041`, which also shifts the escape
  window and bypasses the lone-surrogate rejection; `number()` accepts `01`, `1.`, `-.5`.
  Harmless downstream (`as_u64` rejects non-finite/negative/fractional/>2^53, and strings are
  matched against fixed allowlists) but laxer than the module's "deliberately strict" contract.
- http.rs:193-195 - a request truncated at EOF after a complete header line is served as
  though complete, because `read_line` cannot distinguish EOF from the terminating blank line.
  No smuggling risk (always `Connection: close`, no pipelining, TE rejected).

Verification:
- cargo fmt --check: passed
- cargo test --workspace --no-fail-fast: passed, 182 tests, 0 failures, 1 pre-existing ignored
- cargo test -p engine ui:: (5 consecutive runs): 67/67 each run, no flakes
- cargo clippy -p engine --all-targets: 0 warnings in engine/src/ui and src/cmdline.rs
  (remaining warnings are pre-existing in core/engine)
- git diff --check 8ceb480..7b7225a: clean
- Benchmarks not run: the diff is additive (engine/src/ui/*, one `pub mod ui;` line, and
  src/cmdline.rs) and touches no movegen or search hot path.
- AC#1/#2/#3/#4/#5 proven live against the release binary over curl and raw sockets: loopback
  bind and URL print, --ui-port 8731 and --no-open, asset/state GET, POST move (e2e4 applied,
  engine replied a7a6), SSE head plus streaming plus reconnection (resume at current id
  replays nothing; a future id 999999 correctly re-sends current state), missing/wrong token
  403 invalid_token, Host: evil.com 403 forbidden_host, cross-origin POST with a valid token
  403 forbidden_origin, oversized body 413, path traversal 404, and CSP/no-store/nosniff/
  Referrer-Policy present on every response.
- AC#6 proven live: --ui --uci, --ui --dev, and --uci --dev each rejected with exit code 2;
  --ui-port and --no-open each require --ui.
- AC#7 reviewed for substance, not presence: the 67 tests assert distinct error codes,
  boundary cases (body exactly at MAX_BODY, truncated and extended tokens), and that rejected
  commands leave the revision unchanged.
---

author: @codex
created: 2026-07-18 14:02
---
Implementation handoff
Branch: task-1.3-ui-server
Worktree: /Users/seabo/seaborg-worktrees/task-1.3-ui-server
Base: 8ceb480cdfd3af94de0bd82849aa027bb1c99519
Implementation target: 7e472552cb398199352d5c47f2214d2bba763373
Resolved findings: REV-1-01, REV-1-02
Verification:
- cargo fmt --check: passed
- cargo clippy -p engine --all-targets: 0 warnings in engine/src/ui and src/cmdline.rs (remainder pre-existing in core/engine)
- cargo test --workspace --no-fail-fast: passed, 188 tests, 0 failures, 1 pre-existing ignored
- cargo test -p engine ui:: (5 consecutive runs): 73/73 each run, no flakes
- git diff --check: passed
- Regression tests: each of the 6 new tests was confirmed to fail against the previous implementation before being kept
- Live REV-1-01 (release binary, --ui --ui-port 8742 --no-open): 5000 bare connections absorbed where the review repro died at 4106; 503 too_many_connections returned, no panic logged, full recovery to 200 and a played move (e2e4, revision 1, engine replied to revision 2) after release
- Live REV-1-02: oversized POST then a 1-byte drip; 413 delivered, connection closed after 4.5s having accepted 3 of 20000 bytes, against the 60.2s/40-byte hold in the review
Known failures: none

Notes for the reviewer. Two points worth your judgement rather than silent acceptance.

First, REV-1-02's fix makes a deliberate trade. Bounding the drain by wall time means a client still sending when the deadline passes has its connection closed with data queued, so the kernel sends RST and that client loses its 413. A client that stops sending still receives it, which oversized_requests_are_refused_before_the_body_is_buffered covers. I took thread safety over response delivery for a peer that is by then misbehaving; say if you would rather it went the other way.

Second, the shutdown lost-wakeup observation is fixed but has no regression test, which is the one place this handoff falls short of the standard I applied elsewhere. The window is a few instructions wide and the waiter holds the published lock across all of it, so I could not reach it from outside the type. I wrote a 400-round stress test, confirmed it passed with the bug still present, and deleted it rather than bank false assurance. The reasoning is recorded at the call site in session.rs.

All five non-blocking observations were fixed in place, so no follow-up tickets were filed and none are requested. The json.rs and read_line changes are the largest of those; if you would rather they had been separate tickets, they are cleanly separable commits-worth of the diff.
---

author: @codex
created: 2026-07-18 14:12
---
Review attempt: 2
Reviewed branch: task-1.3-ui-server
Reviewed implementation: 7e472552cb398199352d5c47f2214d2bba763373
Verdict: approved

Both blocking findings from attempt 1 are resolved, each reproduced against the release binary
built from this target and each carrying a regression test. All seven acceptance criteria remain
proven. The full 8ceb480..7e47255 diff was re-reviewed, not only the fix delta.

REV-1-01 resolved. The accept loop now claims a ConnectionPermit from a MAX_CONNECTIONS (64) pool
by compare-and-swap before spawning, and the permit is owned by the connection closure so the slot
returns on any thread exit including a panic unwind. Spawning moved to thread::Builder, so a
refused thread is an io::Result stepped over with the accept backoff rather than a panic unwinding
UiServer::run. Over-cap peers are answered 503 too_many_connections on the accept thread under a
250ms write timeout and a 100ms poll-bounded drain, so a refused peer can neither stall the loop
nor lose its status to an RST. UiHandle::shutdown still works at cap, because the accepting flag is
checked before the permit is claimed. Verified live on this build: 5000 bare connections opened
against --ui-port 8791 (the attempt-1 repro died at 4106); the probe received
`HTTP/1.1 503 Service Unavailable` with `{"error":"too_many_connections"}`, the process stayed
alive with no panic logged, and a probe after releasing the flood returned `HTTP/1.1 200 OK`.

REV-1-02 resolved. drain_rejected_request now recomputes the time left against an absolute
DRAIN_DEADLINE before every read, mirroring http::apply_deadline, while keeping the MAX_DRAIN byte
cap. Verified live: an oversized POST followed by one byte every 1.5s received its 413, then the
write failed after 4.5s having sent 3 bytes, against the 60.2s / 40-byte hold recorded in attempt
1. The deliberate trade is accepted: a peer still sending when the deadline passes forfeits its
response to the reset, while a peer that stops sending still receives the 413 — confirmed
separately with a short body, which returned `HTTP/1.1 413 Payload Too Large`.

The five non-blocking observations were all fixed in place and are correct as written.
check_origin_headers now states reasoning that holds and points at MAX_CONNECTIONS for the residual
surface. Session::shutdown stores `running` under `published` before notify_all, closing the
lost-wakeup window against wait_for_update's read of the same flag under that lock. write_score
takes INF_P/INF_N ahead of the mate branches as Score's Display does, and app.js switches on the
score tag rather than assuming an untagged score is centipawns. json::hex4 requires four actual hex
digits, and number() is a JSON-grammar scanner rather than a deferral to f64::from_str.
http::read_line returns Option so EOF is distinguishable from the blank line ending the headers,
and the header loop treats EOF as malformed.

The absent regression test for the shutdown lost-wakeup is accepted. The window lies between
wait_for_update reading `running` and wait_timeout parking, and the waiter holds `published` across
all of it, so it is not reachable from outside the type; discarding a stress test that passed with
the bug present was the right call over banking false assurance. The fix is justified by locking
discipline and the reasoning is recorded at the call site.

Non-blocking observation, no action required and no follow-up requested. Refusing a connection
costs the accept loop up to REFUSE_DRAIN_POLL (10ms) for a peer that sent nothing, so a sustained
flood degrades accept throughput. This is bounded, local-process only, and arises only once the cap
is already reached — at which point the user is being refused regardless — so it is a fair trade
for delivering the 503 rather than an RST.

Verification (all at 7e47255):
- cargo fmt --check: passed
- cargo test --workspace --no-fail-fast: passed, 188 tests, 0 failures, 1 pre-existing ignored
- cargo test -p engine ui:: (8 consecutive runs): 73/73 each run, no flakes
- cargo clippy -p engine --all-targets: 0 warnings matching engine/src/ui or src/cmdline.rs after
  touching both to force recompilation (remaining warnings are pre-existing in core/engine)
- git diff --stat 7e47255..HEAD: only the task file, handoff metadata alone
- Benchmarks not run: the diff is confined to engine/src/ui/*, one `pub mod ui;` line, and
  src/cmdline.rs, and touches no movegen or search hot path
- AC#1/#2 live: `--ui --ui-port 8791 --no-open` printed the URL, lsof confirmed a single
  127.0.0.1:8791 IPv4 listener, and no browser was launched
- AC#3 live: GET / returned the page, POST /api/move applied e2e4 (revision 1) and the engine
  replied a7a6 (revision 2), and /api/events streamed `retry:`, `id:` and `data:` frames; resuming
  at the current id replayed nothing and a future id 999999 correctly re-sent current state
- AC#4 live: wrong token 403, `Host: evil.com` 403, cross-origin POST with a valid token 403
- AC#5 live: exact content types, `Cache-Control: no-store`, the full CSP, nosniff and
  `Referrer-Policy: no-referrer` on every response; path traversal 404, PUT 405, wrong
  Content-Type 415
- AC#6 live: `--ui --uci`, `--ui --dev` and `--uci --dev` each exit 2; `--ui-port` and `--no-open`
  each require `--ui` and exit 2 alone
- AC#7 reviewed for substance: the 73 tests assert distinct error codes and boundary cases, and the
  six new ones (connection cap, drain deadline, wire infinities, json escapes, json numbers, http
  truncation) each target a specific defect from attempt 1
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added engine::ui, a dependency-free loopback HTTP/1.1 server over the TASK-1.2 GameController, plus the `--ui`/`--ui-port`/`--no-open` CLI lifecycle and mode exclusivity. Embedded assets and state are served over GET, the three commands are bounded POSTs gated on the per-process session token, and updates stream over a reconnectable SSE endpoint; every response carries a restrictive CSP, nosniff, no-store, and Referrer-Policy, and Host/Origin are validated against the server's own loopback authority. The accept loop is capped at MAX_CONNECTIONS with an RAII permit and a non-panicking spawn, and both the request and drain paths are bounded by absolute deadlines. Verified at 7e47255: cargo fmt --check clean, cargo clippy -p engine --all-targets with zero warnings in engine/src/ui and src/cmdline.rs, cargo test --workspace --no-fail-fast passing 188 tests with 0 failures, cargo test -p engine ui:: passing 73/73 on 8 consecutive runs, and live verification against the release binary of startup, gameplay, security headers, SSE streaming and reconnection, mode exclusivity, a 5000-connection flood answered 503 with full recovery, and a dripping client cut off after 4.5s.
<!-- SECTION:FINAL_SUMMARY:END -->
