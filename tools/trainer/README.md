# NNUE trainer

The Python/PyTorch training project for Seaborg's NNUE network. It consumes the
packed self-play samples the engine generates, trains a model, and **exports the
quantized `SBNN` network file the engine loads and runs**.

Everything here implements the shared decisions in
[`docs/nnue-design-contract.md`](../../docs/nnue-design-contract.md): the feature
set and index formula, the topology and its parameterizable dimensions, the
blended win-probability training target, and the quantization scheme. When the
contract and this code disagree, the contract wins.

## Layout

| File | Role |
| --- | --- |
| `model.py` | The NNUE model and its `NnueConfig` (the contract's parameterizable dimensions), including the quantization-aware forward pass. |
| `data.py` | The dataloader: memory-maps the packed format and decodes batches into sparse `EmbeddingBag` inputs with vectorised NumPy. |
| `train.py` | Training loop, the blended target and its `LambdaSchedule`, the validation split, checkpoint writing, and the throughput benchmark. |
| `split.py` | The deterministic by-shard (by-game) train/validation split, derived from the corpus provenance manifest. |
| `sweep.py` | The architecture-sweep screen: candidate enumeration, the loss/NPS Pareto frontier, and the finalist SPRT hand-off. |
| `export.py` | Quantizes a checkpoint and writes the versioned `SBNN` network file; also the integer forward pass the export is checked against. |
| `testsupport.py` | A reference encoder for the packed format, used by the tests. |
| `test_data.py`, `test_model.py`, `test_train.py`, `test_export.py` | `unittest` suites (no pytest dependency). |

## Setup

```sh
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt
```

CPU wheels are enough. For GPU training, install a CUDA `torch` build and pass
`--device cuda`.

## Generating data

The packed samples come from the engine's self-play data generator (no external
games — see the contract's purity boundary):

```sh
cargo build --release --bin seaborg
./target/release/seaborg datagen --games 3000 --nodes 3000 \
    --filter-opening-plies 8 --opening-plies 6 --out samples.bin
```

## Training

```sh
.venv/bin/python train.py --data samples.bin --epochs 25 --batch-size 16384 \
    --hidden 256 --lambda 0.3 --out checkpoint.pt
```

Key flags mirror the contract's parameters: `--hidden` (H, a positive multiple
of 16), `--activation` (`crelu`/`screlu`), `--scale`, and `--lambda` (the weight
on the game outcome; 0 trusts search, 1 trusts the result).

The checkpoint stores the architecture config plus float weights:
`feature_transformer.weight` is `[768, H]` in the same feature-major order the
on-disk `W_ft` block uses, so quantized export serialises it without
transposing.

### Scheduling lambda

`lambda` weights the game outcome against the search score. Self-play outcomes
from a weak bootstrap are noisy, so the contract's schedule leans on search
scores early and shifts toward outcomes as strength grows across reinforcement
generations. A run trains one generation, so a schedule resolves to a single
`lambda` for that run and the ramp plays out across successive runs:

```sh
# Generation 3 of a 0.1 -> 0.5 ramp spanning 10 generations.
.venv/bin/python train.py --data samples.bin --lambda 0.1 --lambda-end 0.5 \
    --lambda-generations 10 --generation 3 --out gen3.pt
```

Without `--lambda-end`, `--lambda` is a constant (default 0.3).

### Quantization-aware training

The engine runs an **integer** network, and the `QB = 64` output-weight grid
alone shifts a naively-quantized score by tens of centipawns. So training is
quantization-aware by default: the forward pass rounds weights and activations
onto the engine's integer grids (with a straight-through gradient), so the model
optimises the behaviour the export will actually ship. Pass
`--no-quantization-aware` to train the plain fp32 model instead. Either way, the
feature-transformer weights are clamped each step so the i16 accumulator cannot
overflow for any legal position — the contract makes that overflow a defect.

### Validation split: by-shard, not by-position

`--split` chooses how the validation set is held out. The default `by-position`
shuffles individual positions and reserves a `--val-fraction` slice — fine for a
convergence check, but it **leaks**: successive plies of one self-play game are
near-duplicates carrying the same outcome label, so a random split puts near-twins
on both sides of the boundary and makes validation loss optimistic.

`--split by-shard` holds out whole datagen runs instead. It reads the corpus
provenance manifest (`corpus.manifest.json`, written beside `corpus.bin` by
`tools/rl/datagen_campaign.py`), maps each shard to its contiguous record span,
and reserves whole lowest-hash shards for validation — so no game, and no
position's same-game neighbour, straddles the split. The choice is a fixed hash of
each shard's run identity folded with `--split-seed`, so the same corpus and seed
yield the byte-identical split on every run; a sweep therefore compares every
candidate on the same held-out games. This is required for a fair architecture
sweep (see [`docs/nnue-architecture-sweep.md`](../../docs/nnue-architecture-sweep.md)).

