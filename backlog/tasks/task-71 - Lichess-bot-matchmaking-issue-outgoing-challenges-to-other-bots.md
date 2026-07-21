---
id: TASK-71
title: 'Lichess bot matchmaking: issue outgoing challenges to other bots'
status: Done
assignee:
  - '@george'
created_date: '2026-07-20 23:23'
updated_date: '2026-07-21 00:52'
labels: []
dependencies: []
references:
  - 'https://github.com/lichess-bot-devs/lichess-bot'
ordinal: 116000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The lichess subcommand is currently purely reactive: it loads `seaborg-lichess.toml` and filters incoming challenges via the `[challenge]` policy (`ChallengePolicy` in `lichess/src/config.rs`), but it can never seek games itself. `lichess/src/client.rs` exposes only `accept_challenge`/`decline_challenge`; there is no outgoing challenge issuance, no idle trigger, and no config to describe who to challenge.

Add a matchmaking subsystem so the bot can proactively challenge other bots when otherwise idle, mirroring the `matchmaking` block of the lichess-bot Python reference (https://github.com/lichess-bot-devs/lichess-bot). This closes the one substantial gap between our config surface and the reference implementation.

Scope: config-driven opponent selection and challenge creation against the Lichess API, gated behind an explicit opt-in so existing reactive behaviour is unchanged by default. Whether this ships as one task or is split into subtasks (config, opponent selection, challenge issuance, decline-tracking/backoff) is for the implementer to decide when the task is picked up.

Reference behaviour worth mirroring from the Python config: opt-in `allow_matchmaking`; challenge over pooled variant/time-control/increment choices; opponent rating targeting (min/max and rating-difference); rated/casual/random mode; an idle timeout before a challenge is issued; a minimum gap between challenges; concurrency-aware issuance (do not exceed max concurrent games and optionally reserve slots for humans); a block list of bots never to challenge; and a decline filter that suppresses re-challenging a bot that just declined (coarse/fine).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The bot can issue outgoing challenges to other bots via the Lichess challenge API (new client method(s) alongside the existing accept/decline).
- [x] #2 Matchmaking is opt-in and disabled by default; with it disabled the bot behaves exactly as today (reactive only).
- [x] #3 A new matchmaking config section (loaded from the same TOML as `[challenge]`) controls at minimum: enable toggle, candidate variant/time-control/increment pools, opponent rating bounds and/or rating difference, rated/casual/random mode selection, idle-timeout before challenging, and minimum interval between challenges. Unknown keys are rejected consistently with the existing config, and invalid/inconsistent settings fail at startup with a clear error.
- [x] #4 Opponent selection targets only eligible online bots consistent with the configured pools and rating constraints, and never challenges a bot on the configured block list.
- [x] #5 Challenge issuance is concurrency-aware: it never starts a matchmaking game that would exceed `max_concurrent_games`, and respects any configured reservation for human challengers.
- [x] #6 When a bot declines a matchmaking challenge, the bot avoids immediately re-issuing an equivalent challenge to that bot (decline-based backoff), per the configured filter.
- [x] #7 The example config file (`lichess/seaborg-lichess.example.toml`) documents every new setting with its default.
- [x] #8 Tests cover config parsing/validation of the new section and the opponent-eligibility/decline-backoff decision logic; workspace fmt, clippy (-D warnings), and tests pass.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. config.rs: add opt-in [matchmaking] section (enabled=false default) with variant/initial/increment pools, min/max rating bounds, mode (rated|casual|random), idle_timeout, min interval, reserved human slots, block_list, decline_backoff. deny_unknown_fields + validation (pools non-empty when enabled, rating bounds ordered, mode valid, reserved<max_concurrent). Byte-identical default behaviour (AC#2/#3).
2. matchmaking.rs (new, pure/testable like policy.rs): BotInfo + perfs parsing, speed classification from (initial,increment), Matchmaker state (last_issued, idle_since, outstanding challenge, per-bot decline backoff, rotation counters). Pure methods: choose(now, active_games)->Idle|Seek (idle-timeout, min-interval, concurrency+human-reservation gating), select_opponent(candidates, spec) filtering by pool/rating/blocklist/decline-backoff, record_issued/declined/game_started/game_finished.
3. client.rs: online_bots(nb) via GET /api/bot/online (NDJSON), create_challenge(user, spec) via POST /api/challenge/{user}.
4. event.rs: add ChallengeDeclined variant capturing destUser id (was swallowed by Other), for decline backoff.
5. run.rs: thread Matchmaker (disabled=inert when off) into event loop + handle_event; tick on keepalive/after events using Instant::now; feed gameStart/gameFinish/challengeDeclined to matchmaker state. Default-config path unchanged.
6. example toml + docs for every new key (AC#7).
7. Tests: config parse/validate, speed classify, opponent eligibility, decline backoff, idle/interval/concurrency gating; fmt+clippy(-D)+tests (AC#8). NOTE: rating targeting via min/max bounds only (AC#3 'and/or'); rating-difference deliberately omitted to avoid fragile self-rating/speed machinery.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented opt-in matchmaking as one modular slice (per implementer split decision). All decision logic is pure and unit-tested in matchmaking.rs (mirroring policy.rs); the event-loop change is a single idle tick at the existing keepalive/event boundary, so the loop's blocking model is unchanged and the reactive-only path is byte-for-byte identical when matchmaking is disabled (verified by disabled_matchmaking_issues_no_challenge_on_a_keepalive).

Design notes for review:
- reserved_human_slots is meaningful: matchmaking's cap is max_concurrent_games - reserved_human_slots, so it can stack games up to that reduced cap while leaving room for humans. (An earlier draft that blocked on any active game made the knob dead; fixed.)
- idle timeout is measured from the last game START (record_game_started), not continuously, so multiple matchmaking games can stack without dogpiling.
- Rating targeting is by min/max bounds on the opponent's rating for the chosen time control's speed (AC#3 'and/or'). rating_difference was deliberately NOT added: it needs our own per-speed rating and reliable speed classification of our side, which is fragile machinery I did not want to half-ship. Flagging for reviewer to decide if a follow-up is wanted.
- A candidate with no rating for the chosen speed is skipped (cannot confirm bounds).
- Decline backoff is per-bot and time-based (decline_backoff_seconds); the map is pruned on record.
- Pool/mode selection is deterministic (rotating cursors, alternating rated/casual) to keep tests reproducible without an rng dependency.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @george
created: 2026-07-20 23:59
---
Implementation handoff
Branch: task-71-lichess-matchmaking
Worktree: /Users/seabo/seaborg-worktrees/task-71-lichess-matchmaking
Base: 8674a8c582f063af71cd3a4c7ea79904685cc774
Implementation target: 0c9f192fb77267757237ed218b33951a7da0ca6b
Resolved findings: none (new work)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (exit 0, no warnings)
- cargo test --workspace: pass (8 suites ok, 0 failed; lichess 93 tests)
Known failures: none
---

author: @george
created: 2026-07-21 00:13
---
Review attempt: 1
Reviewed branch: task-71-lichess-matchmaking
Reviewed implementation: 0c9f192fb77267757237ed218b33951a7da0ca6b
Verdict: approved

Scope/immutability: base 8674a8c is an ancestor of target 0c9f192, which is an ancestor of tip 0009371. The only commit after the target touches this task file (handoff metadata); no implementation file changed between target and approval.

Acceptance criteria (all proven):
- #1 client.online_bots + client.create_challenge issue outgoing challenges alongside accept/decline.
- #2 enabled=false by default; maybe_seek_matchmaking_game and Matchmaker::choose short-circuit when disabled; reactive path unchanged (disabled_matchmaking_issues_no_challenge_on_a_keepalive).
- #3 [matchmaking] section with enable/pools/rating bounds/mode/idle-timeout/min-interval; deny_unknown_fields + validate() reject unknown keys, empty enabled pools, inverted bounds, and full human reservation (config tests).
- #4 select_opponent filters to online BOT accounts within rating bounds, excluding self and block list (selects_first_eligible_bot_within_rating_bounds, selection_skips_self_block_list_and_non_bots, a_candidate_without_a_rating_for_the_speed_is_skipped).
- #5 choose gates on active_games >= max_concurrent_games - reserved_human_slots (reserved_human_slots_hold_matchmaking_below_the_cap, full-cap test).
- #6 record_declined starts per-bot time-based backoff wired from Event::ChallengeDeclined (a_declined_bot_is_skipped_until_the_backoff_elapses, challenge_declined_carries_the_declining_bot).
- #7 seaborg-lichess.example.toml documents every new key with its default.
- #8 config + decision-logic tests present; fmt/clippy(-D)/test all pass.

Verification (on 0c9f192):
- cargo fmt --check: pass (exit 0)
- cargo clippy --workspace --all-targets --all-features -- -D warnings (clean CARGO_TARGET_DIR): pass, no warnings
- cargo test --workspace: pass (exit 0; lichess 93 tests; 0 failed across all suites)
Diff is confined to the lichess crate (no movegen/search hot paths), so perft/movegen benchmarks are not applicable.

Non-blocking observation (not a gate): the doc comment on Matchmaker::record_issued refers to a `target` parameter that no longer exists (the method takes only `now`). Cosmetic; can be tidied on any future touch of this file.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Adds opt-in proactive matchmaking to the lichess bot. New pure/tested Matchmaker (lichess/src/matchmaking.rs) owns all timing, opponent-eligibility, and decline-backoff decisions; client gains online_bots (GET /api/bot/online) and create_challenge (POST /api/challenge/{user}); event.rs adds a ChallengeDeclined variant; run.rs ticks the matchmaker on every event/keepalive and is inert when disabled. Verified on implementation target 0c9f192: cargo fmt --check (pass), cargo clippy --workspace --all-targets --all-features -- -D warnings with a clean CARGO_TARGET_DIR (pass, no warnings), cargo test --workspace (pass; lichess 93 tests, all suites 0 failed). Matchmaking is disabled by default and the reactive-only path is unchanged (disabled_matchmaking_issues_no_challenge_on_a_keepalive).
<!-- SECTION:FINAL_SUMMARY:END -->
