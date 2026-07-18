---
id: TASK-29
title: Add a ply cap on quiescence check extensions
status: To Do
assignee: []
created_date: '2026-07-17 20:29'
updated_date: '2026-07-18 22:02'
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

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 18:30
---
TASK-39 interaction finding: the guaranteed depth-1 search enters quiescence while both cancellation and the time deadline are suppressed. The current threefold/fifty-move termination rules prove finiteness but not a practically small bound because quiet check evasions can recurse without reducing material and irreversible moves reset the fifty-move clock. A total quiescence/check-extension ply cap here would make the deadline-overrun recursion depth explicit; validate the chosen cap against TASK-39's adversarial corpus (doc-3). The measured corpus was fast (10,000 warmed immediate-stop samples, max 1.069 ms; retained startup/warm outlier 5.897 ms), but that is not a proof. TASK-45 separately removes explicit stop/quit/EOF cancellation from dependence on the capped tree after a legal root fallback is recorded.
---

author: @codex
created: 2026-07-18 20:09
---
TASK-39 investigation finding, recorded here so it is not lost when this ticket is picked up.

A quiescence check-extension ply cap alone does NOT bound the TASK-32 abort-suppressed window, and TASK-29 should not be treated as the fix for UCI stop responsiveness.

Evidence (see doc-3 and engine/examples/task39_qtree.rs): the longest run of consecutive quiet check evasions found anywhere was 4 — across the 300-position WAC suite, a 5,000-position random sweep, a 16-position named corpus, and six purpose-built mutual-check/discovered-check batteries. A cap at any plausible value would therefore almost never bind.

The ply-1 quiescence trees that are actually large (reachable to 46 ply and past 20 million nodes on ordinary dense tactical positions) are driven by capture and promotion interleaving, which a check-extension cap does not touch, and which resets the halfmove clock so quiesce Step 1 never fires.

TASK-29 remains worth doing on its own merits: it bounds a genuinely unbounded recursion and helps the time-deadline path. But bounding the ply-1 window by structure would require a total q-node or total q-ply budget rather than a check-extension cap. Prompt cancellation is TASK-45's job.
---

author: @codex
created: 2026-07-18 20:10
---
Correction to the preceding TASK-39 finding: the longest consecutive quiet check-evasion chain observed was 5, not 4. The 5,000-position random sweep (seed 1580315493) found two positions at chain length 5; WAC and the named corpus topped out at 4. The conclusion is unchanged and if anything reinforced: chains cluster at 2-3, so a check-extension cap at any plausible value would almost never bind, while reachable ply-1 q-trees run to 55 ply in that same sweep.
---

author: @codex
created: 2026-07-18 22:02
---
Path correction to the TASK-39 findings above: the reachability model referenced in comment #2 was renamed before merge and now lives at engine/examples/qtree_reachability.rs (it was engine/examples/task39_qtree.rs). The companion latency harness is tools/stop_latency_probe.rb (was tools/task39_stop_probe.rb).

Reproduce the corpus this ticket should validate a chosen cap against with:
  cargo run --release -p engine --example qtree_reachability -- corpus 20000000
  cargo run --release -p engine --example qtree_reachability -- wac 2000000
  cargo run --release -p engine --example qtree_reachability -- sweep 5000 1580315493 200000

The findings in comment #2 and the correction in comment #3 are otherwise unchanged.
---
<!-- COMMENTS:END -->
