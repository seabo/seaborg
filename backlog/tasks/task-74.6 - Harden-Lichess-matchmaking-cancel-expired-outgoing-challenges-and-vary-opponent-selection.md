---
id: TASK-74.6
title: >-
  Lichess matchmaking robustness and challenge-policy precision, plus a
  conformance-divergence note
status: To Do
assignee: []
created_date: '2026-07-21 03:56'
updated_date: '2026-07-21 04:03'
labels:
  - lichess
  - conformance
dependencies:
  - TASK-74.1
parent_task_id: TASK-74
priority: medium
type: enhancement
ordinal: 125000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The lower-risk polish bundle. Depends on the harness (TASK-74.1).

MATCHMAKING ROBUSTNESS: (1) When an outstanding outgoing challenge times out, seaborg clears in-memory tracking (matchmaking.rs choose sets outstanding = None) and never captured the challenge id, so it cannot cancel it on Lichess. Realtime challenges auto-expire at ~20s so this is mostly masked, but a correspondence or kept-alive challenge lingers as a zombie. Reference tracks the single outstanding id and calls li.cancel on expiry. Fix: capture the created challenge id (client.create_challenge currently discards the response body) and cancel it via the cancel endpoint on expiry. (2) Opponent selection is deterministic first-eligible in online-list order (matchmaking.rs select_opponent), so it re-picks the same bot each interval until that bot declines. Reference uses weighted-random over the pool. Fix: make selection non-deterministic (weighted or rotating) while still honouring rating bounds, block list, and decline backoff; keep a seedable/injectable seam so tests can assert both variability and eligibility filtering. (3) Optional/low: skip a target that has blocked the bot (public-data blocking flag).

POLICY / DECLINE PRECISION: the decline endpoint accepts 11 reasons (generic, later, tooFast, tooSlow, timeControl, rated, casual, standard, variant, noBot, onlyBot). seaborg has 7 and collapses fast/slow to timeControl; reference uses tooFast/tooSlow, standard for variant-only bots, later for anti-spam, and maps a mode mismatch to the mode it DOES accept (a rated-only-declined challenge declines with reason casual) rather than seaborg Rated/Casual mapping. Reference also supports incoming allow_list/block_list, per-user max simultaneous games (decline later), recent-bot-challenge throttle, bullet_requires_increment, and rating_difference. Cherry-pick the high-value low-cost items (finer time-control reasons, incoming allow/block list, per-user simultaneous-game limit) and DOCUMENT the intentional divergences (idle-timeout default in seconds vs the reference minutes; deterministic-vs-random covered above) in a short living note (crate docs or lichess/REFERENCE_CONFORMANCE.md) so future work does not re-flag them as bugs.

References: lichess-bot lib/matchmaking.py (outstanding-id cancel, weighted selection, per-opponent backoff), lib/model.py (is_supported decline chain), config.yml.default; Lichess OpenAPI challengeDecline (11-reason enum), challengeCancel.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The id of an issued matchmaking challenge is captured and, when that challenge expires unanswered, a cancel request is sent for it
- [ ] #2 Opponent selection over an unchanging online pool does not repeatedly pick the same bot every interval (weighted-random or rotation), while still honouring rating bounds, block list, and decline backoff; selection is seedable/injectable for tests
- [ ] #3 A time-control decline distinguishes too-fast vs too-slow (tooFast/tooSlow) rather than always timeControl, and a mode-mismatch decline offers the alternative mode the bot accepts (or the current mapping is kept with a documented rationale)
- [ ] #4 Incoming challenges honour an allow_list/block_list and a per-user simultaneous-game limit (decline reason later), matching the config surface added here
- [ ] #5 A short conformance note records each intentional divergence from the reference (one-line rationale each) so they are not re-flagged as bugs
- [ ] #6 Pinned tests cover: an expired outstanding challenge triggering a cancel with the tracked id, selection spreading across eligible candidates, the new decline-reason mappings, and the block/allow and per-user-limit paths
- [ ] #7 cargo fmt --check, cargo clippy --workspace --all-targets --all-features -D warnings, and cargo test --workspace all pass
<!-- AC:END -->