```sh
.venv/bin/python train.py --data corpus.bin --split by-shard \
    --manifest corpus.manifest.json --val-fraction 0.1 --split-seed 0 \
    --hidden 256 --out checkpoint.pt
```

`--manifest` defaults to `corpus.manifest.json` beside `--data`. The trainer
rejects a corpus whose record count disagrees with the manifest, so a stale
manifest cannot silently mis-split the data.

## Exporting a network

`export.py` quantizes a checkpoint and writes the versioned `SBNN` file
(`engine/src/nnue/format.rs`) the engine loads directly:

```sh
.venv/bin/python export.py --checkpoint checkpoint.pt --out network.sbnn
```

Quantization follows the contract (round half to even): `W_ft, b_ft = round(·QA)`
as i16, `W_out = round(·QB)` as i16, `b_out = round(·QA·QB)` as i32. The export
refuses a network whose accumulator could overflow i16 or whose weights overflow
their integer type, so a written file is always one the engine can run.

Because training is quantization-aware, the exported integer network reproduces
the model's own centipawn evaluation to within the dequantizing divide's rounding
(≤ 1 cp): with the same rounded weights and activations on both sides,
`integer_eval_cp` equals `round(SCALE · fout)`. `test_export.py` asserts this on a
trained fixture, and a Rust integration test
(`engine/tests/loads_exported_network.rs`) loads an exported fixture to confirm
the two languages agree on the byte layout.

### Golden vectors (the cross-language sync check)

Byte-layout agreement is not the same as *arithmetic* agreement: the two
languages must also compute the same integer score from the same weights. The
exporter emits a golden-vector fixture to pin that down — a network plus a set of
`(category, FEN, expected-centipawn)` triples, where the expected score is this
exporter's integer forward pass:

```sh
.venv/bin/python export.py --emit-golden ../../engine/tests/fixtures
# writes golden_v1.sbnn and golden_v1.vectors
```

The positions span tactical, endgame, king-safety, and near-overflow (maximally
dense) boards, so the check exercises the clip and the wide-value regime, not just
quiet middlegames. The engine's differential test
(`engine/src/nnue/inference.rs`) loads the pair and asserts its scalar forward
pass — and, on a CPU with AVX2, its SIMD forward pass — reproduces every expected
integer exactly, a three-way equality across the language boundary. Pass
`--golden <dir>` alongside `--checkpoint/--out` to emit the same vectors for a real
trained network. `test_export.py` derives the golden features from a FEN
independently of the packed-record decoder and checks the two agree, and asserts
the committed fixture still matches what the exporter emits.

## Measured throughput

The network is tiny (~197k parameters at H=256), so training is dataloader-bound:
if the loader cannot decode samples faster than the model consumes them, the GPU
starves. The loader is built to stay ahead — memory-mapped file, whole-batch
vectorised decode, no per-sample Python loop — and it decodes batches across
several threads (`--num-workers`) so decoding is not stuck on a single core.

`--num-workers` is a pure speedup: because each batch is a function only of its
slice of the shuffled index order, the decode is spread across threads and the
results are collected in submission order, so the sequence of batches — and thus
the training trajectory — is identical for any worker count. It defaults to a
conservative parallel value; set it to taste against the `--benchmark` figure on
your host. It changes wall time only, never the trained network, which is why the
architecture sweep can run every candidate under it.

Measured on the training host (AMD Ryzen 9 3900XT, 12 physical cores, CPU decode,
one ~5.1M-sample shard, batch size 8,192):

```sh
.venv/bin/python train.py --data samples.bin --benchmark --batch-size 8192 --num-workers 8
# dataloader throughput: ~1,445,000 samples/sec (batch_size=8192, num_workers=8)
```

| `--num-workers` | Throughput | Speedup |
| ---: | ---: | ---: |
| 1 (serial) | ~574,000 samples/sec | 1.0× |
| 8 | **~1,445,000 samples/sec** | ~2.5× |
| 12 | ~1,285,000 samples/sec | ~2.2× |

The speedup plateaus and then declines: decode is memory-bandwidth bound, so a
few threads saturate the memory system and adding more only adds contention. On
this host eight workers is the sweet spot. The numbers scale with the machine —
re-run `--benchmark` to record them for a given host. Threads (not processes) do
the work: NumPy releases the GIL for the vectorised decode, so threads run it
concurrently while sharing the memmap, with none of the per-batch copies or
fork-after-CUDA hazards a process pool would bring.

