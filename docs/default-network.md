# The default network

A Seaborg binary carries a trained NNUE network inside the executable and plays
with it. `cargo build --release` is enough: there is no network file to ship
alongside the binary and no option to set before it plays at full strength.

The committed network is `engine/nets/default.sbnn`. It is linked in with
`include_bytes!` behind the default-on `embedded-net` Cargo feature, and parsed
at first use by `Network::read` — the same loader an operator-supplied
`EvalFile` goes through, so a corrupt or foreign file is rejected by the same
rules rather than trusted because it shipped with the build.

## Which evaluator is this binary running?

Every entry point says so on its diagnostic channel at startup, and the UCI
driver says so again whenever the evaluator changes:

```console
$ seaborg <<< 'quit'
seaborg 0.1.0 by George Seabridge (commit 30e530a14690)
evaluator: NNUE built-in gen-000 (hidden width 256, parameter hash 0xdaf86bb3d50cec6b)
```

The line names the network's promotion identifier, its hidden width, and the
parameter hash from its header. The hash is the discriminating field: two builds
can carry networks of the same width that play quite differently, and the hash
is what attributes a game or a benchmark to one of them. A build with no
embedded network reports `evaluator: hand-crafted evaluation` instead.

These reports go to stderr, never to stdout, so protocol output stays valid UCI
at every point in a session — including before the `uci` handshake, where no
`info string` would be legal. The Lichess bot logs the same line at startup.

## Choosing a different evaluator

The `EvalFile` UCI option takes three kinds of value:

| Value | Effect |
| --- | --- |
| `<empty>` | The built-in default: the embedded network, or the hand-crafted evaluation in a build without one. This is the advertised default and the state a session starts in. |
| `none` | The hand-crafted evaluation, whatever the build embeds. |
| a path | That `SBNN` file, overriding the embedded network. |

```
setoption name EvalFile value /abs/path/gen-007.sbnn
setoption name EvalFile value none
setoption name EvalFile value <empty>
```

A change is applied at a quiescent boundary and clears the hash, because the
transposition table caches static evaluations that belong to the evaluation
function rather than to the position. A file that fails to load changes nothing
and reports why.

Paths must be whitespace-free: the UCI parser takes the value as a single token.
The word `none` shadows a relative path spelled exactly `none`; such a file is
still reachable as `./none`.

## Building without an embedded network

```sh
cargo build --release --no-default-features
```

This produces a working engine that evaluates with the hand-crafted evaluation
and carries no weights. It is how the network's contribution to strength is
measured — build both ways and play them against each other — and it keeps the
crates buildable and testable with the feature off:

```sh
cargo test --workspace --no-default-features
```

Every path into the engine must switch together, so `lichess` and the root
`seaborg` package depend on `engine` with `default-features = false` and
re-export an `embedded-net` feature of their own. Cargo unifies features across
the dependency graph: a single edge left on the engine's defaults would
re-enable embedding for the whole build no matter what the top-level build
asked for.

## Entry points that do not use the embedded network

`seaborg datagen` evaluates with the hand-crafted evaluation unless `--network`
names a file, in a build that embeds a network as much as in one that does not.
This is deliberate. The generation a training sample belongs to is defined by
which network produced it, so data generation names its evaluator explicitly;
picking up whatever the binary happened to embed would make the labels
untraceable, and would silently corrupt the bootstrap generation, whose labels
must come from the hand-crafted evaluation. Self-play sets the evaluator from
its own configuration for the same reason.

The reinforcement loop's generation-0 gate therefore passes
`--baseline-option EvalFile=none` rather than omitting the option.

## Promoting and re-baking a network

The training loop (`tools/rl/`) promotes a candidate to `best.sbnn` when it
passes its SPRT gate. Promoting one into the binary is a separate, deliberate
commit — only promoted defaults are committed, not every training generation:

1. Copy the promoted network over `engine/nets/default.sbnn`.
2. Update `BUILT_IN_NETWORK_ID` in `engine/src/nnue/embedded.rs` to the
   generation identifier it was promoted under. The file keeps a fixed name so
   re-baking is a content change rather than a rename, which makes that constant
   the only record of *which* network a build carries.
3. Update the architecture and parameter-hash assertions in the same file's
   tests. They exist to make a mismatched bake fail loudly: a network swapped in
   without its identifier updated would still play, but every measurement
   attributed to it afterwards would name the wrong network.
4. Run `cargo test --workspace` and record the new evaluator line, exactly as
   the binary prints it, in the benchmark attribution for any strength result
   measured against the new default (see `docs/strength-testing.md` and
   `BENCHMARKS.md`).
