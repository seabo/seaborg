---
id: TASK-74.5
title: >-
  Give human challengers priority in Lichess challenge acceptance (accept
  ordering + reserved human slots)
status: To Do
assignee: []
created_date: '2026-07-21 03:55'
updated_date: '2026-07-21 03:56'
labels:
  - lichess
  - conformance
dependencies:
  - TASK-74.4
parent_task_id: TASK-74
priority: medium
type: feature
ordinal: 124000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
seaborg accepts incoming challenges inline in arrival order and only reserves human slots inside the matchmaking cap, not in the acceptance path. With max_concurrent_games = 1 and matchmaking enabled, incoming bot challenges (or matchmaking-started games) can take the only slot, so a human trying to challenge the bot is effectively locked out.

Reference behaviour: supported challenges are queued and accepted in priority order (sort_by best/first, preference human/bot); games_reserved_for_humans reduces the bot-game cap (max_bot_games = max_games - reserved), and a bot challenge at the queue head blocks acceptance to hold the remaining slots open for humans.

Fix: introduce a short-lived accept queue (or an equivalent ordering step) and enforce reserved-human-slots on the acceptance side so a configured number of slots is always reachable by human challengers even while matchmaking/bot games are active. Reuse the existing reserved_human_slots config concept and make it apply to incoming acceptance, not just outgoing matchmaking. Depends on the accept-time slot accounting task.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A configured number of game slots is reserved so a human challenge can be accepted even when bot challenges/games would otherwise fill the cap
- [ ] #2 When multiple challenges are pending and preference is set, human challenges are accepted ahead of bot challenges
- [ ] #3 A bot challenge is not accepted into a slot reserved for humans (it waits or is declined per policy) while a human-reachable slot must remain free
- [ ] #4 Pinned harness scenarios cover: a human accepted ahead of a queued bot challenge, and a bot challenge held out of a reserved human slot
- [ ] #5 cargo fmt --check, cargo clippy -D warnings, and cargo test --workspace all pass
<!-- AC:END -->
