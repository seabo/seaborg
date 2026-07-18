---
id: TASK-1.3
title: Add the loopback UI server and `--ui` lifecycle
status: Changes Requested
assignee:
  - '@codex'
created_date: '2026-07-17 15:40'
updated_date: '2026-07-18 13:32'
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
- [ ] #1 `seaborg --ui` binds to 127.0.0.1 on an available port, prints the URL, and opens it only after the listener is ready
- [ ] #2 `--ui-port` selects a fixed port and `--no-open` suppresses browser launch, with clear errors for bind or launch failures
- [ ] #3 Embedded application assets and current state are available over GET, commands use bounded POST endpoints, and updates stream through a reconnectable Server-Sent Events endpoint
- [ ] #4 Mutating requests require the process session token and unexpected Host or Origin values are rejected
- [ ] #5 Responses set appropriate content types, no-store state caching, and a restrictive Content Security Policy
- [ ] #6 `--ui`, `--uci`, and `--dev` cannot be selected together
- [ ] #7 Protocol tests cover startup, state retrieval, command validation, SSE reconnection, request limits, and shutdown
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add engine::ui module: hand-rolled loopback HTTP/1.1 server on std::net::TcpListener (no new dependencies), thread-per-connection, with a small owned JSON writer for snapshot serialization.
2. Bind 127.0.0.1 on port 0 (or --ui-port), resolve the actual local port from the listener, print the URL, and only then launch the browser via the existing open crate; --no-open suppresses launch. Report bind and launch failures with clear errors.
3. Route a narrow fixed surface: GET / and static embedded assets (include_str!/include_bytes! placeholder index.html, app.js, style.css), GET state snapshot JSON, bounded POST command endpoints (move, undo, reset, new game), and a reconnectable SSE stream carrying versioned snapshots and search progress.
4. Own a single GameController behind a Mutex on a dedicated driver thread that calls poll() on a cadence and publishes revisions to SSE subscribers; SSE clients reconnect by last-seen revision and immediately receive current state.
5. Enforce local security: per-process random session token required on all mutating requests, Host and Origin allowlist restricted to loopback, capped request line/header/body sizes, restrictive Content-Security-Policy, no-store on state responses, correct content types, and no arbitrary file or generic engine-command endpoint.
6. Wire the CLI: add --ui, --ui-port, --no-open to clap Args and make --ui, --uci, --dev mutually exclusive; integrate startup and graceful shutdown.
7. Add protocol tests driving the server over real loopback sockets: startup and port selection, asset and state retrieval, command validation (illegal, stale revision, missing/incorrect token, bad Host/Origin, oversized body), SSE streaming and reconnection, request limits, mutual-exclusion flag errors, and shutdown.
8. Run cargo fmt --check and cargo test --workspace, recording any pre-existing baseline failure, then commit the implementation target and hand off to review.
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
<!-- COMMENTS:END -->
