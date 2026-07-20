---
id: TASK-68.5
title: 'Harden the Lichess bot: reconnect, rate limits, chat, seeks'
status: Ready to Merge
assignee:
  - '@george'
created_date: '2026-07-19 22:34'
updated_date: '2026-07-20 15:13'
labels: []
dependencies:
  - TASK-68.4
references:
  - 'https://lichess.org/api'
parent_task_id: TASK-68
priority: low
type: enhancement
ordinal: 91000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Production-hardening and optional niceties on top of a working bot (TASK-68.4). Keep each item small and independently reviewable; split further if a reviewer would prefer.

Scope:
- Stream resilience: the event stream and per-game streams drop routinely. Reconnect with exponential backoff; distinguish recoverable disconnects from terminal game-over.
- Rate limiting: honor HTTP 429 with backoff; use a single shared client/connection pool; avoid hammering endpoints.
- Graceful shutdown: on Ctrl-C stop accepting new challenges and let in-flight games finish (or resign) cleanly rather than dropping connections mid-move.
- Chat (optional): a greeting and/or 'good game' message; optionally respond to a small set of chat commands.
- Proactive play (optional): issue challenges or open seeks when idle, per config. Note this needs the `challenge:write` token scope in addition to `bot:play`.

Everything here is additive; the bot from TASK-68.4 must keep working if the optional items are deferred.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Dropped event/game streams reconnect with exponential backoff without crashing or abandoning live games
- [x] #2 HTTP 429 responses are handled with backoff and do not crash the bot
- [x] #3 Ctrl-C triggers a graceful shutdown that stops accepting challenges and does not corrupt in-flight games
- [x] #4 Any optional chat or proactive-challenge features added are gated by config and documented, including the challenge:write scope requirement for issuing challenges
- [x] #5 cargo fmt --check, clippy (workspace, all-targets, all-features, -D warnings), and cargo test --workspace all pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Scope the three mandatory hardening pillars (reconnect+backoff, HTTP 429, graceful shutdown). Defer optional chat/proactive play; AC#4 is then vacuous (no optional feature added) but document the challenge:write scope note for the deferred work.
2. New backoff.rs: pure exponential Backoff (base/max, doubling, reset); unit-tested schedule.
3. New shutdown.rs: injectable Shutdown handle (owned Arc for tests, static+signal for prod) and a Unix SIGINT/SIGTERM handler via the already-present libc; no-op on non-Unix.
4. error.rs: add RateLimited { retry_after } variant for HTTP 429.
5. transport.rs: single shared ureq::Agent (one pool); manual status handling (401 -> Unauthorized, 429 -> RateLimited with Retry-After, other non-2xx -> Http); connect/response timeouts; bounded 429 retry with backoff that aborts promptly on shutdown, factored into a testable helper.
6. client.rs: surface keepalive ticks (yield Result<Option<Event>>) so the loops can observe shutdown between real events; add resign_game.
7. run.rs: resilient event loop that reconnects with backoff, keeps a de-duplicated active-game set across reconnects (so an event-stream replay never double-spawns a worker), declines/stops accepting on shutdown, spawns workers with a Shutdown clone, and joins workers on shutdown.
8. game.rs: resilient per-game loop that reconnects on a mid-game drop but stops on a terminal status; on shutdown it resigns the in-flight game cleanly instead of making another move.
9. Tests: backoff schedule, shutdown handle, 429 retry helper, event-loop reconnect+dedup+shutdown-decline, game reconnect+shutdown-resign, all via fake transports with an injected no-op sleeper (no network, no real sleeps).
10. Run cargo fmt --check, clippy (-D warnings), cargo test --workspace.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the three mandatory hardening pillars; deferred the optional niceties.

