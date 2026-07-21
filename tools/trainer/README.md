# NNUE trainer

The Python/PyTorch training project for Seaborg's NNUE network. It consumes the
packed self-play samples the engine generates and produces an **fp32
checkpoint**. Quantized export and the strength-preserving numeric guarantees are
separate, later tasks; this project stops at float weights.

Everything here implements the shared decisions in
[`docs/nnue-design-contract.md`](../../docs/nnue-design-contract.md): the feature
set and index formula, the topology and its parameterizable dimensions, and the
blended win-probability training target. When the contract and this code
disagree, the contract wins.

## Layout

| File | Role |
| --- | --- |
| `model.py` | The float NNUE model and its `NnueConfig` (the contract's parameterizable dimensions). |
| `data.py` | The dataloader: memory-maps the packed format and decodes batches into sparse `EmbeddingBag` inputs with vectorised NumPy. |
| `train.py` | Training loop, the blended target, checkpoint writing, and the throughput benchmark. |
| `testsupport.py` | A reference encoder for the packed format, used by the tests. |
| `test_data.py`, `test_model.py` | `unittest` suites (no pytest dependency). |

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

## Measured throughput

The network is tiny (~197k parameters at H=256), so training is dataloader-bound:
if the loader cannot decode samples faster than the model consumes them, the GPU
starves. The loader is built to stay ahead — memory-mapped file, whole-batch
vectorised decode, no per-sample Python loop.

Measured on this machine (Apple Silicon, CPU, `torch` 2.13, 216,233 real
self-play samples, batch size 16,384):

```sh
.venv/bin/python train.py --data samples.bin --benchmark --batch-size 16384
# dataloader throughput: ~561,000 samples/sec
```

| Stage | Throughput | Notes |
| --- | --- | --- |
| Dataloader (decode only) | **~561,000 samples/sec** | random-access shuffled batches |
| Full CPU training step | ~197,000 samples/sec | decode + forward + backward + optimizer + validation |

The loader runs ~2.8× faster than the full CPU training step, so it does not
starve even a CPU trainer; a GPU consumes the model faster still, which is why
the decode rate is the figure that matters and it is the larger one. The numbers
scale with the machine — re-run `--benchmark` to record them for a given host.

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

## Testing

```sh
.venv/bin/python -m unittest discover -p 'test_*.py'
```

`test_data.py` checks feature indices against the contract formula by hand,
side-to-move perspective selection, target decoding, stream-header rejection, and
the mirror invariance of the sparse encoding. `test_model.py` checks
configuration validation, parameterization, that a mirrored position evaluates
identically (an architectural property that holds without training), the target
blend, and that a short run converges.
