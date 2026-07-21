# Engine strength regression testing

Run the repository-owned orchestration with Python 3 and a maintained
[FastChess](https://github.com/Disservin/fastchess) installation:

```sh
python3 tools/strength/strength_test.py \
  --baseline /absolute/builds/seaborg-baseline \
  --baseline-id 'git:BASE_SHA target-cpu=native release locked' \
  --candidate /absolute/builds/seaborg-candidate \
  --candidate-id 'git:CANDIDATE_SHA target-cpu=native release locked' \
  --build-settings 'cargo build --release; rustc ...; target-cpu=native' \
  --output artifacts/strength-BASE_SHA-CANDIDATE_SHA
```

Both paths must name already-built, executable, immutable artifacts. The tool
hashes their bytes; the caller must supply the revision and complete optimized
build settings because an executable cannot reveal those reliably. Never
overwrite either binary while a test runs. The output directory must not exist,
which protects prior evidence. It contains `report.json`, the complete runner
log, and finished games in PGN form. Archive the entire directory. Rerun the
printed command against the same hashed binaries and suite; FastChess consumes
the suite sequentially, so a rerun starts at the same opening. A stopped test is
evidence, but this first version deliberately reruns rather than statistically
resuming an interrupted SPRT.

`--engine-arg` passes a command-line flag identically to both engine
executables. seaborg needs no argument: it runs in UCI mode by default when
invoked with no subcommand, which is how the tool launches it. (Passing the old
`-u` flag now fails, because the current CLI has no such flag and rejects the
unknown argument at startup.) Use the `=`-prefixed form for any dash arguments a
future engine does need, so they are not mistaken for options.

## Installing FastChess

FastChess is a small, dependency-light C++ binary (no Qt). Build and install a
pinned release:

```sh
git clone --branch v1.7.0-alpha https://github.com/Disservin/fastchess
cd fastchess && make -j
install -m 0755 fastchess ~/.local/bin/fastchess   # any directory on PATH
```

Pass `--runner /path/to/fastchess` if it is not on `PATH`. The exact
`-version` string and the complete command are recorded in every report; a
runner upgrade that changes the score/SPRT output lines produces an
infrastructure error until the parser fixtures are updated. cutechess-cli is a
reasonable alternative runner, but its output format differs and the parser here
targets FastChess.

## Statistical contract

The candidate is player 1. Authoritative defaults test an Elo interval from
`elo0=-5` to `elo1=0`, with `alpha=0.05` and `beta=0.05`. Crossing the upper
log-likelihood boundary is `PASS`: sufficient evidence, at these chosen
hypotheses and error rates, against the practically significant regression.
Crossing the lower boundary is `FAIL`. Exhausting the even game cap without a
boundary is `INCONCLUSIVE`, never a pass. Exit statuses are 0 PASS, 1 FAIL,
2 INCONCLUSIVE, and 3 INFRASTRUCTURE ERROR. Only exit 0 is suitable as a merge
gate.

This policy does not prove equality, rule out a regression smaller than five
Elo, or guarantee any one finite match result. Lower error rates or a narrower
indifference interval cost more games. Override `--elo0`, `--elo1`, `--alpha`,
`--beta`, and `--max-games` explicitly when calibrating future policy; every
value is reported. Treat calibration as a documented policy decision, not a
claim that arbitrarily small regressions are detectable.

Compute cost is variable and commonly thousands of games. Increase
`--concurrency` only when the machine can give every engine equal, uncontended
resources. Use the same machine, OS, compiler, target features, and
background-load controls for both builds. `--engine-option NAME=VALUE` applies
identically to both players.

## Resource limits

`--limit` sets the FastChess resource budget applied equally to both engines.
Authoritative runs require a time-based limit (`tc=...` or `st=...`), e.g.
`--limit tc=10+0.1` for ten seconds plus 0.1s per move; this enforces the
equal-time contract. Smoke mode may also use `depth=N` or `nodes=N` for speed.
`--match-timeout SECONDS` aborts a match that runs longer than expected and
reports INFRASTRUCTURE ERROR, guarding against a hung runner or engine.

Engines must return a legal move within the configured limit and stay
responsive under the runner. seaborg satisfies this at fast timed controls: the
allocation policy in `engine/src/time.rs` never asks for more than the clock
holds, and the search honours a guaranteed-first-ply contract that always
produces a legal `bestmove` and cannot hang under a depleted clock. The earlier
illegal-null-move and hang failures (backlog task-32 and task-34) are fixed and
pinned by the `timed_selfplay` integration fixture, which plays whole games at
fast controls and asserts every move is legal and every game terminates. Use
`--mode smoke --limit depth=4` to exercise the full path against seaborg
quickly, or `--limit tc=...` for a timed run.

## Pairing, inputs, and failures

`openings-v1.epd` is repository-authored under CC0; its provenance is beside the
file and its SHA-256 is embedded in the script. Any silent change fails before
play. FastChess plays each opening as a colour-reversed pair (`-games 2
-repeat 2`, with `-rounds` counting pairs so the total equals `--max-games`),
sequential order, equal limit/hash/threads/options, and restarts engine
processes between games so stale state cannot cross games. The tool also
performs a UCI handshake/readiness/depth-one preflight, then fails closed on
missing inputs/dependencies, malformed or incomplete output, crashes,
disconnects, illegal moves, and time forfeits. Because any such event fails
closed to an `INFRASTRUCTURE ERROR` before a result is recorded, a completed
result always reports zero forfeits and zero crashes; those fields are reserved
for a future counting mode, and real crashes/forfeits appear instead in the
error report. FastChess emits `Illegal PV move` warnings for a bad
principal-variation line while the game still finishes; these are not treated as
failures.

## Smoke/calibration mode

Exercise the complete orchestration cheaply with `--mode smoke --limit depth=4
--max-games 4`. Smoke mode is printed and recorded as `NON-AUTHORITATIVE`; even
if it crosses a boundary it returns INCONCLUSIVE (2), never the authoritative
PASS status. It detects setup faults but says nothing reliable about strength.

This test complements formatting, unit, workspace, perft, and UCI protocol
tests. Whether a change can affect strength and should invoke this command is a
human/agent judgment outside the script; there is intentionally no path-based
or automatic functional-change classification.

Runner option semantics follow the
[FastChess documentation](https://github.com/Disservin/fastchess). The rationale
follows established
[Stockfish regression-test practice](https://official-stockfish.github.io/docs/stockfish-wiki/Regression-Tests.html),
while the defaults here are Seaborg policy.
