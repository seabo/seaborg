---
id: TASK-74.1
title: >-
  Fix self-authored (from_self) Lichess challenges and build the event-replay
  conformance harness
status: To Do
assignee: []
created_date: '2026-07-21 03:54'
updated_date: '2026-07-21 04:02'
labels:
  - lichess
  - conformance
  - testing
dependencies: []
parent_task_id: TASK-74
priority: high
type: chore
ordinal: 120000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Combines the live from_self bug fix with the shared test mechanism that pins it, since the harness first real scenario IS the from_self case.

LIVE BUG (from_self): the account event stream delivers a challenge event for BOTH incoming challenges and the bot own outgoing challenges (Lichess ChallengeJson carries an optional direction enum in/out; the challenger of an outgoing one is the bot itself). seaborg handle_event (lichess/src/run.rs) runs every challenge event through policy::evaluate and calls accept_challenge, so for its own matchmaking challenges it POSTs /api/challenge/{id}/accept and gets 404. Reference lichess-bot handle_challenge returns immediately when chlng.from_self (challenger.name == own username) and the accept queue discards from_self entries defensively. Fix: identify the bot own identity (the run loop already holds account.id/bot_id) and skip challenge events the bot itself issued before evaluating them. direction is OPTIONAL per spec, so the primary check must be challenger identity (challenger.id == own id), with direction parsed and used only as corroboration.

HARNESS: create the reusable test mechanism the rest of the sweep pins cases into: replay a recorded sequence of account-event NDJSON lines through the real event loop against a fake transport, and assert the exact outbound API calls (accept/decline/create/cancel, with ids and decline reasons) and resulting active-slot state. Generalises the existing fake-transport tests (lichess/src/run.rs test module, event.rs tests) into a table/fixture form. Use captured/real Lichess JSON verbatim where possible so fixtures include fields seaborg does not yet parse (direction, destUser, speed, perf, color/finalColor), exercising unknown-field tolerance.

References: lichess-bot lib/lichess_bot.py (handle_challenge, accept_challenges), lib/model.py (Challenge.from_self); Lichess OpenAPI ChallengeJson (direction enum in/out, optional), apiStreamEvent (replays all current challenges and games on connect).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The harness can replay an ordered list of NDJSON event lines through the event loop against a fake transport and assert the ordered outbound accept/decline/create/cancel calls (with ids and decline reasons) plus the final active-games slot count
- [ ] #2 Fixtures use captured/real Lichess JSON shapes including fields not yet parsed (direction, destUser, speed, perf, color/finalColor), exercising unknown-field tolerance
- [ ] #3 A challenge event whose challenger is the bot itself is ignored: no accept and no decline call is made
- [ ] #4 Self-identification is by challenger identity (own account id) and works when direction is absent; direction is parsed and, when present, out is treated as outgoing and in as incoming
- [ ] #5 A genuine incoming challenge (challenger != self) is still evaluated and accepted/declined exactly as before
- [ ] #6 Pinned scenarios cover: (a) a self/outgoing challenge echoed on the stream is ignored, (b) an incoming human challenge that passes policy is accepted once and starts one game, (c) a self challenge with direction absent is still ignored
- [ ] #7 cargo fmt --check, cargo clippy --workspace --all-targets --all-features -D warnings, and cargo test --workspace all pass
<!-- AC:END -->
