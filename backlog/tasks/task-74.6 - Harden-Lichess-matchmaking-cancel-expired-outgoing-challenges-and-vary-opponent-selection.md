---
id: TASK-74.6
title: >-
  Lichess matchmaking robustness and challenge-policy precision, plus a
  conformance-divergence note
status: In Review
assignee:
  - '@claude'
created_date: '2026-07-21 03:56'
updated_date: '2026-07-21 15:44'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. client.rs: create_challenge parses the create response and returns the challenge id; add cancel_challenge (POST /api/challenge/{id}/cancel).
2. matchmaking.rs: Outstanding stores the challenge id; record_issued(now, id); choose() stashes the id of an abandoned/expired outstanding challenge; take_challenge_to_cancel() drains it. Replace first-eligible select_opponent with random selection over eligible candidates using an internal SplitMix64 rng; add with_seed builder as the injectable seam (production seeds from system entropy; new() uses a fixed seed for deterministic tests).
3. run.rs seek_matchmaking_game: capture the created id via record_issued; after choose, cancel any abandoned challenge (outside the lock). GameSlots tracks a per-slot owner id and exposes games_for_user; process_accept_queue enforces max_games_per_user (decline reason later) and passes the challenger id as slot owner.
4. policy.rs: add DeclineReason TooFast/TooSlow/Later/Standard (11 total). Time control too-fast vs too-slow; standard vs variant reason; mode mismatch offers the alternative mode (rated->casual, casual->rated). allow_list/block_list matched by account id (case-insensitive) -> decline generic.
5. config.rs: ChallengePolicy gains allow_list, block_list, max_games_per_user.
6. lichess/REFERENCE_CONFORMANCE.md: record intentional divergences (idle-timeout seconds vs reference minutes, uniform-random vs rating-weighted selection, id-based case-insensitive list matching, unimplemented reference knobs).
7. seaborg-lichess.example.toml: document the new challenge fields.
8. Add pinned tests (expired->cancel, selection spread, new decline mappings, allow/block + per-user-limit) and run fmt/clippy/test.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented in the lichess crate:
- client.rs: create_challenge now returns the created challenge id (parsed from the create response); added cancel_challenge (POST /api/challenge/{id}/cancel).
- matchmaking.rs: Outstanding tracks the challenge id; record_issued takes it; choose() abandons a lapsed outstanding challenge (before the cap check, so a full board does not delay cancellation) and stashes its id for take_challenge_to_cancel(). Opponent selection is uniform-random over the eligible pool via an internal SplitMix64 PRNG; with_seed is the injectable seam (production seeds from system entropy; new() uses a fixed seed for deterministic tests).
- run.rs: seek_matchmaking_game captures the created id via record_issued and cancels an abandoned challenge outside the matchmaker lock. GameSlots stores a per-slot owner and exposes games_for_user; process_accept_queue enforces max_games_per_user (decline reason later) before the capacity check and passes the challenger id as the slot owner.
- policy.rs: DeclineReason extended to the 11 Lichess reasons (added Later, Standard, TooFast, TooSlow). classify checks block_list/allow_list first (by account id, case-insensitive, decline generic); splits clock declines into tooFast/tooSlow; reports standard vs variant; a mode mismatch offers the accepted mode (rated->casual, casual->rated).
- config.rs: ChallengePolicy gains allow_list, block_list, max_games_per_user.
- lichess/REFERENCE_CONFORMANCE.md records intentional divergences; example TOML documents the new fields.

Tests: new/updated coverage for expired-outstanding cancel by tracked id (unit + integration, incl. full-cap case), selection spread + seed injectability, the new decline mappings, allow/block via classify and the event loop, and the per-account-limit path. Existing selection/matchmaking tests updated for random selection (order-independent assertions).
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 15:44
---
Implementation handoff
Branch: task-74.6-lichess-matchmaking-robustness
Worktree: /Users/seabo/seaborg-worktrees/task-74.6-lichess-matchmaking-robustness
Base: a5e52e604b0db0d87346785b1052a9bd268ac937
Implementation target: ac87e0f263f59f3f98da0d877d81279387f9da64
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (exit 0)
- cargo test --workspace: pass (exit 0; lichess 131 tests green)
Known failures: none
---
<!-- COMMENTS:END -->
