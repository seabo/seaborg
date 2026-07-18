---
id: TASK-33
title: Automate merge integration when the manual merge skill becomes a bottleneck
status: To Do
assignee: []
created_date: '2026-07-18 00:21'
updated_date: '2026-07-18 00:31'
labels:
  - architecture
  - process
  - lifecycle
dependencies: []
priority: low
type: chore
ordinal: 36000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Status: future enhancement. Not needed until the manual `$merge` skill is a throughput bottleneck.

Context

The `$merge` skill (see .agents/skills/merge/SKILL.md) already provides the correct integration gate: a human invokes merges serially, and for each approved task the skill merges the immutable approved target onto the live primary tip, re-runs required checks and hot-path (perft/movegen) benchmarks on the integrated result, and advances primary only if that result is clean and green. A compare-and-swap on the primary ref keeps this correct even if two invocations overlap, so human serialization is a throughput assumption, not a correctness one. This keeps primary green under real integration while review stays a pure isolation check on the immutable SHA.

That manual gate is correct but human-paced: throughput is capped at roughly 1 / test_time and needs a human in the loop per land. In the target agent-swarm mode, once many approved tasks queue up, that cap becomes the bottleneck.

What to do (only when the bottleneck is real)

Automate the same gate so it no longer needs per-land human invocation, and add throughput mechanics. Do not change the safety model: still merge (never rebase) the immutable target, still verify the integrated result, still compare-and-swap onto the live primary tip, still eject failures to Changes Requested.

Candidate mechanics:
- A single logical integrator that drains the Ready to Merge queue without a human per land, respecting Backlog task dependencies.
- Speculative / batched execution (build primary+A, primary+A+B, ... in parallel; land the green prefix; discard from the first failure and rebuild) to lift throughput past 1 / test_time.
- Overlap detection -> automatic re-review: when forward-integration touches files or modules a recently-landed task changed, route to a fresh isolation review instead of auto-landing (the manual skill only surfaces this for human judgment). This needs $review to record base SHA and touched paths.

First, gather evidence from the manual skill in use (land rate, conflict rate, eject rate, wait times) to justify the investment before building any of this.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Evidence gathered from the manual `$merge` skill in use (land rate, conflict rate, eject rate, wait times) shows per-land human invocation is a real throughput bottleneck that justifies automation
- [ ] #2 A single logical integrator drains the Ready to Merge queue without a human per land, respecting Backlog task dependencies, preserving the manual skill's safety model (merge not rebase, verify the integrated result, compare-and-swap onto the live primary tip, eject failures to Changes Requested)
- [ ] #3 Speculative or batched execution is added so throughput is no longer capped at roughly 1 / test_time, with any coverage the batching drops logged rather than silently skipped
- [ ] #4 Overlap detection automatically routes an integration that touches files or modules a recently-landed task changed to a fresh isolation review instead of auto-landing; $review records the base SHA and touched paths so overlap is computable
- [ ] #5 The safety guarantees are unchanged from the manual skill: review remains a pure isolation check on the immutable SHA, and primary only advances on a clean, green integrated result
<!-- AC:END -->
