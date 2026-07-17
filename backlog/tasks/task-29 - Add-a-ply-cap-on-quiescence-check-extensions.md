---
id: TASK-29
title: Add a ply cap on quiescence check extensions
status: To Do
assignee: []
created_date: '2026-07-17 20:29'
labels:
  - search
  - performance
dependencies: []
ordinal: 32000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Follow-up from the TASK-9 review (task-9-quiescence-semantics).

quiesce() takes no depth/ply argument, so the new check-evasion recursion is bounded only by the draw rules (threefold + fifty-move clock). In check-heavy positions this searches ALL legal evasions (quiet king moves, blocks) as full-window q-nodes, each triggering a full evaluate() plus capture search, potentially many plies deep.

Termination is guaranteed, but the node explosion is a time-management risk. Many engines cap check extensions in quiescence (first ply only, or a bounded ply counter).

Investigate the practical node-count/time impact in check-heavy positions and add a ply cap or check-extension limit to quiescence if warranted.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The node-count / time impact of unbounded quiescence check extensions is measured in representative check-heavy positions
- [ ] #2 A ply cap or check-extension limit is added to quiescence, or a decision to leave it unbounded is recorded with rationale
<!-- AC:END -->
