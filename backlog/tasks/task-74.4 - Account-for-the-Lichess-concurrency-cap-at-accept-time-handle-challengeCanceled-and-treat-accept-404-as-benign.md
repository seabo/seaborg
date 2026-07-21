---
id: TASK-74.4
title: >-
  Lichess accept path: cap accounting at accept-time, challengeCanceled, benign
  404, and human-slot priority
status: Done
assignee:
  - '@claude'
created_date: '2026-07-21 03:55'
updated_date: '2026-07-21 13:33'
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
- [x] #1 With max_concurrent_games = N and N accepted challenges awaiting gameStart, an additional incoming challenge is declined for capacity rather than accepted
- [x] #2 Accepting a challenge reserves a slot that is reconciled (not double-counted) when its gameStart arrives, and released if the challenge is canceled or the accept fails
- [x] #3 A challengeCanceled event releases any slot reserved for that challenge
- [x] #4 A 404 response to accept is treated as an expected challenge-gone outcome and does not surface as a warning or an error to the caller
- [x] #5 A configured number of game slots is reserved on the acceptance side so a human challenge can be accepted even when bot challenges/games would otherwise fill the cap, and a bot challenge is not accepted into a reserved human slot
- [x] #6 When multiple challenges are pending and a preference is configured, human challenges are accepted ahead of bot challenges
- [x] #7 Pinned harness scenarios cover: over-cap accept prevention, challengeCanceled releasing a reserved slot, a benign 404 accept, a human accepted ahead of a queued bot challenge, and a bot held out of a reserved human slot
- [x] #8 cargo fmt --check, cargo clippy --workspace --all-targets --all-features -D warnings, and cargo test --workspace all pass
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implementation decisions:
- Slot reconciliation relies on Lichess reusing the challenge id as the resulting game id (reference lichess-bot depends on the same). GameSlots is a Reserved/Active state map keyed by that id: reserve() on accept, start() promotes Reserved->Active in place (or records a fresh Active for an accepted outgoing matchmaking challenge that was never reserved), release_reservation() frees a still-reserved slot on cancel/accept-failure, remove() on finish. len() counts reserved+active, so the cap sees reserved-but-not-started games. Updated the harness gameStart fixture id to equal the challenge id to reflect this.
- Acceptance is deferred to a short-lived queue drained once per event burst (the consumer drains all immediately-available events, then processes). Unsuitable challenges are still declined immediately; suitable ones are buffered so a burst can be sorted (humans first when challenge.prefer_human_challenges is set; stable) and checked against an effective cap: bots below max_concurrent_games - reserved_human_slots, humans below max_concurrent_games. Over-cap challenges are declined generic.
- reserved_human_slots (in [matchmaking]) now also gates acceptance, per the task; it takes effect whether or not matchmaking is enabled. Documented in config.rs.
- 404 on accept maps to a new Error::NotFound (is_recoverable() stays true so unrelated call sites are unchanged); the accept site matches it explicitly to release the reservation and log at debug, not warn.
- policy::evaluate split into cap-free policy::classify; the cap/reservation/priority decision moved to run.rs::process_accept_queue.
- Replay harness generalised to drive event batches (&[&[&str]]); handle_one_stream processes per-event. Added new deferred tool: FakeTransport can 404 a configured accept id.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 13:03
---
Implementation handoff
Branch: task-74.4-lichess-accept-path
Worktree: /Users/seabo/seaborg-worktrees/task-74.4-lichess-accept-path
Base: c9bca2a7bfe33e7bd7d74358f5562512f29ba141
Implementation target: f68d429
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass on rerun (lichess 110/110; workspace all green)
Known failures: engine/tests/timed_selfplay.rs::fast_timed_self_play_never_forfeits_or_hangs failed once under the parallel full-suite run (CPU contention), then passed 3/3 in isolation and on a full-suite rerun. It is a load-sensitive timing flake in the engine crate; the diff touches only lichess/src and does not touch engine, so it is unrelated to this change.
---

