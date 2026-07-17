---
id: TASK-1.1
title: Refactor search behind a reusable typed engine API
status: To Do
assignee: []
created_date: '2026-07-17 15:39'
labels: []
dependencies: []
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
parent_task_id: TASK-1
type: task
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Decouple search execution and reporting from the current stdin/stdout UCI driver so browser and UCI integrations can consume the same typed search lifecycle. This is the prerequisite for all UI runtime work.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Callers can start a search from a Position with a depth, time, or infinite limit and receive a typed final outcome
- [ ] #2 Iterative-deepening progress, score, nodes, NPS, principal variation, and current-move information are available as typed events rather than being printed by Search
- [ ] #3 A running search can be cancelled and reports an outcome that distinguishes completion from cancellation
- [ ] #4 UCI mode formats the typed events into its existing `info` and `bestmove` output without a behavior regression
- [ ] #5 Tests cover completed search, cancellation, event delivery, and UCI output formatting
<!-- AC:END -->
