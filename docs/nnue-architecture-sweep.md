# NNUE architecture-sweep methodology

This document is a decision record, not code. It fixes the methodology for
choosing the next NNUE architecture *before* many candidate networks are trained
on the fixed corpus, so the sweep is fair, reproducible, and its result is
trustworthy. It is the companion to [`nnue-design-contract.md`](nnue-design-contract.md):
the contract fixes what the network *is* (feature set, quantization, file
format, training target); this document fixes how competing *shapes* of that
network are measured and one is selected.

Status: accepted, July 2026. Consumed by the topology-v2 work and the sweep run
that selects the v2 network. If a decision here changes after the sweep starts,
change it here first rather than letting individual candidate runs use
inconsistent protocols — an inconsistent protocol silently invalidates the
comparison across candidates.

## The governing objective

**Playing strength is a joint function of eval quality and search speed, so the
objective is realized fixed-time-control Elo — not validation loss, and not
NPS.** A network is only worth shipping if it makes the engine win more games at
a real clock. Two facts follow and shape everything below:

- A more accurate eval that is too slow loses on the clock: it searches shallower
  in the same wall-clock time, and the depth it forfeits can outweigh the
  per-node accuracy it gained.
- A faster eval that is less accurate misjudges positions: it searches deeper but
  steers toward worse moves.

The sweep therefore cannot optimize either axis alone. It optimizes the
trade-off, and the only instrument that measures the trade-off directly is a
game match at a fixed time control.

Game matches are expensive — commonly thousands of games per comparison (see
[`strength-testing.md`](strength-testing.md)). Training a candidate and measuring
its loss and NPS is cheap by comparison. So the methodology is a **funnel**: a
cheap screen (loss vs NPS) ranks many candidates and discards the clearly
dominated ones; only a few survivors earn the expensive game matches that make
the final decision.

## The two screen axes

The screen plots each candidate as one point in a two-dimensional space: eval
**quality** on one axis, eval **cost** on the other. Both axes must be measured
identically across every candidate, or the points are not comparable.

### Quality axis: post-QAT quantized validation loss

**Decision: the quality axis is the validation loss of the exported,
quantization-aware-trained *integer* network, with the loss function, `λ`, and
the training budget held fixed across all candidates. Only the architecture
varies.**

Rationale and the specifics that must be held fixed:

- **Post-quantization, not the fp32 model.** The engine runs an integer network,
  and quantization shifts the score (the `QB = 64` output grid alone moves a
  naively-quantized eval by tens of centipawns). The trainer is
  quantization-aware by default, so the model already optimizes the behaviour the
  export ships; the screen must score that same integer behaviour, not the fp32
  proxy. Concretely: score the loss the quantization-aware forward pass produces,
  i.e. the number that survives export to `SBNN`.
