---
id: TASK-72
title: >-
  Fix Lichess matchmaking: surface HTTP error bodies, unjam on challenge
  failure, safer default mode
status: To Do
assignee: []
created_date: '2026-07-21 02:05'
updated_date: '2026-07-21 02:05'
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
- [ ] #1 On a non-success HTTP status other than 401/429, the transport reads the response body and includes it in the surfaced error (Error::Http), so a 400 from Lichess logs the server-provided reason rather than only "unexpected status 400"; a unit test proves the body text reaches the error
- [ ] #2 When an outgoing matchmaking challenge fails at create time (recoverable HTTP error), the matchmaker applies a per-opponent penalty so the same bot is not re-selected on the immediately following attempts; a test drives a failing create_challenge and asserts the next attempt targets a different eligible bot (or none) rather than re-challenging the same one
- [ ] #3 cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings, and cargo test --workspace all pass
- [ ] #4 The bundled example config makes the rated-challenge rejection risk explicit: the [matchmaking] mode comment documents that rated challenges to bots are frequently rejected at creation, and the default mode is chosen so enabling matchmaking out of the box does not produce guaranteed repeated create-time rejections
<!-- AC:END -->
