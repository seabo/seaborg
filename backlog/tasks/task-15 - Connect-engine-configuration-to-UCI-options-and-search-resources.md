---
id: TASK-15
title: Connect engine configuration to UCI options and search resources
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-17 17:14'
updated_date: '2026-07-20 18:04'
labels:
  - engine
  - uci
  - configuration
dependencies:
  - TASK-57
references:
  - engine/src/options.rs
  - engine/src/engine.rs
  - engine/src/search.rs
  - engine/src/uci.rs
  - tools/strength/strength_test.py
  - README.md
priority: high
type: enhancement
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Engine resource configuration is split between an unused Config type and ad hoc state in the UCI driver. Hash is currently the only advertised option; the search worker count is fixed at one. Establish one authoritative configuration owner and truthful, validated UCI resource options so the Lazy SMP programme can change hash and worker resources only at safe lifecycle boundaries.

This task owns the configuration model, validation, and quiescent application semantics. It does not itself need to spawn multiple search workers: the Lazy SMP search-team tasks consume this foundation. The Threads option must not be advertised before multiple workers are real, but the configuration design must accommodate it without another ownership rewrite.

Concurrency boundary. An active search owns Arc clones of the shared transposition table. Hash replacement, physical clearing, and any worker-resource rebuild must occur only after the complete search team has been cancelled and joined. The existing SearchEngine::clear_hash Arc::get_mut boundary and SearchHandle join-on-drop behavior are invariants to preserve.

Truthfulness boundary. The UCI handshake must advertise exactly what the running engine applies. The repository strength tool already sends Hash and Threads options, so it must remain possible to run against builds that do not yet advertise Threads and then use it once Lazy SMP lands.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 One authoritative runtime configuration owns all advertised engine resource settings; obsolete or disconnected configuration types are removed or integrated
- [ ] #2 The UCI handshake advertises exactly the configurable options implemented by that build, with documented defaults and bounds
- [ ] #3 setoption validates values and applies resource changes only after any active search or search team has been cancelled and fully joined
- [ ] #4 Hash controls the actual transposition-table allocation, and allocation failure or an unsupported size is handled without leaving configuration and resources inconsistent
- [ ] #5 The configuration model supports a worker count, but Threads is not advertised until a real multi-worker search consumes it
- [ ] #6 Hash replacement, clearing, and worker-resource rebuilds occur only at an owner-controlled quiescent boundary where no worker holds the old table allocation
- [ ] #7 Tests cover defaults, handshake truthfulness, valid and invalid values, repeated changes, and changes while a search is active
- [ ] #8 Strength tooling and documentation accurately describe which options are required and do not claim unsupported Lazy SMP behavior
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Replace the dead Config/HashConfig/MoveOrderingConfig/HaltingConfig types in engine/src/options.rs with one authoritative EngineConfig owning hash_mb, threads (worker count), and debug. Centralise Hash default/min/max and Threads default/min/max as associated constants (single source of truth); keep the live EngineOpt enum.
2. Make the constants the sole source of the advertised handshake, parser validation, and config validation: add EngineConfig::validate_hash_mb + set_hash_mb (Result), an advertised_uci_options() renderer, and a Display for the 'config' command. Threads stays unadvertised (max 1) with a comment explaining it awaits real multi-worker search.
3. uci.rs parse_hash validates via the shared constant range (preserving the existing reject-0/1025 behaviour). Threads remains an unadvertised, tolerated InvalidOption.
4. engine.rs drive(): own one EngineConfig; on setoption Hash, stop+join the active search then apply at the quiescent boundary via a new SearchEngine::set_hash_size that builds the new Table first and asserts the old allocation is unshared (Arc::get_mut) before swapping (construct-then-commit, no inconsistency); DebugMode no longer needlessly stops the search; wire the 'config' command to display EngineConfig; derive the uci handshake option line from advertised_uci_options().
5. search.rs: add SearchEngine::set_hash_size enforcing the owner-controlled quiescent rebuild alongside clear_hash.
6. Docs/tooling (AC#8): move README 'LazySMP multithreading' from Features to Future features (single worker today); add a UCI options note documenting Hash bounds and that Threads is not yet advertised; clarify strength_test.py --threads help that seaborg is single-worker and tolerates Threads for forward-compat.
7. Tests (AC#7): EngineConfig unit tests (defaults, valid/invalid/repeated hash, bounds, threads); driver tests for handshake truthfulness (already present, keep exact), setoption while a search is active, repeated hash changes, and that resize happens at a quiescent boundary. Run fmt/clippy/test.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Established EngineConfig (engine/src/options.rs) as the single authoritative owner of hash size, worker count, and debug mode, replacing the dead Config/HashConfig/MoveOrderingConfig/HaltingConfig types. Bounds live as associated constants read by three call sites — advertised_uci_options(), the uci.rs parser, and the config's own validate/set methods — so the advertised range is by construction the accepted range.

Hash application (engine.rs drive): on setoption Hash the driver stops+joins the active search, updates the config, then reallocates via the new SearchEngine::set_hash_size (search.rs). That method builds the replacement Table before touching the live one and asserts exclusivity through Arc::get_mut, the same quiescent-boundary guard as clear_hash, so a resize can never replace an allocation a worker still holds and a rejected value leaves config and table untouched. DebugMode no longer stops a running search. The custom 'config' command now displays the config.

Threads: modelled as a validated worker count (default 1, THREADS_MAX 1) but intentionally not advertised while the search is single-worker; the parser still tolerates an unknown option on the diagnostic channel, so the strength harness that always sends Threads keeps working.

AC#8 docs/tooling: README moved 'LazySMP multithreading' from Features to Future features and added a UCI options section; tools/strength/strength_test.py --threads help now explains the single-worker tolerance.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-20 18:04
---
Implementation handoff
Branch: task-15-engine-config-uci-options
Worktree: /Users/seabo/seaborg-worktrees/task-15-engine-config-uci-options
Base: ba6aec1d2d2633c672e9945d52864fb09c011140
Implementation target: a4a2e60b76bbed4d41abd586c77a46672d575b5e
Resolved findings: none (initial implementation)
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (clean)
- cargo test --workspace: pass (engine 300 passed / 45 passed integration incl. 8 new; workspace 0 failed)
Known failures: none
---
<!-- COMMENTS:END -->