## Convergence

A representative 25-epoch run over the 216k-sample set above (`--lambda 0.3`,
`--lr 1e-2`, H=256):

| Epoch | Train loss | Val loss |
| --- | --- | --- |
| 1 | 0.0462 | 0.0309 |
| 5 | 0.0086 | 0.0086 |
| 10 | 0.0042 | 0.0055 |
| 25 | 0.0016 | 0.0040 |

Both losses fall monotonically and the validation loss tracks the training loss
without diverging, so the model is fitting a generalisable signal rather than
memorising. Loss is MSE in win-probability space, so these are squared errors on
a `[0, 1]` target: a final val loss of 0.004 is a typical error of ~0.06 in win
probability.

## Architecture sweep

`sweep.py` runs the screen that chooses the next network's *shape*
([`docs/nnue-architecture-sweep.md`](../../docs/nnue-architecture-sweep.md) fixes
the methodology). It enumerates candidate architectures one factor at a time —
hidden width, CReLU vs SCReLU, output-bucket count, output-stack depth, and
dense-tail int8 quantization — with every non-architectural knob held fixed (loss,
`lambda`, epochs/lr/batch/seed, corpus, and the by-shard split). For each it trains
and exports a QAT-quantized `SBNN`, records the post-QAT validation loss and the
realized single-thread NPS, computes the loss/NPS Pareto frontier, and writes
`sweep.json`: every screened point with attribution (network parameter hash and
binary commit), the frontier, the finalists spanning the trade, and the exact
`strength_test.py` commands to play them.

The screen ranks; it never selects. It stops at the SPRT commands — running the
multi-day training and the thousands of games, then picking the winner by realized
Elo, is the downstream campaign.

### Running it on the rig

Training and the NPS measurement are heavy and must run on one quiet host for the
whole sweep; [`rig`](../rl/README.md) is that host. Build an optimized,
`target-cpu=native` release binary first — NPS measured on a different build or
machine is not comparable and must never be mixed within a sweep.

```sh
RUSTFLAGS="-C target-cpu=native" cargo build --release --bin seaborg
.venv/bin/python sweep.py \
    --engine ../../target/release/seaborg \
    --corpus corpus.bin --manifest corpus.manifest.json \
    --baseline-net ../../engine/nets/default.sbnn --baseline-id gen-002 \
    --nps-suite ../diag/bench-positions.epd --nps-depth 13 \
    --build-settings 'RUSTFLAGS="-C target-cpu=native" cargo build --release' \
    --limit tc=10+0.1 --out-dir sweep-out
```

**The single-thread NPS protocol.** The cost axis is realized in-engine NPS, not a
forward-pass microbenchmark: the driver loads each candidate network, searches a
fixed position suite to a fixed depth on a **single thread**, and aggregates total
nodes over total search time. This folds in the incremental-accumulator cost the
search actually pays and the cache pressure a wider feature transformer adds —
exactly what decides whether a bigger net is affordable. Run on an otherwise idle
machine; NPS is sensitive to CPU contention. The same `--engine`, `--nps-suite`,
and `--nps-depth` must be used for every candidate.

**FastChess prerequisite.** The emitted commands invoke
`tools/strength/strength_test.py`, which drives [FastChess](https://github.com/Disservin/fastchess);
install it before running them (see
[`docs/strength-testing.md`](../../docs/strength-testing.md#installing-fastchess)).
The sweep itself does not need FastChess — only the finalist SPRT matches it hands
off do.

## Testing

```sh
.venv/bin/python -m unittest discover -p 'test_*.py'
```

`test_data.py` checks feature indices against the contract formula by hand,
side-to-move perspective selection, target decoding, stream-header rejection, and
the mirror invariance of the sparse encoding. `test_model.py` checks
configuration validation, parameterization, that a mirrored position evaluates
identically (an architectural property that holds without training), the target
blend, and that a short run converges. `test_train.py` pins the `LambdaSchedule`
arithmetic and its effect on the blended target. `test_export.py` checks the
quantization rounding, the accumulator bound, the `SBNN` serialization (with a
reader written independently of the writer), that the exported integer network
reproduces a trained model within tolerance, and the golden-vector emission — the
FEN feature derivation against the packed decoder, the category coverage, and that
the committed fixture matches the current export. `test_split.py` proves the
by-shard split is leak-free (no shard straddles the boundary) and byte-identical for
a given corpus and seed, and that the trainer rejects a corpus disagreeing with its
manifest. `test_sweep.py` pins the sweep's domination and Pareto-frontier logic
(including ties and single-candidate cases), the one-factor enumeration, and the
finalist SPRT hand-off.