Key decisions:
- New backoff.rs (exponential Backoff, base 1s / cap 30s for reconnects) and shutdown.rs (Shutdown flag with an owned backing for tests and a static backing set by a Unix SIGINT/SIGTERM handler). Used libc directly for the signal handler: it is already in the build graph via ureq's TLS stack, so this adds zero new crates, whereas a dedicated Ctrl-C crate pulled in ~7 (nix/objc2/block2/...). The handler is cfg(unix); non-Unix builds get a handle tripped only programmatically.
- Transport now owns one shared ureq::Agent (single connection pool, AC#2) with http_status_as_error(false) so it can map statuses itself: 401 -> Unauthorized, 429 -> RateLimited{retry_after}, other non-2xx -> Http. 429 is retried with backoff honoring Retry-After, in a helper that aborts promptly on shutdown (Shutdown::sleep chunks the wait). Connect/response-header timeouts are set; deliberately NO recv-body timeout, since the streams are long-lived bodies.
- Stream reconnect lives in run.rs (event stream) and game.rs (per-game). A drop that is not a terminal game status reconnects with backoff; terminal status stops. Because the position is rebuilt from the server's authoritative move list every state, a reconnect resumes in sync (no incremental drift). Keepalive lines are now surfaced (event_stream/game_stream yield Result<Option<Event>>) so both loops observe a shutdown request between real events.
- Duplicate-worker guard: the event stream replays in-progress games on reconnect, so gameStart is de-duplicated against an ActiveGames set. That set is shared (Arc<Mutex<HashSet>>) and each worker removes its own game on exit, so the concurrency cap stays correct even if a gameFinish is missed while disconnected -- a failure mode the old event-only count could not handle.
- Graceful shutdown: on Ctrl-C the event loop stops accepting challenges and returns; each in-flight worker resigns via POST /api/bot/game/{id}/resign (bounded, clean) rather than dropping mid-move, and on_state refuses to start a new search or submit once shutdown is set; run() joins all workers before returning. A second Ctrl-C escalates to the OS default (force quit) under BSD-persistent signal semantics on macOS.

Scope / AC#4: optional chat and proactive-challenge (seek) features were deferred to keep the change focused and independently reviewable, so no optional feature is config-gated in this task (AC#4 is vacuous). Recorded here for whoever adds proactive play later: issuing challenges/opening seeks needs the challenge:write token scope in addition to bot:play.

Verification (workspace, in the task worktree):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (lichess 68 tests incl. new backoff/shutdown/429-retry/reconnect/dedup/resign coverage; whole workspace green)
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-20 14:25
---
Implementation handoff
Branch: task-68.5-lichess-hardening
Worktree: /Users/seabo/seaborg-worktrees/task-68.5-lichess-hardening
Base: 1a5c1ef1d9193d719753b6af29a241731cf06c4a
Implementation target: 9e1891ee068c75b19e1b1e16a8afea96afa852b0
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (no warnings)
- cargo test --workspace: pass (lichess 68 tests; whole workspace green, 2 pre-existing ignored in engine)
Known failures: none

Reviewer notes:
- AC#4 is vacuous: optional chat/proactive-challenge features were deferred, so nothing new is config-gated. The challenge:write scope requirement for issuing challenges is recorded in the implementation notes for the future proactive-play work.
- Graceful-shutdown wake-up relies on Lichess stream keepalives plus connect/response-header timeouts; there is intentionally no recv-body timeout on the long-lived streams. Signal handling is cfg(unix) via libc (no new crates).
---

author: @george
created: 2026-07-20 15:13
---
Review attempt: 1
Reviewed branch: task-68.5-lichess-hardening
Reviewed implementation: 9e1891ee068c75b19e1b1e16a8afea96afa852b0
Base: 1a5c1ef1d9193d719753b6af29a241731cf06c4a
Verdict: approved

Immutability: target 9e1891e descends from the recorded base 1a5c1ef; the only commit after it (branch tip 2fefbdf) touches solely the backlog task file, so no implementation file changed after the reviewed SHA. Worktree clean.

Acceptance criteria (all proven):
- #1 Event loop and per-game loop both reconnect with exponential Backoff (base 1s / cap 30s). ActiveGames dedups a replayed gameStart across reconnects; per-game reconnect resyncs from the authoritative move list. Covered by event_loop_reconnects_after_a_drop_then_stops_on_shutdown, reconnects_after_a_midgame_drop_and_finishes, duplicate_game_start_does_not_spawn_a_second_worker, active_games_tracks_membership_and_frees_slots.
- #2 429 mapped to Error::RateLimited and retried by with_rate_limit_retry (honors Retry-After, backoff fallback, bounded attempts, shutdown-aware) over one shared ureq::Agent. Covered by the five transport retry tests.
- #3 cfg(unix) SIGINT/SIGTERM handler trips a shared Shutdown; loop stops accepting, workers resign cleanly, run() joins workers. Covered by resigns_the_in_flight_game_on_shutdown, shutdown_midstream_stops_before_moving, event_loop_returns_immediately_when_already_shut_down.
- #4 Vacuous: optional chat/proactive play deferred (task sanctions deferral); challenge:write scope documented in implementation notes.
- #5 Required checks pass (see below).

Scope: changes confined to the lichess crate plus Cargo.lock/Cargo.toml (libc target-gated to cfg(unix), no new crate in the graph). No new #[allow], no source comments citing task/AC/finding IDs, no movegen/search hot-path changes (no benchmark required).

Verification (run on the implementation target in the task worktree):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, exit 0, fresh CARGO_TARGET_DIR, 0 warnings
- cargo test --workspace: pass (lichess 68; whole workspace green, 2 pre-existing ignored)

Non-blocking observation (not required for these ACs; consistent with the pre-TASK-68.4 design): a transient Error::Http on a move-submission POST propagates out of on_state and terminates that game worker rather than reconnecting, since the reconnect loop only converts stream-open/stream-read failures to a reconnect. 429s on moves are still retried by the transport. Worth considering in a future hardening pass but outside AC#1's stream-drop scope and not a regression.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Hardened the Lichess bot with three mandatory pillars: (1) exponential-backoff reconnect for the account event stream (run.rs) and each per-game stream (game.rs), with an Arc<Mutex<HashSet>> ActiveGames set that survives event-stream reconnects so a replayed gameStart never double-spawns a worker and the concurrency cap stays correct even across a missed gameFinish; (2) HTTP 429 handling in a single shared ureq::Agent (one connection pool) via with_rate_limit_retry, which honors Retry-After, falls back to a doubling backoff, is bounded, and aborts promptly on shutdown; (3) graceful shutdown via a cfg(unix) SIGINT/SIGTERM handler that trips a shared Shutdown flag, after which the event loop stops accepting challenges, in-flight workers resign cleanly (POST .../resign) instead of dropping mid-move, and run() joins every worker before returning. Optional chat/proactive-play were deferred as the task sanctions; the challenge:write scope requirement is documented for that future work (AC#4 vacuous). Verified on implementation target 9e1891ee068c75b19e1b1e16a8afea96afa852b0: cargo fmt --check clean; cargo clippy --workspace --all-targets --all-features -- -D warnings clean (exit 0, fresh CARGO_TARGET_DIR, 0 warnings); cargo test --workspace green (lichess 68 tests incl. backoff schedule, shutdown handle, 429-retry helper, event-loop reconnect/dedup/shutdown-decline, per-game reconnect and shutdown-resign; whole workspace passing, 2 pre-existing ignored).
<!-- SECTION:FINAL_SUMMARY:END -->
