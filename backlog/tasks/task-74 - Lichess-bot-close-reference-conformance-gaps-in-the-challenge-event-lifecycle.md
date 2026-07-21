---
id: TASK-74
title: 'Lichess bot: close reference-conformance gaps in the challenge/event lifecycle'
status: Done
assignee: []
created_date: '2026-07-21 03:54'
updated_date: '2026-07-21 20:44'
labels:
  - lichess
  - conformance
dependencies: []
priority: high
type: chore
ordinal: 119000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Umbrella for a one-time, deliberate sweep of the Lichess bot event/challenge/matchmaking behaviour against the reference implementation (lichess-bot, github.com/lichess-bot-devs/lichess-bot) and the authoritative Lichess OpenAPI spec (github.com/lichess-org/api).

Motivation: matchmaking has been fixed reactively across TASK-71/72/73 and a fourth live bug (the bot accepting its own outgoing challenges), each time rediscovering a case the reference already handles. The recurrences all cluster in one subsystem (the challenge/event lifecycle), so this sweep replaces reactive patching with a single gap-analysis-driven pass whose results are pinned as tests.

Scope covers only the risky surface: account event-stream dispatch, the incoming-challenge accept/decline decision, concurrency-cap accounting, and matchmaking cadence/selection. Out of scope: chat, opening books, online move sources, tablebase probing, correspondence play, and the reference config schema wholesale — seaborg keeps its own architecture (typed events, per-game threads, TOML config).

CONFORMANCE LEDGER (reference behaviour -> seaborg status). Each child task pins its rows as tests.

Event dispatch / challenge lifecycle:
- from_self: ref ignores a challenge whose challenger is the bot itself; seaborg accepts it -> 404. STATUS: MISSING (live bug). -> child: from_self.
- Concurrency slot reserved at accept-time: ref inserts into active_games on accept, before gameStart; seaborg counts only on gameStart, so it can over-accept past max_concurrent_games. STATUS: MISSING. -> child: cap accounting.
- challengeCanceled: ref frees the reserved slot; seaborg drops it as Other. STATUS: MISSING. -> child: cap accounting.
- 404 on accept = challenge gone (spec: accepted/declined/canceled/expired): should be benign, seaborg logs WARN. STATUS: PARTIAL. -> child: cap accounting.
- Accept queue + human/bot ordering + reserved-for-humans on the accept side: seaborg accepts inline FIFO, human reservation only in matchmaking cap. STATUS: MISSING. -> child: human-priority.
- Event ingestion isolated from blocking HTTP: ref reads the stream in a separate process feeding a queue, so a 429 backoff during accept/matchmaking cannot stall ingestion; seaborg reads + does matchmaking HTTP on one thread, so a challenge-create 429 (60-600s) stalls incoming-challenge handling -> the observed UI hang. STATUS: MISSING (architecture). -> child: decouple ingestion.
- Event stream replays all current challenges AND games on connect (spec-confirmed): seaborg reliance on gameStart replay is fine; dedup already present. STATUS: OK (no action).
- Ping-driven maintenance each keepalive: seaborg already seeks matchmaking on every event and Ok(None) keepalive. STATUS: OK.

Decline-reason / policy precision:
- 11 decline reasons (generic, later, tooFast, tooSlow, timeControl, rated, casual, standard, variant, noBot, onlyBot); seaborg has 7, uses timeControl for fast/slow, and maps a rated-declined challenge to Rated rather than the reference inverted mapping (offer the mode you DO accept). STATUS: PARTIAL. -> child: policy precision.
- Incoming allow_list / block_list, per-user simultaneous-game limit, recent-bot-challenge throttle, bullet_requires_increment, rating_difference (accept within own rating +/- N): seaborg has none. STATUS: MISSING. -> child: policy precision.

Matchmaking:
- Cancel expired outstanding challenge via API: ref tracks the challenge id and cancels on expiry; seaborg drops the tracking, never captures the id, leaves a possible zombie (mostly masked for realtime by Lichess 20s auto-expiry, real for correspondence). STATUS: MISSING. -> child: matchmaking robustness.
- Opponent selection: ref weighted-random over the online pool; seaborg deterministic first-eligible, can hammer one bot until it declines. STATUS: DIVERGENT. -> child: matchmaking robustness.
- Back off opponent on any create-failure (not just decline): seaborg already does via record_challenge_failed. STATUS: OK.
- Check whether the target has blocked us before challenging: seaborg none. STATUS: MISSING (low). -> child: matchmaking robustness.
- Idle timeout units: ref default is 30 MINUTES idle; seaborg default 30 SECONDS -> very frequent challenges. STATUS: intentional-tuning (document). -> child: policy precision / docs.

References: lichess-bot lib/lichess_bot.py, lib/model.py, lib/matchmaking.py, config.yml.default; Lichess OpenAPI doc/specs (apiStreamEvent, ChallengeJson direction enum in/out [OPTIONAL], challengeAccept 404, challengeDecline reason enum).
<!-- SECTION:DESCRIPTION:END -->
