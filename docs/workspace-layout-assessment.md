# Rust workspace layout assessment

This document records the workspace structure as of July 2026 and evaluates it
against common Cargo and Rust conventions. The conclusion is that the package
boundaries and dependency direction are fundamentally sound. Targeted naming,
API, and manifest cleanup will improve clarity; a wholesale directory or crate
reorganisation would not currently pay for its migration cost.

## Current layout

The repository is both the Cargo workspace root and the `seaborg` binary
package. Its workspace has three members:

| Package | Location and targets | Responsibility | Workspace dependencies |
| --- | --- | --- | --- |
| `seaborg` | Repository root; one binary, integration tests, and six Criterion benchmarks | Process entry point, command-line selection, logging, developer utilities, and build revision metadata | `core`, `engine` |
| `core` | `core/`; one library | Chess-domain representation and rules: bitboards, pieces and squares, positions and FEN, move representation and generation, precomputed tables, and global initialization | None |
| `engine` | `engine/`; one library and one analysis example | Game/search behavior layered on the chess domain: evaluation, move ordering, search, transposition tables, time control, UCI, and the loopback browser UI | `core` |

The internal dependency graph is acyclic:

```text
seaborg binary ───────> engine library ───────> core library
       └──────────────────────────────────────> core library
```

The direct `seaborg` to `core` edge supports its development and perft command
modules and root benchmarks. The `engine` crate never depends back on the
binary package. Tests generally live beside their modules; process-level build
metadata tests live in root `tests/`, performance targets in root `benches/`,
and the engine-specific analysis program in `engine/examples/`.

Within `core`, low-level implementation helpers such as bit-twiddling, masks,
macros, and precalculation internals are private. The `position` module also
uses a conventional facade: its implementation submodules are private and it
re-exports the domain types callers need. By contrast, most top-level modules
in both libraries are public, and `engine` exposes every implementation module
from its crate root.

## Convention assessment

### What is already idiomatic

- A root package may also be a workspace root. Keeping the only executable at
  the repository root is supported directly by Cargo and avoids an otherwise
  cosmetic move into another directory.
- The domain, engine, and executable layers have distinct responsibilities and
  dependencies flow in one direction. There are no cyclic package edges.
- Standard Cargo target locations are used: `src/`, `tests/`, `benches/`, and
  `examples/`. Module filenames and directories follow snake-case Rust naming.
- Unit tests are colocated with implementation modules, while integration and
  benchmark targets are placed at package boundaries.
- Three packages are few enough that keeping member directories at the root is
  easy to scan. A `crates/` container is a common monorepo convention, not an
  idiomatic-Rust requirement.

### Deviations worth correcting

1. **The package name `core` collides conceptually with Rust's `core` crate.**
   Imports such as `use core::position::Position` look like standard-library
   imports and obscure which project owns the API. The generic package name
   `engine` creates a milder version of the same ambiguity. The collision is
   visible in the binary today, where a leading `::engine` path and a comment
   are needed after importing the `engine::engine` module.
2. **Library boundaries are package boundaries rather than supported API
   boundaries.** `engine/src/lib.rs` publicly declares every implementation
   module. Root benchmarks and the binary consequently reach into paths such
   as `engine::search`, `engine::tt`, and `engine::perft`. `core` has some good
   local facades but likewise exports broad implementation-shaped modules.
   This makes routine internal reorganisation look like a workspace-wide API
   migration.
3. **Workspace-wide manifest policy is repeated rather than centralized.** All
   packages repeat edition, version, license, path relationships, and common
   dependency declarations. Versions of shared dependencies can drift, and
   direct dependencies already include different `rand` generations. Cargo's
   workspace package and dependency inheritance are intended for this case.

These are maintainability issues, but they do not show that the three current
responsibility boundaries are wrong.

## Recommendations and follow-up work

### Rename crates and define deliberate facades

Rename the domain library to a project-specific name (for example
`seaborg-chess`) and choose equally unambiguous names for public engine APIs.
Make implementation modules private where possible, re-export stable domain
types and engine entry points from deliberate crate-root or feature facades,
and migrate the binary, tests, examples, and benchmarks to those facades.

Rationale: this removes the standard-library name ambiguity and lets modules
evolve without every internal path becoming an accidental contract. The work
is **medium effort** because imports across all packages and benchmark targets
must be migrated together and visibility changes will reveal implicit API
coupling. This recommendation is tracked by **TASK-20, “Clarify crate names and
public API boundaries.”**

### Centralize workspace manifest policy and modernize dependencies

Adopt the current Cargo resolver explicitly and use workspace inheritance for
shared package metadata and dependencies where it improves consistency. During
that change, remove unused direct dependencies and deliberately reconcile
duplicate direct dependency generations instead of mechanically upgrading the
lockfile.

Rationale: a single declaration prevents member manifests from silently
diverging and makes dependency ownership easier to audit. This is **small to
medium effort**, with most risk in dependency API migrations rather than the
manifest restructuring itself. It is tracked by **TASK-21, “Modernize and
deduplicate the dependency graph.”**

### Keep the present directories and package boundaries

Do not move members under `crates/`, convert the root into a virtual workspace,
or split UCI, search, perft, and browser UI into additional packages now.
Those layouts can be idiomatic in a larger repository, but they do not create
an architectural boundary by themselves. The current components ship together,
share engine-domain behavior, and have no demonstrated need for independent
versioning, feature selection, or reuse. Such a move would touch paths, build
metadata, documentation, and tooling without changing dependency direction.

This is a **no-change recommendation** with no follow-up task. Reconsider a
split only when a concrete consumer needs a component independently, compile
time or optional dependencies become a measured problem, or ownership and
release cadence diverge. At that point the new constraint can determine the
right boundary rather than directory aesthetics.

## Result

The workspace is structurally idiomatic: it uses standard Cargo target
locations, has understandable package responsibilities, and maintains an
acyclic layering. Complete TASK-20 and TASK-21 to address the two substantive
families of deviation. No broader workspace reorganisation is justified by the
current repository.
