---
id: TASK-76
title: >-
  Graceful drain mode for the lichess bot: finish current games, stop starting
  new ones
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-21 13:20'
updated_date: '2026-07-21 13:53'
labels:
  - lichess
dependencies: []
type: enhancement
ordinal: 129000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Operators running the lichess subcommand under a matchmaking config currently have no way to stop the bot cleanly between games. The only shutdown path (SIGINT/SIGTERM) trips a single flag that immediately resigns in-flight games, so an operator must either wait for a game to end or forfeit it. Because matchmaking re-seeks within roughly a second of a slot freeing up, there is only a brief window to intervene manually after a game finishes before the next one starts automatically.

This task adds a graceful "drain" stage: enter drain on the first interrupt (stop seeking new matchmaking games while letting all in-flight games play to completion), and escalate to the existing immediate shutdown on a second interrupt. Once draining and all active games have finished, the process exits cleanly with no forfeits.

The intended integration points already exist: the signal handler and shutdown atomic in lichess/src/shutdown.rs, the enabled-gated seek path in seek_matchmaking_game / Matchmaker::choose (lichess/src/run.rs, lichess/src/matchmaking.rs), and worker-driven removal from ActiveGames on game exit (lichess/src/run.rs) which makes drain-to-zero detection free. The worker itself should need no behavioral change while draining.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 First SIGINT/SIGTERM enters drain mode instead of resigning: in-flight games continue to completion and no new matchmaking game is sought
- [ ] #2 A second SIGINT/SIGTERM while draining performs the existing immediate shutdown (in-flight games resign, threads join promptly)
- [ ] #3 While draining, the matchmaker does not seek or issue new challenges, and no new incoming challenge is accepted into a new game
- [ ] #4 When drain mode is active and the active-game count reaches zero, the process shuts down and exits cleanly with no forfeits
- [ ] #5 Entering drain mode logs a clear operator-facing message stating how many active games remain and that a second interrupt quits immediately
- [ ] #6 Tests cover the state transitions: normal -> drain -> immediate shutdown, and drain -> auto-exit when active games reach zero
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. shutdown.rs: replace the single AtomicBool with a three-value stage machine (Running/Draining/ShuttingDown) backed by an AtomicU8. Keep is_requested()==ShuttingDown so workers/reader/transport are unchanged. Add is_draining() (>=Draining) and begin_drain() (Running->Draining only). request() sets ShuttingDown. The signal handler escalates one stage per interrupt: first SIGINT/SIGTERM enters Draining, any later one goes to ShuttingDown.
2. run.rs matchmaking: stop seeking while draining (run_matchmaking exits its loop on is_draining; seek_matchmaking_game guards early) so no new outgoing challenge is issued.
3. run.rs consumer: keep the reader/consumer running during drain so in-flight games play out; announce drain once with the remaining active-game count and the second-interrupt note; when draining and slots reach zero, escalate to full shutdown and exit cleanly. Pass shutdown into handle_event; decline any incoming challenge while draining instead of accepting it into a new game (GameStart/GameFinish still handled). Add GameSlots::is_empty().
4. Tests: stage transitions in shutdown.rs (normal->drain->immediate, begin_drain idempotency); consumer drain auto-exit at zero and drain->immediate escalation; draining declines an incoming challenge; matchmaker seeks nothing while draining.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented graceful drain across shutdown.rs and run.rs.

shutdown.rs: replaced the single AtomicBool with a three-value ordered stage
(Running/Draining/ShuttingDown) backed by an AtomicU8. is_requested() now means
ShuttingDown only, preserving the existing "resign and stop" contract for game
workers, the event reader, and the transport (all unchanged). Added is_draining()
(>= Draining), begin_drain() (Running->Draining via compare_exchange, returns
whether it transitioned so the drain is announced once), request() (escalates to
ShuttingDown from any stage), and stage(). The SIGINT/SIGTERM handler escalates one
stage per interrupt: first drains, any later one shuts down now (async-signal-safe
load-then-store).

run.rs:
- run_matchmaking exits its loop on is_draining(); seek_matchmaking_game also
  early-returns when draining, closing the mid-poll race, so no outgoing challenge
  is issued while draining (AC#3).
- run_event_consumer keeps running through a drain so in-flight games play out. It
  announces the drain once (drain_message: remaining count + second-interrupt hint,
  AC#5), and when draining with zero held slots it calls request() and breaks,
  giving a clean exit (AC#4). Drain-to-zero is observed via GameSlots, which each
  worker frees on exit, so it does not depend on the event stream being up.
- handle_event now takes the shutdown handle; while draining it declines every
  incoming challenge (Generic) after the existing self-challenge guard, rather than
  buffering it for acceptance (AC#3). GameStart/GameFinish are still handled, so a
  game accepted just before the interrupt still starts and completes.
- Added GameSlots::is_empty().

Behavior mapping: AC#1 first interrupt -> Draining (no resign, no new seek);
AC#2 second interrupt -> request()/ShuttingDown -> workers resign, threads join;
AC#3 matchmaker + incoming challenges suppressed while draining; AC#4 drain-to-zero
auto-exit; AC#5 operator log; AC#6 tests below.

Tests added:
- shutdown.rs: starts_running_and_requests_shut_down, drain_then_immediate_shutdown,
  begin_drain_does_not_pull_back_an_immediate_shutdown, begin_drain_is_idempotent,
  clones_share_the_same_flag (extended).
- run.rs: draining_with_a_game_still_running_keeps_the_consumer_alive_until_it_ends
  (drain -> auto-exit at zero), a_second_interrupt_while_draining_shuts_down_immediately
  (normal -> drain -> immediate), draining_declines_an_incoming_challenge_instead_of_accepting_it,
  draining_matchmaker_seeks_no_new_game, drain_message_states_the_count_and_the_second_interrupt.
  A SilentTransport (Send+Sync) backs the consumer-thread tests since FakeTransport
  is !Send.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 13:53
---
Implementation handoff
Branch: task-76-lichess-drain-mode
Worktree: /Users/seabo/seaborg-worktrees/task-76-lichess-drain-mode
Base: 645fa9cb05ca80d366dc1c4e92b3d7b1c42cef3a
Implementation target: 9e6de21
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (0 warnings)
- cargo test --workspace: pass (all suites green; lichess 118 tests)
Known failures: none
---
<!-- COMMENTS:END -->
