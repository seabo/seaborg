---
id: TASK-71
title: 'Lichess bot matchmaking: issue outgoing challenges to other bots'
status: To Do
assignee: []
created_date: '2026-07-20 23:23'
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
- [ ] #1 The bot can issue outgoing challenges to other bots via the Lichess challenge API (new client method(s) alongside the existing accept/decline).
- [ ] #2 Matchmaking is opt-in and disabled by default; with it disabled the bot behaves exactly as today (reactive only).
- [ ] #3 A new matchmaking config section (loaded from the same TOML as `[challenge]`) controls at minimum: enable toggle, candidate variant/time-control/increment pools, opponent rating bounds and/or rating difference, rated/casual/random mode selection, idle-timeout before challenging, and minimum interval between challenges. Unknown keys are rejected consistently with the existing config, and invalid/inconsistent settings fail at startup with a clear error.
- [ ] #4 Opponent selection targets only eligible online bots consistent with the configured pools and rating constraints, and never challenges a bot on the configured block list.
- [ ] #5 Challenge issuance is concurrency-aware: it never starts a matchmaking game that would exceed `max_concurrent_games`, and respects any configured reservation for human challengers.
- [ ] #6 When a bot declines a matchmaking challenge, the bot avoids immediately re-issuing an equivalent challenge to that bot (decline-based backoff), per the configured filter.
- [ ] #7 The example config file (`lichess/seaborg-lichess.example.toml`) documents every new setting with its default.
- [ ] #8 Tests cover config parsing/validation of the new section and the opponent-eligibility/decline-backoff decision logic; workspace fmt, clippy (-D warnings), and tests pass.
<!-- AC:END -->
