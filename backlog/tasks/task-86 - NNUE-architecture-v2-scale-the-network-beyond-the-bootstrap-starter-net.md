---
id: TASK-86
title: 'NNUE architecture v2: scale the network beyond the bootstrap starter net'
status: To Do
assignee: []
created_date: '2026-07-25 12:22'
labels:
  - nnue
dependencies: []
priority: high
ordinal: 142000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The gen-000..gen-002 self-play bootstrap (TASK-69) plateaued at ~+337 Elo over the hand-crafted evaluation with the deliberately minimal starter network (768 -> 256 -> 1, single hidden layer, no king buckets). The plateau is a recipe ceiling, not NNUE exhaustion: the next lever is a larger/better network trained on the higher-quality corpus being generated in TASK-81. This epic covers graduating to a bigger architecture: first the enablers that make a bigger net affordable (incremental accumulator in the search hot loop, SCReLU), then a topology-only v2 (wider feature transformer, output buckets, deeper output stack) validated by a loss-vs-NPS Pareto sweep on the fixed corpus, and finally (gated on results and the TASK-82 diagnostic) the king-bucketed feature set. Playing strength is a joint function of eval quality and search speed, so architecture selection is governed by realized fixed-time-control Elo, screened by a loss/NPS frontier.
<!-- SECTION:DESCRIPTION:END -->
