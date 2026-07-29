---
id: TASK-96
title: >-
  First-class value-fidelity eval metrics (cp error, winner agreement, win-prob
  percentiles, calibration)
status: In Progress
assignee:
  - '@george'
created_date: '2026-07-29 18:43'
updated_date: '2026-07-29 18:43'
labels:
  - nnue
  - tooling
dependencies: []
documentation:
  - docs/nnue-architecture-sweep.md
priority: high
type: feature
ordinal: 162000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Validation loss (blended win-probability MSE) is hard to interpret as eval quality: it is "just a loss number", and rank correlation against the teacher has been shown to saturate. Add interpretable value-fidelity metrics that compare the network s static eval to the teacher search score (the label), computed on the held-out validation split and reported alongside val loss on every training run, plus a standalone evaluation over any exported network. Eval quality should be a first-class, legible output every generation, not an abstract loss.

Units are fixed by the trainer: the model output fout is the win-probability logit (p = sigmoid(fout)); the teacher win-prob is sigmoid(search_cp / scale) with scale=400. So model cp = fout * scale and teacher cp = the stored i16 search score. The reference for fidelity is the pure teacher search score (score_target), not the lambda-blended training target.

Metrics: mean and RMS eval error in centipawns; winner (sign) agreement rate; win-probability error percentiles (median / 90th / 99th); and a calibration curve (bin by predicted win-prob, compare to realized outcome from the stored WDL). Mate-band scores must not corrupt the cp metrics (win-prob metrics are naturally bounded; cp error must clamp or exclude mates explicitly).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A metrics module computes, on a validation split, at least: eval error in centipawns (MAE and RMSE), winner-agreement rate, win-probability error percentiles (median/90th/99th), and calibration bins, comparing the network eval to the teacher search score; unit-tested against hand-computed fixtures
- [ ] #2 train.py reports these metrics alongside val loss at the end of every run and stores them in the checkpoint, so they are first-class training outputs
- [ ] #3 A standalone CLI evaluates an existing exported SBNN on a corpus validation split and prints the same metrics, so any net (e.g. gen-002, gen3) is assessable without retraining
- [ ] #4 The cp metrics are robust to mate-band scores (mates clamped or excluded explicitly and reported as such); win-prob metrics remain bounded
- [ ] #5 Repo-required checks pass; tests cover the metric math, mate handling, and the standalone path
<!-- AC:END -->