author: @claude
created: 2026-07-21 13:23
---
Review verdict: APPROVE

Branch: task-74.4-lichess-accept-path
Base: c9bca2a
Implementation target (code): f68d429 — immutable; verified an ancestor of the branch tip, with only the task file changed after it.

Reviewed the full base..target diff (lichess/src/{config,error,event,policy,transport,run}.rs). Scope is confined to the lichess crate; no move-generation or search hot path is touched, so the speed benchmarks do not apply.

Acceptance criteria — all proven by pinned tests in lichess/src/run.rs / event.rs:
- #1 over-cap accept prevention: accept_reserves_a_slot_so_the_next_challenge_is_over_cap
- #2 reserve reconciled on gameStart / released on cancel/accept-fail: game_slots_reserve_start_and_free, replay "incoming human challenge is accepted and starts one game", challenge_canceled_releases_the_reserved_slot, a_404_accept_is_benign_and_frees_the_slot
- #3 challengeCanceled releases a reserved slot: challenge_canceled_releases_the_reserved_slot (+ challenge_canceled_carries_the_challenge_id)
- #4 benign 404 on accept: a_404_accept_is_benign_and_frees_the_slot; 404 maps to Error::NotFound, handled at the accept site as debug + release, not WARN/error
- #5 human-reserved slots on the acceptance side / bot held out: a_bot_is_held_out_of_a_reserved_human_slot
- #6 human preference ordering: a_human_is_accepted_ahead_of_a_bot_in_the_same_burst
- #7 all five scenarios pinned in the replay harness / dedicated tests
- #8 required checks pass (below)

Verification run on the target:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass, clean, with a fresh CARGO_TARGET_DIR
- cargo test --workspace: green on a clean pass (lichess 110/110; engine 379/379)

Pre-existing flake (not patch-introduced): engine/src/ui/tests.rs::a_last_event_id_from_a_previous_process_still_receives_current_state failed once during a contended full-suite run, then passed 3/3 in isolation and green on a clean full-workspace pass. It is a load-sensitive HTTP TestServer timing test in the engine crate; the diff touches only lichess/src, so it cannot be affected by this change.

Non-blocking observation (not a finding): a reserved slot is freed only by gameStart, challengeCanceled, or a failed accept — there is no independent expiry. The common reconnect case is healed by Lichess replaying gameStart for ongoing games; a slot could orphan only if a game's entire lifetime elapsed during a stream outage. This matches the task's chosen design and no acceptance criterion requires expiry.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Brought the Lichess challenge-accept path to reference behaviour. GameSlots (a Reserved/Active state map keyed by the shared challenge/game id) replaces the gameStart-only ActiveGames: a slot is reserved on accept, promoted in place on the matching gameStart (no double count), released on challengeCanceled or a failed accept, and removed on game finish; len() counts reserved+active so the cap sees reserved-but-not-started games. challengeCanceled is now a modeled event that frees a reserved slot; HTTP 404 on accept maps to a new benign Error::NotFound handled at the accept site (debug log, slot released) rather than a WARN. Acceptance is deferred to a short-lived queue drained once per event burst: suitable challenges are buffered, sorted humans-first when challenge.prefer_human_challenges is set, and accepted under an effective cap (bots: max_concurrent_games - matchmaking.reserved_human_slots; humans: full cap), so a configured number of slots stays reachable by humans and a bot is held out of a reserved human slot. policy::evaluate was split into cap-free policy::classify. Verified with cargo fmt --check (clean), cargo clippy --workspace --all-targets --all-features -- -D warnings (clean, fresh CARGO_TARGET_DIR), and cargo test --workspace (green on a clean pass; lichess 110/110). Pinned harness scenarios cover over-cap decline, challengeCanceled release, benign 404 accept, human-ahead-of-bot, and bot-held-out-of-reserved-slot.
<!-- SECTION:FINAL_SUMMARY:END -->
