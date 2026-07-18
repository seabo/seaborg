---
id: TASK-49
title: Correct static exchange evaluation for pawn promotions
status: To Do
assignee: []
created_date: '2026-07-18 18:30'
labels: []
dependencies: []
references:
  - engine/src/see.rs
  - 'http://www.talkchess.com/forum3/viewtopic.php?f=7&t=77787'
priority: medium
type: bug
ordinal: 49000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
SEE does not account for pawn promotions. Two cases are wrong: a pawn promoting with a capture, and a pawn promoting without a capture as the first move of the exchange sequence. In both, the material swing includes the promotion gain, which the current swap-off loop ignores, so SEE can misjudge captures involving a pawn on the 7th rank.

The test module in engine/src/see.rs already carries commented-out positions covering this, plus a note suggesting a search extension whenever a pawn is on the 7th rank as an alternative. Work through the linked discussion and decide between correcting the SEE swap loop and extending instead; the ticket is satisfied either way provided the reported values are correct and tested.

Reference: http://www.talkchess.com/forum3/viewtopic.php?f=7&t=77787
TODO site: engine/src/see.rs:134.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 SEE returns the correct value for a pawn capture that promotes
- [ ] #2 SEE returns the correct value when a non-capturing promotion is the first move of the exchange
- [ ] #3 The currently commented-out promotion positions in the see.rs test module are enabled as asserting tests
- [ ] #4 The chosen approach (corrected swap loop vs search extension) is recorded with its rationale in the implementation notes
- [ ] #5 The promotion TODO at engine/src/see.rs:134 is removed
<!-- AC:END -->
