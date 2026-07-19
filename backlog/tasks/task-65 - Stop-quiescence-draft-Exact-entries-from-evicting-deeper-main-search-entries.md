---
id: TASK-65
title: Stop quiescence-draft Exact entries from evicting deeper main-search entries
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-19 15:07'
updated_date: '2026-07-19 20:42'
labels:
  - transposition-table
  - search
  - quiescence
  - performance
dependencies:
  - TASK-60
references:
  - engine/src/tt.rs
  - engine/src/search.rs
priority: low
type: bug
ordinal: 83000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Table::store keeps an existing same-key entry only when all three of: existing.depth > depth, existing.age == age, and NOT (the incoming bound is Exact while the existing bound is not). That third clause lets a shallow Exact write displace a deeper inexact one, which was a reasonable heuristic while the main search was the table's only writer and every entry carried a real search depth.

TASK-60 made quiescence a writer at a reserved draft of 0. A quiescence fall-through store classified Exact therefore evicts a deeper non-Exact main-search entry for the same position, discarding a genuinely better-informed result in favour of a capture-only one. Confirmed directly against engine/src/tt.rs: seeding a key at depth 8 with Bound::Lower and then storing the same key at depth 0 with Bound::Exact leaves a depth-0 entry.

Measured impact is currently small, which is why TASK-60 was approved with this recorded rather than blocked. Quiescence only produces Exact at PV quiescence nodes: in a zero-window quiescence node a stand pat that would raise alpha instead triggers the beta cutoff, so alpha never rises and the bound is always Upper. Instrumenting real searches over eight positions at depths 5-10 counted 0-1 such evictions per search, with no warm-versus-cold node-count regression and no measurable search benchmark change. The exposure grows if PV quiescence nodes become more common or the exactness rule is widened.

The fix is a policy decision rather than a mechanical change: the depth comparison and the exact-bound preference need to be reconciled now that draft no longer implies comparable search effort. Options include making the exact-bound preference conditional on comparable depth, excluding the reserved quiescence draft from that preference, or ranking same-key candidates by the same quality metric already used for cross-slot victim selection. TASK-60's implementation notes flagged replacement tuning for the new quiescence write volume as unattempted; this is the specific mechanism that pass should address.

Note that the cross-slot victim-selection path already handles this correctly by ranking on depth plus an exact bonus. The defect is confined to the same-key branch.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A quiescence-draft Exact store never displaces a deeper main-search entry for the same key, with a direct regression test over Table::store covering the depth-8 inexact versus depth-0 Exact case
- [ ] #2 The chosen same-key replacement rule is documented at the decision site, stating why draft no longer implies comparable search effort and what the rule now compares
- [ ] #3 Same-key replacement behaviour is specified and tested across the bound and depth combinations that can arise from the two writers, including equal-depth and differing-age cases
- [ ] #4 Warm-versus-cold node counts and the search benchmark show no regression against the pre-change measurement on the same machine
- [ ] #5 Cross-slot victim selection is confirmed unchanged, or any change to it is measured and justified
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Extract the existing depth/bound/age replacement-quality calculation and use it consistently for both same-key updates and cross-slot victim selection. Replace a same-key entry when the incoming current-age candidate has equal or greater quality, preserving an existing move when the new entry is move-less.
2. Document at Table::store that draft is writer-specific rather than a comparable effort measure, and that same-key candidates are compared by the shared depth, Exact-bound, and relative-age quality metric.
3. Replace the old shallow-Exact special-case tests with a table-driven same-key policy matrix covering quiescence draft versus deeper main-search bounds, equal depths and bounds, quality boundaries, and differing ages; retain cross-slot tests unchanged.
4. Run focused TT/search tests, capture warm-versus-cold and hash-load node-count evidence, compare the search benchmark against the pre-change baseline on this machine, then run the repository-required formatting, strict Clippy, and workspace test gates.
<!-- SECTION:PLAN:END -->
