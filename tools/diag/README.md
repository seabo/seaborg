# Strength-gulf diagnostic (seaborg vs Stockfish)

Tooling for the one-off spike that decomposes seaborg's playing-strength deficit
against a frontier engine into three axes — raw speed (NPS), selectivity
(effective branching factor / depth per node), and evaluation quality — so
effort can be directed at the axis that is actually limiting. This measures; it
does not change the engine. The findings are written up in `BENCHMARKS.md`.

Stockfish is used purely as a measurement reference. It never touches training
data, so it does not bear on the self-play-purity constraint that governs the
NNUE programme.

## Why these three axes, and where each shows up

Playing strength is (roughly) speed × depth-per-node × eval-per-node. The
physics matters for where each is measured:

- **NPS (raw speed).** Nodes searched per second. A small net can make seaborg's
  raw NPS *higher* than Stockfish's even though each Stockfish node is worth
  more — so NPS alone is never conclusive.
- **Selectivity / EBF (depth per node).** Better reductions and pruning reach a
  given depth in fewer nodes. This shows up at fixed nodes / fixed time, **not**
  at fixed depth: at fixed depth both engines by definition reach the same
  depth. `EBF = nodes**(1/depth)`; lower is more selective.
- **Eval quality (per-node accuracy).** How well the static evaluation ranks
  positions, measured search-free so it is not contaminated by search.

## Prerequisites

- A release seaborg built with `RUSTFLAGS="-C target-cpu=native"` (records the
  AVX2/native flags AC#1 asks for; the embedded NNUE net is used by default).
- A Stockfish binary matching the host ISA (e.g. `stockfish-ubuntu-x86-64-avx2`).
- `fastchess` on `PATH` for the gauntlets (AC#4).
- An **idle** single machine: NPS and EBF are sensitive to CPU contention, so
  both engines must be measured on the same quiet host, one thread each.
- `gen_positions.py` additionally needs `python-chess` (run it under
  `uv run --with chess ...`); the other scripts are stdlib-only.

## Scripts

- `uci.py` — shared minimal UCI driver (stdlib only).
- `nps_ebf.py` — AC#1/#2. Fixed-depth and fixed-movetime runs over
  `bench-positions.epd`; reports NPS, EBF, nodes-to-depth, depth-at-time.
- `gen_positions.py` — builds a diverse, mostly-quiet position set for AC#3 by
  sampling lightly-randomised games (needs python-chess + Stockfish).
- `eval_agreement.py` — AC#3. Deep-Stockfish labels each position; queries each
  engine's static eval; reports Spearman rank correlation and decisive-position
  winner accuracy (scale-independent — never raw centipawns).
- `gauntlet.py` — AC#4. One fastchess seaborg-vs-Stockfish match with a per-side
  search limit; parses Elo and W-L-D. Handicap Stockfish (fewer nodes / less
  time) and sweep to find the even point.
- `bench-positions.epd` — phase-balanced fixed suite for NPS/EBF.
- `selectivity_profile.py` — Seaborg's own selectivity profile (no external
  engine). Drives a `--features selstats` build over the same suite at fixed
  depth and fixed nodes, parses the `info string selstats {json}` line it emits,
  and reports the ordering / reduction / re-search / quiescence signals that say
  where effective depth is spent. The `selstats` feature is off by default and
  behaviour-transparent; see the "Search selectivity profile" section of
  `BENCHMARKS.md` for the findings.

## Reproduce

```sh
SB=/path/to/seaborg            # target-cpu=native release
SF=/path/to/stockfish-avx2

# AC#1 + AC#2: NPS and EBF
python3 nps_ebf.py --seaborg "$SB" --stockfish "$SF" \
    --suite bench-positions.epd --depth 15 --movetime 2000 \
    --repeats 3 --hash 64 --out nps_ebf.json

# AC#3: eval quality (isolated from search)
uv run --with chess python3 gen_positions.py --stockfish "$SF" \
    --games 500 --out positions.fen
python3 eval_agreement.py --seaborg "$SB" --stockfish "$SF" \
    --positions positions.fen --label-depth 20 --out eval_agreement.json

# AC#4: head-to-head, fixed nodes then fixed time (equal budgets first),
# then handicap Stockfish to find the even point.
python3 gauntlet.py --seaborg "$SB" --stockfish "$SF" \
    --seaborg-limit nodes=200000 --sf-limit nodes=200000 \
    --games 400 --concurrency 10 --openings ../strength/openings-v1.epd
python3 gauntlet.py --seaborg "$SB" --stockfish "$SF" \
    --seaborg-limit st=0.1 --sf-limit st=0.1 \
    --games 400 --concurrency 10 --openings ../strength/openings-v1.epd
```

# Mate-find-rate diagnostic (self-play only, no external engine)

A separate, self-contained diagnostic that asks whether the engine, at normal
search settings, finds forced mates that *provably* exist. A low find rate on
rules-verified mates is direct evidence of search over-pruning — the nominal
depth covers the mate but pruning discards the winning line — worth hundreds of
Elo; a high rate rules out a gross tactical hole.

Unlike the strength-gulf spike above, this consults **no external resource**: the
positions come from Seaborg's own self-play and the ground truth is the rules of
chess, proven by the move generator. No tablebase, opening book, or other engine
is involved.

## Ground truth is the rules, not a search

A forced mate is game-theoretically decided, so it can be proven with movegen
alone. The proof lives in `engine::mate` (the `MateSolver`): an exhaustive
minimax over legal moves that credits the side to move with a mate only when
*every* defender reply still loses. It shares no code with the engine's heuristic
search — no evaluation, no transposition table, no pruning that could discard a
defence — so it can serve as ground truth for a diagnostic *of* that search. The
generator additionally replays each proven line with an independent, purely
mechanical playout check.

## Two stages

1. **Generate the suite (Rust).** `mate_suite_gen` plays self-play games, and for
   every position the games searched, uses `MateSolver` to prove whether the side
   to move has a forced mate. Proven mates are written to `suites/mate_suite.json`
   with their distance and every proven mate-preserving first move. (The
   self-play search score is used only to skip clearly non-winning positions
   before the expensive proof; it never decides whether a mate is real.)

   ```sh
   cargo build --release -p engine --example mate_suite_gen
   ./target/release/examples/mate_suite_gen \
       --games 600 --nodes 40000 --max-mate-plies 7 --per-bucket 40 \
       --out suites/mate_suite.json
   ```

   `suites/mate_suite.json` is committed, so measurement needs no regeneration.
   Regenerate it to grow the set, reach deeper mates (raise `--max-mate-plies`
   and `--node-cap`), or source it from a specific network (`--network`).

2. **Measure a network (Python).** `mate_find_rate.py` runs a given network at a
   normal blitz per-move budget over the suite and reports, broken down by mate
   distance, how often the engine plays a mating move (checked against the proven
   winning moves) and how often it reports the mate in its score. The network is
   selected with the `EvalFile` UCI option, so comparing networks needs no
   rebuild.

   ```sh
   python3 mate_find_rate.py --seaborg "$SB" \
       --suite ../../suites/mate_suite.json \
       --network net.sbnn --movetime 200 --out mate_find_rate.json
   # --network none forces the hand-crafted evaluation; omit it to use the
   # binary's embedded network. --depth / --nodes replace --movetime for a
   # wall-clock-independent comparison.
   ```

The verifier is unit-tested in `engine::mate` (a known mate accepted, a
stalemate and a non-mate rejected, and agreement with an independent reference
solver), and the measurement's scoring logic in `test_mate_find_rate.py`.
