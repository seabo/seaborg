---
id: TASK-74.2
title: >-
  Ignore self-authored (from_self) Lichess challenges instead of trying to
  accept them
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
ordinal: 121000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Live bug. The account event stream delivers a challenge event for BOTH incoming challenges and the bot own outgoing challenges (the Lichess spec ChallengeJson carries an optional direction enum in/out; the challenger of an outgoing one is the bot itself). seaborg handle_event (lichess/src/run.rs) runs every challenge event through policy::evaluate and calls accept_challenge, so for its own matchmaking challenges it POSTs /api/challenge/{id}/accept and gets 404 Not found (you cannot accept a challenge you sent). Observed in production logs: "accepting challenge X from seaborg1" -> 404 -> opponent then declines the real outgoing challenge.

Reference behaviour: lichess-bot handle_challenge returns immediately when chlng.from_self, and the accept queue defensively discards from_self entries too. from_self is computed as challenger.name == own username.

Fix: identify the bot own identity (the run loop already holds account.id/bot_id) and skip challenge events the bot itself issued before evaluating/accepting/declining them. Because the spec marks direction OPTIONAL, the primary check must be challenger-identity (challenger.id == own id), with direction used only as corroboration when present. Parse direction into the Challenge type so it is available and exercised by fixtures.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A challenge event whose challenger is the bot itself is ignored: no accept and no decline call is made
- [ ] #2 Self-identification is by challenger identity (own account id), not solely the optional direction field, and works when direction is absent
- [ ] #3 When present, a direction of out is treated as outgoing and in as incoming; parsing tolerates the field being absent
- [ ] #4 A genuine incoming challenge (challenger != self) is still evaluated and accepted/declined exactly as before
- [ ] #5 Pinned harness scenarios cover: (a) an outgoing/self challenge echoed on the stream is ignored, (b) an incoming challenge is still accepted, (c) a self challenge with direction absent is still ignored
- [ ] #6 cargo fmt --check, cargo clippy -D warnings, and cargo test --workspace all pass
<!-- AC:END -->
