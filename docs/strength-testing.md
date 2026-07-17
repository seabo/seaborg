# Engine strength regression testing

Run the repository-owned orchestration with Python 3 and a maintained
`cutechess-cli` installation:

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
printed command against the same hashed binaries and suite; cutechess consumes
the suite sequentially, so a rerun starts at the same opening. A stopped test is
evidence, but this first version deliberately reruns rather than statistically
resuming an interrupted SPRT.

## Statistical contract

The candidate is cutechess player 1. Authoritative defaults test an Elo interval
from `elo0=-5` to `elo1=0`, with `alpha=0.05` and `beta=0.05`. Crossing the
upper log-likelihood boundary is `PASS`: sufficient evidence, at these chosen
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

Compute cost is variable and commonly thousands of games. Defaults allow
10,000 games at 10 seconds plus 0.1 seconds per move, one thread, 64 MiB hash,
and one concurrent game. Budget hours or days depending on hardware and draws.
Increase `--concurrency` only when the machine can give every engine equal,
uncontended resources. Use the same machine, OS, compiler, target features, and
background-load controls for both builds. `--engine-option NAME=VALUE` applies
identically to both players.

## Pairing, inputs, and failures

`openings-v1.epd` is repository-authored under CC0; its provenance is beside the
file and its SHA-256 is embedded in the script. Any silent change fails before
play. Cutechess uses each opening twice with colours reversed, sequential order,
equal time/hash/threads/options, and `restart=on` so stale process state cannot
cross games. The runner validates engine result claims. The tool also performs
a UCI handshake/readiness/depth-one preflight, then fails closed on missing
inputs/dependencies, malformed or incomplete output, crashes, disconnects,
illegal moves, and time forfeits. Because any such event fails closed to an
`INFRASTRUCTURE ERROR` before a result is recorded, a completed result always
reports zero forfeits and zero crashes; those fields are reserved for a future
counting mode, and real crashes/forfeits appear instead in the error report.

Runner output formats are part of the validated interface. Use a current
cutechess release that emits the documented score and SPRT lines; the exact
`-version` output and complete command are recorded. A runner upgrade that
changes these lines produces an infrastructure error until its parser fixtures
are updated.

## Smoke/calibration mode

Exercise the complete orchestration cheaply with `--mode smoke --max-games 4`
and a short time control. Smoke mode is printed and recorded as
`NON-AUTHORITATIVE`; even if it crosses a boundary it returns INCONCLUSIVE (2),
never the authoritative PASS status. It detects setup faults but says nothing
reliable about strength.

This test complements formatting, unit, workspace, perft, and UCI protocol
tests. Whether a change can affect strength and should invoke this command is a
human/agent judgment outside the script; there is intentionally no path-based
or automatic functional-change classification.

Runner option semantics follow the
[cutechess-cli manual](https://github.com/cutechess/cutechess/blob/master/docs/cutechess-cli.6).
The rationale follows established
[Stockfish regression-test practice](https://official-stockfish.github.io/docs/stockfish-wiki/Regression-Tests.html),
while the defaults here are Seaborg policy.
