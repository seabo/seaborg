---
id: TASK-74.4
title: >-
  Lichess accept path: cap accounting at accept-time, challengeCanceled, benign
  404, and human-slot priority
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-21 03:55'
updated_date: '2026-07-21 12:44'
labels:
  - lichess
  - conformance
dependencies:
  - TASK-74.1
parent_task_id: TASK-74
priority: high
type: bug
ordinal: 123000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The whole incoming-challenge acceptance path, brought to reference behaviour. Depends on the harness (TASK-74.1) to pin its scenarios.

CAP ACCOUNTING: seaborg only records a game as active on gameStart (lichess/src/run.rs ActiveGames populated in the GameStart arm). Between accepting a challenge and gameStart arriving the count is stale, so it can accept several challenges that all exceed max_concurrent_games. Reference lichess-bot inserts the game id into active_games the moment it accepts (reserving the slot before gameStart), frees that reservation on challengeCanceled, and swallows a 404 on accept as Skip missing without re-queueing. Fix: reserve a slot on accept and reconcile it when the matching gameStart arrives (no double count) or release it if the game is canceled / accept fails; the cap check must count reserved-but-not-started games. seaborg currently parses challengeCanceled as Event::Other (ignored) and logs accept-404 as WARN even though 404 is the spec challenge-gone outcome.

HUMAN PRIORITY: seaborg accepts inline in arrival order and only reserves human slots inside the matchmaking cap, not in the acceptance path, so with max_concurrent_games=1 and matchmaking on, incoming bot challenges/games take the only slot and a human is locked out. Reference queues supported challenges and accepts in priority order (sort_by, preference human/bot); games_reserved_for_humans reduces max_bot_games, and a bot challenge at the queue head blocks acceptance to hold remaining slots for humans. Fix: add a short-lived accept queue (or equivalent ordering step) and enforce reserved-human-slots on the acceptance side so a configured number of slots stays reachable by humans even while bot games/challenges are active; make the existing reserved_human_slots concept apply to incoming acceptance, not just outgoing matchmaking.

References: lichess-bot lib/lichess_bot.py (accept_challenges, sort_challenges, active_games reservation, challengeCanceled handling); Lichess OpenAPI challengeAccept (404 = challenge not found).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 With max_concurrent_games = N and N accepted challenges awaiting gameStart, an additional incoming challenge is declined for capacity rather than accepted
- [ ] #2 Accepting a challenge reserves a slot that is reconciled (not double-counted) when its gameStart arrives, and released if the challenge is canceled or the accept fails
- [ ] #3 A challengeCanceled event releases any slot reserved for that challenge
- [ ] #4 A 404 response to accept is treated as an expected challenge-gone outcome and does not surface as a warning or an error to the caller
- [ ] #5 A configured number of game slots is reserved on the acceptance side so a human challenge can be accepted even when bot challenges/games would otherwise fill the cap, and a bot challenge is not accepted into a reserved human slot
- [ ] #6 When multiple challenges are pending and a preference is configured, human challenges are accepted ahead of bot challenges
- [ ] #7 Pinned harness scenarios cover: over-cap accept prevention, challengeCanceled releasing a reserved slot, a benign 404 accept, a human accepted ahead of a queued bot challenge, and a bot held out of a reserved human slot
- [ ] #8 cargo fmt --check, cargo clippy --workspace --all-targets --all-features -D warnings, and cargo test --workspace all pass
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. event.rs: add ChallengeCanceled event (challenge id) so a withdrawn challenge is no longer swallowed as Other; keep #[serde(other)] for genuinely-unknown types. Add tests.
2. error.rs + transport.rs: add Error::NotFound, map HTTP 404 to it in check_status; is_recoverable() stays true so existing swallow-behaviour is unchanged everywhere except the accept site.
3. run.rs slot model: replace ActiveGames' HashSet with a Reserved/Active state map. reserve(id) on accept, start(id) promotes Reserved->Active (reconcile, no double count) or inserts a fresh Active for matchmaking-accepted games, release_reservation(id) frees a Reserved slot on cancel/accept-failure, remove(id) on finish/worker-exit. len() counts reserved+active so the cap sees reserved-but-not-started games. (Relies on Lichess challenge id == resulting game id.)
4. policy.rs: split cap check out of evaluate into a suitability classification; the cap/reservation/priority decision moves to the accept queue.
5. run.rs accept queue: handle_event enqueues suitable challenges and declines unsuitable ones immediately. process_accept_queue sorts by preference (humans first when configured, stable) and, per challenge, accepts under an effective cap (bots: max - reserved_human_slots; humans: max), reserving a slot then POSTing accept; a benign 404 or other error releases the reservation. Over-cap challenges are declined (generic). Consumer drains available events then processes the queue.
6. config.rs: add challenge.prefer_human_challenges (default false); apply the existing matchmaking.reserved_human_slots to the acceptance side.
7. Extend the replay harness to drive batches so priority/reservation scenarios are pinnable; pin over-cap decline, challengeCanceled release, benign 404, human-ahead-of-bot, bot-held-out-of-reserved-slot. Update fixtures so challenge id == game id.
8. cargo fmt/clippy/test.
<!-- SECTION:PLAN:END -->
