# Reinforcement loop

The orchestration that turns Seaborg's self-play into a stronger NNUE network,
one gated generation at a time. It is the **mechanism**: it composes the datagen
subcommand, the [trainer](../trainer/README.md), the exporter, and the
[strength-test SPRT harness](../strength/) — and adds only the loop, the gate
decision, and the bookkeeping. It contains no numeric machinery of its own.

`loop.py` is where the pieces meet; the network format, quantization, training
target, and purity boundary are all fixed by
[`docs/nnue-design-contract.md`](../../docs/nnue-design-contract.md).

## One iteration

Generation `g` runs four steps:

1. **Generate** self-play data with the current best network as the evaluator.
   Generation 0 has no network and bootstraps from the hand-crafted evaluation
   (`seaborg datagen` with no `--network`); each later generation evaluates with
   the current best network (`--network best.sbnn`), which is the last candidate
   that passed the gate — not necessarily the immediately preceding generation,
   since a rejected candidate promotes nothing.
2. **Train** a candidate on that data (`train.py --generation g`, so the lambda
   schedule advances across generations) and **export** it to the `SBNN` file the
   engine loads (`export.py`).
3. **Gate** the candidate against the current best with the strength harness. One
   `seaborg` binary plays both sides; they are told apart only by the `EvalFile`
   UCI option — `--candidate-option EvalFile=candidate.sbnn` against
   `--baseline-option EvalFile=best.sbnn` (generation 0's baseline is
   `--baseline-option EvalFile=none`, which asks for the hand-crafted evaluation
   explicitly: the engine binary embeds a network and plays with it by default,
   so omitting the option would gate against that network instead). The candidate
   is player 1, and only
   the harness's `PASS` (exit 0) is a promotion.
4. **Promote** the candidate to current-best only on `PASS`, and **record** the
   decision and its attribution in the ledger either way.

## The self-play purity boundary

The claim the loop protects is that strength was produced by the engine playing
itself, seeded only by its hand-crafted evaluation. This holds by construction:
the only evaluator any iteration ever plays with is the hand-crafted default or a
network **this loop promoted** from earlier self-play. No foreign engine, game
database, opening book, or imported weights enter the loop. `test_loop.py` pins
this invariant directly.

## Running it

`loop.py` drives real tools, so it needs a release engine build, the trainer's
Python dependencies, and FastChess for the gate:

```sh
cargo build --release --bin seaborg
python3 tools/rl/loop.py \
    --state tools/rl/state \
    --engine target/release/seaborg \
    --python tools/trainer/.venv/bin/python \
    --iterations 1 \
    --games 3000 --nodes 3000 \
    --train-arg --epochs --train-arg 25 --train-arg --hidden --train-arg 256 \
    --build-settings 'cargo build --release; target-cpu=native'
```

`--mode smoke --limit depth=4 --max-games 4` exercises the whole path cheaply;
smoke gates never return an authoritative `PASS`, so a smoke run demonstrates the
orchestration without promoting anything on the strength of a non-authoritative
result. Extra arguments for each underlying tool pass through with `--datagen-arg`,
`--train-arg`, `--export-arg`, and `--gate-arg` (repeatable).

`EvalFile` paths are read by the engine's UCI parser as a single whitespace-free
token, so a run's state directory must not contain spaces.

## The state directory

A run writes everything under `--state` (git-ignored); nothing lands in the
repository. Layout:

| Path | Contents |
| --- | --- |
| `best.sbnn`, `best.json` | the current best network and which generation it came from |
| `networks/gen-NNN.sbnn` | every promoted network, archived by generation |
| `iterations/gen-NNN/` | that generation's samples, checkpoint, candidate, and gate report + logs |
| `ledger.jsonl` | append-only record, one line per iteration |

Each ledger line carries the attribution a strength change needs to stay
accountable — data volume (games and samples), the node budget, the candidate and
baseline network ids, the gate verdict, and the measured Elo delta — mirroring
what [`BENCHMARKS.md`](../../BENCHMARKS.md) and the strength harness require. The
loop resumes from the ledger: generation numbers continue past the highest it
records.

## Testing

```sh
python3 -m unittest discover -p 'test_*.py'
```

`test_loop.py` runs the loop against a fake backend in place of datagen, PyTorch,
and FastChess, so it checks the generation-0 bootstrap, promote-only-on-`PASS`,
the survival of a rejected candidate's predecessor, the attribution fields, the
purity invariant, generation numbering and resume, and that a broken step stops
an iteration without recording it — none of which need the heavy tools.
