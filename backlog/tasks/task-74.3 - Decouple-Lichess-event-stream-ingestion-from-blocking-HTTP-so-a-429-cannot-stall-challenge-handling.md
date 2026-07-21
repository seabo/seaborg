---
id: TASK-74.3
title: >-
  Decouple Lichess event-stream ingestion from blocking HTTP so a 429 cannot
  stall challenge handling
status: To Do
assignee: []
created_date: '2026-07-21 03:55'
labels:
  - lichess
  - conformance
dependencies: []
parent_task_id: TASK-74
priority: high
type: bug
ordinal: 122000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Root cause of the "challenge from the UI just hangs" symptom. seaborg reads the account event stream and performs matchmaking/accept HTTP on the same single thread (lichess/src/run.rs run_event_stream_once calls maybe_seek_matchmaking_game inline after each event/keepalive). A challenge-create or bot-list call that hits HTTP 429 goes into the transport rate-limit backoff (RATE_LIMIT_BASE 60s up to 600s, on the calling thread). While it sleeps, no stream lines are read, so an incoming human challenge event is not processed until the backoff ends -> the UI spinner hangs for up to minutes.

Reference behaviour: lichess-bot reads GET /api/stream/event in a dedicated process that only decodes lines onto a queue; all accept/decline/matchmaking HTTP (and therefore any 429 backoff) runs in a separate consumer, so stream ingestion is never blocked by an outbound-call backoff.

Fix (seaborg-idiomatic, std threads): read the event stream on its own thread that pushes decoded events (and keepalive ticks) onto a channel; run event handling and matchmaking on the consumer. Reading must remain responsive to shutdown and reconnect. Alternatively/additionally bound outbound-call backoff so it cannot indefinitely starve ingestion — but isolation is the target design. Reconnect/backoff semantics from TASK-73 must be preserved.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Event-stream line reading runs independently of accept/decline/matchmaking HTTP, so an outbound-call rate-limit backoff does not delay processing of already-delivered events
- [ ] #2 A pinned/integration test demonstrates that while an outbound matchmaking call is blocked (simulated stall/429), a concurrently delivered incoming challenge is still handled promptly
- [ ] #3 Shutdown remains prompt (no waiting out a full backoff) and reconnect-with-backoff behaviour from TASK-73 is preserved
- [ ] #4 No duplicate game workers on gameStart replay; existing active-set dedup still holds
- [ ] #5 cargo fmt --check, cargo clippy -D warnings, and cargo test --workspace all pass
<!-- AC:END -->
