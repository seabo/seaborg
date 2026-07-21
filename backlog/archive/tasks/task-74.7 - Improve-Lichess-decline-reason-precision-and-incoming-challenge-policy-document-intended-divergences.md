---
id: TASK-74.7
title: >-
  Improve Lichess decline-reason precision and incoming-challenge policy;
  document intended divergences
status: To Do
assignee: []
created_date: '2026-07-21 03:56'
updated_date: '2026-07-21 03:57'
labels:
  - lichess
  - conformance
dependencies:
  - TASK-74.1
parent_task_id: TASK-74
priority: low
type: enhancement
ordinal: 126000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Bring the incoming-challenge policy (lichess/src/policy.rs) closer to the reference where it improves challenger experience and anti-spam, and record the divergences seaborg keeps on purpose.

Reference/spec facts: the decline endpoint accepts 11 reasons (generic, later, tooFast, tooSlow, timeControl, rated, casual, standard, variant, noBot, onlyBot). lichess-bot uses tooFast/tooSlow where seaborg collapses to timeControl, uses standard for variant-only bots, later for anti-spam limits, and maps a mode mismatch to the mode it DOES accept (a rated-only-declined challenge is declined with reason casual, i.e. offer the alternative) rather than seaborg current Rated/Casual mapping. It also supports: per-user max simultaneous games (decline later), recent-bot-challenge throttle (decline later), an incoming allow_list/block_list, bullet_requires_increment for bot bullet, and accept-within-own-rating +/- N (rating_difference).

This task should cherry-pick the high-value, low-cost items and explicitly DOCUMENT the ones seaborg intentionally does differently (e.g. idle-timeout default in seconds vs the reference minutes; deterministic-vs-random already covered elsewhere). Capture the accepted divergences as a short living note (in the crate docs or a lichess/REFERENCE_CONFORMANCE.md) so future work does not rediscover them as bugs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A time-control decline distinguishes too-fast vs too-slow (tooFast/tooSlow) rather than always timeControl, and a variant-only decline can use standard where appropriate
- [ ] #2 A mode-mismatch decline offers the alternative mode the bot accepts (reference inverted mapping), or the current mapping is kept with a documented rationale
- [ ] #3 Incoming challenges honour an allow_list/block_list and a per-user simultaneous-game limit (decline reason later), matching the config surface added here
- [ ] #4 A short conformance note records each intentional divergence from the reference (with a one-line rationale each) so they are not re-flagged as bugs
- [ ] #5 Pinned tests cover the new decline-reason mappings and the block/allow and per-user-limit paths
- [ ] #6 cargo fmt --check, cargo clippy -D warnings, and cargo test --workspace all pass
<!-- AC:END -->
