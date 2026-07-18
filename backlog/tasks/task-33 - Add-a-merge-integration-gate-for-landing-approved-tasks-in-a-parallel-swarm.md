---
id: TASK-33
title: Add a merge-integration gate for landing approved tasks in a parallel swarm
status: To Do
assignee: []
created_date: '2026-07-18 00:21'
labels:
  - architecture
  - process
  - lifecycle
dependencies: []
priority: high
type: chore
ordinal: 36000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Context / problem

The task lifecycle reviews and approves an immutable implementation SHA against its frozen base (the base->target diff). That certifies a change is correct *in isolation*. It does not certify that the primary branch is still green after the change is integrated, because primary advances between when a base is frozen and when the branch is merged.

Today `$review` approves (`Ready to Merge`) and a human merges to `Done` with no re-verification of the integrated result. With a single serialized developer this gap is small. The project's target operating mode is an agent swarm landing many tickets in parallel, where many branches are each approved against stale bases and semantic merge conflicts (textually-clean merges that are logically wrong or break tests) are the common case, not the tail. In that world an ungated merge cannot keep primary green — this is the classic "two independently-green branches merge into a red primary" problem (the not-rocket-science rule / bors / merge queues).

Decision already taken: review stays an isolation check on the immutable SHA (this is what lets review fan out in parallel). The missing piece is a merge-time integration gate. Testing a prospective merge at review time is rejected: it breaks target immutability and is stale the moment primary moves.

What to do (this ticket)

Design and document the `Ready to Merge` -> `Done` transition as an integrator-driven merge gate, and specify its mechanics precisely enough to implement. Build the correct *serial* gate; explicitly defer throughput optimizations (speculation/batching) to follow-up tasks. Update TASK_LIFECYCLE.md and the review skill scope, and capture follow-up tasks for the automation tooling.

Core mechanics to specify:
- A single authoritative integration step (one logical integrator) forward-integrates the approved immutable target onto the live primary tip, runs required checks + hot-path (perft/movegen) benchmarks on the integrated result, and fast-forwards primary only if clean and green.
- Serialized against the true primary tip for correctness first; speculation/batching is documented as a deferred optimization only.
- Eject policy: textual conflict, or failed integrated checks/benchmarks, returns the task to `Changes Requested` with evidence — never `Done`.
- Overlap re-review: when forward-integration touches files/modules changed by recently-landed tasks, route to a fresh isolation review instead of auto-landing.
- Respect Backlog task dependencies (never land a task before its unlanded dependencies).
- Document the honest tradeoff: landed code is the reviewed change forward-integrated and re-tested, not the exact reviewed bytes; test-suite depth is the primary automated semantic-conflict net.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 TASK_LIFECYCLE.md redefines Ready to Merge -> Done as an integrator-driven merge gate: a single authoritative step forward-integrates the approved immutable target onto the live primary tip, runs required checks and hot-path (perft/movegen) benchmarks on the integrated result, and fast-forwards primary only if the integration is clean and green
- [ ] #2 The gate is specified as serialized against the true primary tip (correctness before throughput); speculative/batched execution is explicitly documented as a deferred optimization and is out of scope here
- [ ] #3 Eject policy is defined: a textual integration conflict, or failed integrated checks/benchmarks, returns the task to Changes Requested with evidence and never to Done
- [ ] #4 Overlap re-review policy is defined: when forward-integration touches files or modules changed by recently-landed tasks, the task is routed to a fresh isolation review rather than auto-landing, and $review records the base SHA and touched paths so overlap is computable
- [ ] #5 The gate respects Backlog task dependencies and never lands a task before its unlanded dependencies
- [ ] #6 The review skill and TASK_LIFECYCLE.md state that review validates the change in isolation against the immutable SHA and does not test a prospective merge; the integration guarantee lives in the gate
- [ ] #7 The landed-code-is-not-the-reviewed-SHA tradeoff is documented, naming test-suite depth as the primary automated semantic-conflict net
- [ ] #8 Follow-up tasks are captured for: (a) the automated integrator/merge-queue tooling, (b) speculative/batched execution for throughput, and (c) overlap-detection tooling
<!-- AC:END -->