- **Loss function fixed.** MSE in win-probability space (the contract's default),
  identical for every candidate. A candidate must never be scored on a different
  loss than its rivals.
- **`λ` fixed.** The blend weight on the game outcome (default `0.3`) is held
  constant across the sweep. `λ` is a label-mixing choice, not an architecture
  choice; varying it between candidates would confound topology with target.
- **Training budget fixed.** Epochs, learning rate, batch size, optimizer, and
  RNG seed are identical for every candidate. A bigger net given more epochs
  would win the screen for a reason that has nothing to do with its shape.
- **One corpus.** Every candidate trains and validates on the same fixed corpus
  (the expanded gen-002 self-play corpus). The corpus is architecture-agnostic —
  features are derived from the stored position at train time — so no candidate
  needs its own data.

The single degree of freedom is the architecture: hidden width `H`, activation
(CReLU vs SCReLU), output-bucket count, and output-stack depth. Everything else
is a constant of the sweep.

### The validation split must be by game, not by position

**Decision: the train/validation split holds out whole games (equivalently,
whole opening lines), never random individual positions.**

Why this matters, and why the obvious split is wrong: the positions within a
single self-play game are heavily correlated. Successive plies differ by one
move, share almost all of their pawn structure and material, and carry the *same*
game-outcome label. A split that assigns individual positions at random therefore
puts near-duplicate positions on both sides of the split. Validation loss
measured that way is optimistic — the model has effectively seen each validation
position's near-twin during training — and, worse for a sweep, the leakage
compresses the gap between candidates, so the axis loses the discriminating power
the whole screen depends on. Holding out by whole game removes the leakage: no
position in validation has its own game's neighbours in the training set.

**This is a required change from the current trainer.** `tools/trainer/train.py`
today shuffles positions and takes the first `--val-fraction` as validation — a
by-position split with exactly the leakage described above. The sweep must not
use that split.

**Realizing a by-game split on the current format.** The packed record
(`engine/src/selfplay/format.rs`) stores a position, its search score, and the
game outcome, but *no game identifier* — records simply stream in play order,
each game's positions written contiguously. So the split is defined at the
granularity the format actually supports:

- **Preferred, no format change:** partition the corpus by whole contiguous game
  spans, and — because the corpus is accumulated from many independent datagen
  runs — reserve entire runs (shards) for validation. A whole run is a superset
  of whole games, so no game can straddle the split. Assign runs to train/val by
  a deterministic hash of the run's identity so the split is fixed and
  reproducible across every candidate. Because opening diversification comes from
  internal randomization at the start of each game, holding out whole runs also
  approximates holding out whole opening lines.
- **If finer (per-game) granularity is wanted:** add a game-boundary marker or
  game id to the packed format under a `FORMAT_VERSION` bump (the format reserves
  the version field for exactly this). This is an enabling option, not required
  for the sweep; the shard-level holdout above is sufficient and cheaper.

Whichever mechanism is used, the split is fixed once and reused byte-for-byte for
every candidate. A split that differs between candidates reintroduces the
confound the fixed-everything-else rule exists to remove.

### Cost axis: realized in-engine single-thread bench NPS

**Decision: the cost axis is realized single-thread nodes-per-second from the
engine's own `bench`, measured with the incremental accumulator maintained
through search — not a standalone forward-pass microbenchmark.**

Why the in-engine bench and not a microbenchmark of `evaluate`:

- **The incremental accumulator is the real cost model.** Once the accumulator is
  maintained along make/unmake through the search hot loop, per-node eval cost is
  `O(features-toggled × H)` — nearly independent of piece count — not the
  `O(pieces × H)` of a from-scratch rebuild. A microbenchmark that rebuilds the
  accumulator per call measures a cost the engine does not pay and would rank a
  wider net far more harshly than the engine actually experiences. The cost axis
  must reflect the incremental path.
- **Only the full search exercises the real access pattern.** Realized NPS folds
  in everything a microbenchmark omits: the fraction of nodes that actually reach
  `evaluate`, accumulator refreshes on the make/unmake seam, and the cache
  pressure a larger feature-transformer `W_ft` puts on the rest of the search.
  Those are precisely the effects that decide whether a wider net is affordable.

Measurement protocol: single thread, a fixed position/depth suite, an optimized
`target-cpu=native` release build, on one quiet machine for the whole sweep. Each
candidate's NPS is recorded in `BENCHMARKS.md` with attribution (the network's
parameter hash and the binary commit), exactly as other benchmark claims are —
NPS measured on different machines or build settings is not comparable and must
never be mixed within the sweep.

## The decision funnel

```text
many candidates
      │  train on fixed corpus (identical config), export QAT-quantized SBNN
      ▼
screen: (post-QAT val loss, in-engine single-thread NPS) per candidate
      │  drop every dominated point
      ▼
Pareto frontier: non-dominated (loss, NPS) trade-offs
      │  a few frontier points, spanning the trade
      ▼
fixed-time-control SPRT vs the current default (and each other)
      ▼
v2 = the finalist with the best realized fixed-TC Elo
```

