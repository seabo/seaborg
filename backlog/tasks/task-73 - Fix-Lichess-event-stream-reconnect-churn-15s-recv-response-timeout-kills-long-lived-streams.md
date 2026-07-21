---
id: TASK-73
title: >-
  Fix Lichess event-stream reconnect churn: 15s recv-response timeout kills
  long-lived streams
status: To Do
assignee: []
created_date: '2026-07-21 03:03'
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
