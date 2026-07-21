---
id: TASK-72
title: >-
  Fix Lichess matchmaking: surface HTTP error bodies, unjam on challenge
  failure, safer default mode
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-21 02:05'
updated_date: '2026-07-21 02:22'
labels: []
dependencies: []
type: bug
ordinal: 117000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Proactive matchmaking (TASK-71) fails in practice: an outgoing rated challenge to a bot returns HTTP 400 and the bot re-challenges the same opponent forever. Three defects compound. (1) The transport discards the HTTP response body: `check_status` in lichess/src/transport.rs maps any unhandled status to `Error::Http("unexpected status {code}")` without reading the body, so the JSON `{"error":"..."}` Lichess returns on a 400 (the only thing that explains the failure) never reaches the log. (2) A create-challenge failure records no per-opponent penalty: `maybe_seek_matchmaking_game` in lichess/src/run.rs logs and returns on a recoverable error, but `select_opponent` deterministically returns the first eligible online bot, so matchmaking re-picks the same unreachable bot every interval and wedges. The decline backoff only fires on a `challengeDeclined` event, never on a create-time HTTP failure. (3) The bundled example config default is `[matchmaking] mode = "random"`, which issues rated challenges on half its ticks; rated bot-to-bot challenges are commonly rejected at creation (e.g. the Maia bots only accept casual), so enabling matchmaking with the shipped default hits repeated hard failures with no explanation. These slipped through because the test FakeTransport `post_form` unconditionally returns Ok, so no test exercises a failed challenge, and there is no live/integration coverage of the challenge contract.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 On a non-success HTTP status other than 401/429, the transport reads the response body and includes it in the surfaced error (Error::Http), so a 400 from Lichess logs the server-provided reason rather than only "unexpected status 400"; a unit test proves the body text reaches the error
- [x] #2 When an outgoing matchmaking challenge fails at create time (recoverable HTTP error), the matchmaker applies a per-opponent penalty so the same bot is not re-selected on the immediately following attempts; a test drives a failing create_challenge and asserts the next attempt targets a different eligible bot (or none) rather than re-challenging the same one
- [x] #3 cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, and cargo test --workspace all pass
- [x] #4 The bundled example config makes the rated-challenge rejection risk explicit: the [matchmaking] mode comment documents that rated challenges to bots are frequently rejected at creation, and the default mode is chosen so enabling matchmaking out of the box does not produce guaranteed repeated create-time rejections
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. transport.rs (AC#1): in check_status, on an unhandled non-success status read the response body and fold it into Error::Http. Extract a pure helper unexpected_status_error(status, body) that appends a trimmed, length-capped body when present, and unit-test it directly (body text reaches the error) without needing a live socket.
2. matchmaking.rs (AC#2): add record_challenge_failed(bot_id, now) that applies the same per-opponent backoff as a decline (share a private record_backoff helper with record_declined). Unit-test that after recording a failure, select_opponent skips that bot and returns the next eligible one.
3. run.rs (AC#2): in maybe_seek_matchmaking_game, on a recoverable create_challenge failure call matchmaker.record_challenge_failed(&target, now) so the wedged bot is skipped next tick. Extend FakeTransport to fail challenge-create POSTs and add a test driving two seek ticks (idle_timeout=0, min_interval=0) asserting the second attempt targets a different bot.
4. config example + code default (AC#4): change the built-in matchmaking mode default from Random to Casual in config.rs (the example file header promises it shows built-in defaults, so both must agree), update the example toml mode value and comment to explain rated challenges to bots are frequently rejected at creation, and update the config.rs default-mode test.
5. Run cargo fmt --check, clippy -D warnings, and cargo test --workspace (AC#3).
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented all three defects on this branch.

- AC#1 (transport): check_status now reads the response body on an unhandled non-success status and folds a trimmed, 500-char-capped snippet into Error::Http through a new pure helper unexpected_status_error(status, body). Unit-tested directly (body reaches the error, empty body omitted, oversized body capped) so no live socket is needed.
- AC#2 (matchmaking + run): added Matchmaker::record_challenge_failed, which applies the same per-opponent backoff as a decline (shared start_backoff helper). run.rs maybe_seek_matchmaking_game calls it on a recoverable create failure. Covered by a matchmaking unit test (selection moves to the next bot after a failure, and re-eligible after the backoff) and a run.rs integration test using a FakeTransport that fails challenge-create POSTs, asserting the second seek targets a different bot.
- AC#4 (config): changed the built-in matchmaking mode default from Random to Casual and the example toml to match (the example header promises it mirrors built-in defaults), with a comment in both explaining rated challenges to bots are frequently rejected at creation. Updated the config default-mode test.

Decision: reused the existing decline-backoff window (decline_backoff_seconds) for create failures rather than adding a separate knob — a create-time rejection is as good a reason to skip a bot as a decline, and a second timing knob was not warranted.

Implementation handoff
Branch: task-72-lichess-matchmaking-fixes
Worktree: /Users/seabo/seaborg-worktrees/task-72-lichess-matchmaking-fixes
Base: 27a19b4
Implementation target: 5e38d3f
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass
- cargo test --workspace: pass (exit code 0); cargo test -p lichess = 98 passed
Known failures: none
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-21 02:14
---
Ready for independent review. Implementation target 5e38d3f on task-72-lichess-matchmaking-fixes; all repo-required checks pass. AC checks and final summary left for the reviewer.
---

author: @claude
created: 2026-07-21 02:22
---
Review attempt: 1
Reviewed branch: task-72-lichess-matchmaking-fixes
Reviewed implementation: 5e38d3f
Verdict: approved

All four acceptance criteria proven against the immutable base(27a19b4)-to-target(5e38d3f) diff; commits after the target contain only handoff metadata; worktree clean.

AC#1: unexpected_status_error folds the response body into Error::Http for unhandled non-success statuses; 2xx returns early and 401/429 stay typed. Tests: transport::tests::unexpected_status_error_{includes_the_response_body,omits_an_empty_body,caps_a_huge_body}.
AC#2: record_challenge_failed applies the shared per-opponent backoff; run.rs calls it on a recoverable create failure (Error::Http is_recoverable). Tests: matchmaking::tests::a_failed_challenge_makes_selection_move_to_the_next_bot; run::tests::a_failed_challenge_moves_matchmaking_to_a_different_bot (asserts posts firstbot then secondbot).
AC#3/AC#4: config default and example toml both Casual with rejection-risk comment; default-mode test updated.

Verification (run in worktree on target-equivalent tip; only the task md differs from 5e38d3f):
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (fresh CARGO_TARGET_DIR, exit 0)
- cargo test --workspace: pass (exit 0); lichess 98 passed incl. 5 new tests

No new #[allow], no unrelated changes, comments are self-contained.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed three compounding Lichess matchmaking defects. (1) transport.rs check_status now reads the response body on an unhandled non-success status (2xx returns early; 401/429 still typed) and folds a trimmed, 500-char-capped snippet into Error::Http via the pure helper unexpected_status_error, so a 400 logs Lichess's reason; three unit tests prove body-reaches-error, empty-body-omitted, oversized-body-capped. (2) matchmaking.rs adds record_challenge_failed applying the shared per-opponent backoff, and run.rs maybe_seek_matchmaking_game calls it on a recoverable create failure, so the deterministic first-eligible selection no longer re-picks the wedged bot; a matchmaking unit test plus a run.rs integration test (FakeTransport failing challenge-create POST) prove the next attempt targets a different bot. (3) config.rs default mode and the example toml both changed Random->Casual with a comment that rated challenges to bots are frequently rejected at creation; default-mode test updated. Verified on implementation target 5e38d3f: cargo fmt --check pass; cargo clippy --workspace --all-targets --all-features -- -D warnings pass (clean CARGO_TARGET_DIR, exit 0); cargo test --workspace pass (exit 0, 5 new tests green).
<!-- SECTION:FINAL_SUMMARY:END -->