1. **Screen.** Train every candidate under the fixed config above, export its
   QAT-quantized network, and record its post-QAT validation loss and its
   in-engine single-thread NPS.
2. **Pareto frontier.** Keep only the non-dominated candidates — a candidate is
   *dominated* if some other candidate is both lower-loss **and** higher-NPS. A
   dominated candidate cannot win on the objective (another net is strictly
   better on both screen axes), so it is discarded **without a game match**. This
   is where the screen saves the compute.
3. **Finalists → SPRT.** A small number of frontier points — chosen to span the
   loss/NPS trade, e.g. a fast-but-looser net, a slow-but-sharper net, and a
   middle one — play fixed-time-control SPRT matches against the current default
   (gen-002) and, where it discriminates, against each other, using the harness
   and statistical contract in [`strength-testing.md`](strength-testing.md).
4. **Selection.** The v2 network is the finalist with the best realized fixed-TC
   Elo that passes its SPRT gate. **Loss and NPS decide who plays; only game
   results decide who wins.**

### Why static loss cannot be the final arbiter

The screen ranks candidates, but it must never *select* one. Three reasons the
lowest-loss net is not necessarily the strongest, all rooted in the governing
objective:

- **Eval quality changes the search tree.** The eval is not scored in isolation
  during a game — it steers search. A different eval reorders moves and changes
  every eval-dependent decision (null-move pruning, late-move reductions,
  futility margins, TT cutoffs), so two nets with equal validation loss can
  search trees of different shape and effective depth in the same time. Loss
  measures per-position ranking on a frozen label set; it does not measure how
  the eval steers the search that actually plays the game. This coupling is why
  the final arbiter must be a real search at a real clock.
- **The clock is absent from loss.** Validation loss ignores NPS entirely. A net
  with lower loss but lower NPS may search shallower and lose. Realized fixed-TC
  Elo is the only measurement that integrates both axes into the quantity we
  actually care about.
- **The labels are self-referential.** Validation loss is measured against labels
  produced by the previous generation's search (gen-002). Those labels are
  imperfect; a net that fits them more closely is not guaranteed to play better,
  only to agree more with gen-002. Game results are judged by outcomes, not by
  agreement with a prior net.

## Reading the frontier: label-limited vs capacity-limited

The shape of the loss/NPS frontier is itself a signal about where the *next*
investment should go, after v2 is chosen. The key observation is what happens to
validation loss as candidate capacity (width, buckets, stack depth) grows.

- **Loss keeps falling as capacity grows — capacity-limited.** The labels still
  contain signal the current architectures cannot capture; more parameters buy
  more accuracy. If, despite that, realized Elo flattens or turns down as nets get
  bigger, the wall is *speed*, not signal: the eval gain is real but NPS cost eats
  it. The lever is efficiency — the incremental accumulator, SCReLU/SIMD, a
  better loss/NPS trade — so more capacity can be afforded, not more labels.
- **Loss stops falling as capacity grows — label-limited.** Adding width or depth
  no longer lowers held-out loss: the models have already extracted the signal
  the labels contain, and the ceiling is label *quality*, not parameter count.
  Spending NPS on more parameters that cannot be trained to a lower loss is
  wasted. The lever is **better labels**: raise the datagen per-move search node
  budget so the search-score labels are sharper, regenerating a higher-quality
  corpus, rather than growing the network.

**A concrete test to tell them apart.** Hold one frontier architecture fixed and
retrain it on a corpus generated at a higher datagen node budget. If its held-out
loss drops, the earlier flattening was label-limited — labels were the ceiling,
and better labels are the next investment. If the loss does not move, the
flattening was capacity/architecture-limited, and the next lever is a better or
more efficient architecture (for example the king-bucketed feature set), not more
labels. This distinguishes the two readings with a single controlled retrain
instead of guessing from the frontier's shape alone.
