---
id: TASK-76
title: >-
  Graceful drain mode for the lichess bot: finish current games, stop starting
  new ones
status: In Progress
assignee:
  - '@claude'
created_date: '2026-07-21 13:20'
updated_date: '2026-07-21 13:44'
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
