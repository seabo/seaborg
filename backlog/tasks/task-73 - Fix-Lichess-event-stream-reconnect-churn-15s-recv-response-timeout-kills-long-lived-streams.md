---
id: TASK-73
title: >-
  Fix Lichess event-stream reconnect churn: 15s recv-response timeout kills
  long-lived streams
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-21 03:03'
updated_date: '2026-07-21 03:12'
labels: []
dependencies: []
priority: high
type: bug
ordinal: 118000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Lichess bot reconnects its event stream every ~15 seconds even when the stream is healthy, logging a steady "event stream disconnected; reconnecting" and starving matchmaking. Root cause: the shared ureq agent in lichess/src/transport.rs sets timeout_recv_response(15s) (RESPONSE_TIMEOUT). The code comment there claims this bounds only the response-header phase and is "deliberately not a body timeout", but that assumption is false for streaming responses in ureq 3.3.0: the deadline is absolute from request-send and applies to the whole long-lived body, so it fires at 15s regardless of Lichess keepalives (~every 7s).

Verified empirically: curl on /api/stream/event stays open 45s+; a probe using the exact agent config dies at 15.06s with "timeout: receive response" despite keepalives at 7s and 14s; the same probe with timeout_recv_response removed streams past 42s. The user log confirms it (connect->first disconnect = 15.02s, gaps grow 16->17->19s as reconnect backoff doubles).

Secondary effect on matchmaking: matchmaking is ticked inside the event-stream loop, so it only runs during the ~15s alive windows. The reconnect backoff (run_event_loop) only resets on a real event (made_progress); an idle bot receives only keepalives, which do not count, so the backoff never resets and doubles toward its 30s cap. Within a couple of minutes the bot is alive ~15s then asleep up to ~30s, issuing challenges rarely and irregularly. Fixing the timeout removes both symptoms.

The fix must keep the 15s header timeout for ordinary request/response calls (get/post_empty/post_form), where it is correct, and only exempt the streaming path (open_stream). Note ureq timeouts are absolute-duration, not idle, so there is no clean "reconnect only when the stream goes silent" knob; matching curl (no receive timeout on streams, letting TCP-level failure end a dead connection) is the practical approach.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A healthy Lichess event stream stays connected well beyond 15s (no "event stream disconnected; reconnecting" on a live stream); streaming endpoints (open_stream) are not subject to the 15s recv-response timeout
- [ ] #2 Ordinary non-streaming request/response calls (get, post_empty, post_form) retain a bounded response-header timeout so a hung server does not block them indefinitely
- [ ] #3 A genuinely dropped/dead stream still triggers reconnect with the existing exponential backoff, and terminal (non-recoverable) errors are still surfaced
- [ ] #4 The misleading comment on RESPONSE_TIMEOUT in transport.rs is corrected to reflect that ureq applies recv-response to streaming bodies
- [ ] #5 Regression coverage exists (unit/integration) demonstrating the streaming path does not impose the 15s response-receive deadline, without requiring live network access
- [ ] #6 cargo fmt --check, clippy (workspace, all-targets, all-features, -D warnings), and cargo test --workspace all pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Refactor HttpTransport agent construction to inject the recv-response timeout, so tests can build the agent with a short deadline without live network.
2. In open_stream, override the request config with timeout_recv_response(None) so the long-lived streaming body is not bound by the header timeout; keep get/post_empty/post_form using the shared agent's 15s recv-response bound.
3. Correct the RESPONSE_TIMEOUT comment: ureq applies recv-response to streaming bodies (deadline persists as a preceeding timeout during body reads), so it is not header-only; document why the stream path exempts it.
4. Add a regression test using a local TcpListener HTTP server: with a short agent recv-response timeout and a server that delays the body past that deadline, open_stream still receives all lines (override effective) while a non-streaming get() against a slow body times out (bound retained).
5. Run fmt, clippy -D warnings, and cargo test --workspace.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Root cause confirmed against ureq 3.3.0 source: RecvBody's next_timeout still
evaluates the RecvResponse deadline as a preceding phase (timings.rs
`preceeding`), so timeout_recv_response bounds the streamed body, not just
headers. Fix clears it per request on the streaming path only.

Changes (lichess/src/transport.rs):
- open_stream: `.config().timeout_recv_response(None).build()` on the GET, so
  the long-lived body has no receive deadline; a dead stream still ends via
  TCP failure -> Error, which the existing run_event_loop backoff reconnects on
  (unchanged).
- get/post_empty/post_form: unchanged, retain the shared agent's 15s bound.
- Refactored agent construction into private with_response_timeout(...) so tests
  inject a short bound; new() delegates with RESPONSE_TIMEOUT (15s).
- Rewrote the RESPONSE_TIMEOUT doc comment to state recv-response also caps body
  reception and why the stream path exempts it.
- Regression tests (loopback TcpListener, no network): streaming survives a
  600ms body gap under a 200ms bound; a non-streaming get times out on an
  800ms body stall under the same bound.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 03:12
---
Implementation handoff
Branch: task-73-lichess-stream-recv-timeout
Worktree: /Users/seabo/seaborg-worktrees/task-73-lichess-stream-recv-timeout
Base: 05880a59a02a47f388fafad164e482fb764c7ccc
Implementation target: c8954cbdade6f44e87d2c3c23937a9c71165abfa
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (all suites; lichess 100 passed, incl. 2 new transport tests)
Known failures: none
---
<!-- COMMENTS:END -->
