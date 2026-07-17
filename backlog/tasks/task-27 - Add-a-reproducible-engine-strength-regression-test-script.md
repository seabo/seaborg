---
id: TASK-27
title: Add a reproducible engine strength-regression test script
status: In Progress
assignee:
  - '@codex'
created_date: '2026-07-17 18:54'
updated_date: '2026-07-17 19:24'
labels: []
dependencies: []
references:
  - >-
    https://official-stockfish.github.io/docs/fishtest-wiki/Creating-my-first-test.html
  - 'https://official-stockfish.github.io/docs/fishtest-wiki/Fishtest-FAQ.html'
  - >-
    https://official-stockfish.github.io/docs/stockfish-wiki/Regression-Tests.html
  - 'https://github.com/cutechess/cutechess/blob/master/docs/cutechess-cli.6'
priority: high
type: feature
ordinal: 30000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Provide a repository-owned test script that compares an immutable candidate engine build with an immutable baseline build by playing controlled, paired self-play games and applying a sequential probability ratio test (SPRT).

The script is intended to be invoked when an agent judges that a change can affect playing strength. Determining whether a change is functional or whether this test must run is explicitly outside this task: do not add path-based change detection, CI diff classification, or other automatic invocation policy.

Finite match testing cannot prove literal equality. The script must instead enforce a documented statistical non-regression contract: reject candidates for which the evidence supports a practically significant regression, accept candidates when the configured SPRT boundary provides sufficient evidence against that regression, and clearly distinguish an inconclusive or resource-capped result from a pass.

The comparison must be reproducible and auditable. Baseline and candidate identities, build configuration, runner version, hardware-relevant settings, opening-suite identity, time control, concurrency, engine options, hypotheses, error rates, game results, likelihood state, crashes and forfeits must be available in the output. The match must use paired openings with colours reversed, equal resources for both engines, a fixed and versioned opening suite, and isolation from stale engine state between games.

Use a maintained UCI tournament runner with SPRT support, such as cutechess-cli or FastChess. Pin or validate external test inputs sufficiently to prevent silent changes. Choose and document initial default hypotheses representing practical non-regression (for example H0 at -5 Elo and H1 at 0 Elo with alpha and beta of 0.05), while allowing explicit overrides for calibration and future policy changes. Defaults are a repository policy choice and must not be represented as proof that smaller regressions are impossible.

The strength test complements rather than replaces ordinary correctness tests, perft, UCI protocol tests, and formatting/workspace tests. Include a cheap preflight that validates both executables as usable UCI engines and fails immediately on startup, protocol, illegal-move, crash, timeout, or configuration errors. Preserve enough match output as artifacts or files for independent review and reruns.

The implementation should be practical on a dedicated or self-hosted machine. It must support a bounded calibration/smoke mode, but that mode must be visibly non-authoritative and must never report the authoritative strength gate as passed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A documented repository command accepts explicit immutable baseline and candidate engine executables or revisions and compares exactly those two identities; its report records both identities.
- [ ] #2 The authoritative mode runs paired self-play from a fixed, versioned opening suite, repeating every selected opening with colours reversed and assigning equal time, threads, hash, and other relevant engine resources.
- [ ] #3 The authoritative verdict is calculated with SPRT using documented default hypotheses and Type I/Type II error rates; the hypotheses and error rates are configurable and printed in every report.
- [ ] #4 Exit status and human-readable output distinguish PASS, FAIL, INCONCLUSIVE/resource limit, and INFRASTRUCTURE ERROR; only PASS has the success status intended for a merge gate.
- [ ] #5 A game-count or resource cap cannot turn an unfinished SPRT into PASS. Reaching the cap produces an inconclusive verdict and retains the accumulated statistics.
- [ ] #6 The script fails safely on missing dependencies or inputs, engine startup or UCI-handshake failures, illegal moves, crashes, time forfeits attributable to broken operation, and malformed or incomplete runner output.
- [ ] #7 The report contains the exact reproducibility inputs: engine identities and hashes, optimized build settings, tournament-runner name/version, opening-suite identity/checksum, time control, concurrency, engine options, SPRT parameters, game count, W/D/L and paired-result statistics when supported, Elo estimate/confidence information when supported, final likelihood statistic/bounds, forfeits, crashes, command/configuration, and verdict.
- [ ] #8 The opening suite and its provenance/licensing are documented and pinned in the repository or fetched with checksum verification; tests cannot silently consume a changed suite.
- [ ] #9 A bounded smoke/calibration mode exercises the complete orchestration and reporting path quickly, is labelled NON-AUTHORITATIVE in output, and cannot emit the authoritative PASS verdict or success status used by the merge gate.
- [ ] #10 Automated tests cover command construction/configuration, paired-colour setup, parameter validation, runner-result parsing, each verdict and exit-code mapping, capped/inconclusive behavior, and representative crash or malformed-output failures without requiring a full strength match.
- [ ] #11 Documentation explains the statistical guarantee and its limitations, expected compute cost, prerequisites, how to run and resume or rerun a comparison, how to preserve results, how defaults may be calibrated, and why a finite test does not prove exact equality or detect arbitrarily small regressions.
- [ ] #12 The task does not implement automatic functional-code detection, path-based invocation rules, or a policy deciding which changes require the script; invocation remains an agent judgment outside this scope.
- [ ] #13 Existing project quality gates continue to pass, including cargo fmt --check and cargo test --workspace.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Resolve REV-1-01 by routing argparse failures through the infrastructure-error verdict and exit code.
2. Recover an explicitly supplied output path on parse failure so a machine-readable error report is preserved when possible.
3. Add real entry-point regression tests for missing arguments and invalid typed values, including verdict, exit-code, and report assertions.
4. Run focused tests and repository quality gates, record the finding resolution, commit a new immutable target, and hand off for review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented a Python 3 cutechess-cli SPRT orchestrator, pinned CC0 opening suite, fail-closed UCI/runner validation, immutable JSON/log/PGN artifacts, smoke-mode gate isolation, unit fixtures, and operator/statistical documentation. No automatic change classification or invocation policy was added.

Verification found the task branch original base fails engine::tt::tests::gen_bound (assertion gen < 64); TASK-27 modifies no Rust sources. The current primary branch contains an independent fix after TASK-27's recorded base.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-17 19:07
---
Implementation handoff
Branch: task-27-strength-regression
Worktree: /Users/seabo/seaborg-worktrees/task-27-strength-regression
Base: dc8f6cef131c0cdfdb3feb3e914dd6e938c3a87d
Implementation target: 613ba0ca7a5c7f6c84b542d7cc53bdf0e64e954f
Resolved findings: none
Verification:
- python3 -m unittest discover -s tools/strength -p 'test_*.py' -v: PASS (7 tests)
- cargo fmt --check: PASS
- git diff --cached --check: PASS
- sha256sum tools/strength/openings-v1.epd: PASS (embedded checksum eca44927b4cabdaa96cb9ab24a66c54e7c7444ac1c3e28d97b4436c110c4e275)
- cargo test --workspace: FAIL on pre-existing engine::tt::tests::gen_bound assertion gen < 64
Known failures: engine::tt::tests::gen_bound fails on recorded base dc8f6ce; TASK-27 changes no Rust sources, and current primary contains an independent later fix.
---

author: @codex
created: 2026-07-17 19:08
---
Implementation handoff correction
The Implementation target in comment #1 was transcribed incorrectly.
Correct Implementation target: 613ba0c013b6c2ce14d364beecb539d56f6f28bf
All other handoff fields and verification evidence remain unchanged.
---

author: @codex
created: 2026-07-17 19:17
---
Review attempt: 1
Reviewed branch: task-27-strength-regression
Reviewed implementation: 613ba0c013b6c2ce14d364beecb539d56f6f28bf
Verdict: changes_requested

REV-1-01 [P1] CLI validation errors masquerade as an inconclusive strength result
Location: tools/strength/strength_test.py:72
Impact: Missing required arguments and invalid argparse-typed values exit directly through argparse with status 2. The documented contract reserves 2 for an actual INCONCLUSIVE match, so configuration/invocation failures can be misclassified by automation and do not print the required INFRASTRUCTURE ERROR verdict or preserve a report. This violates acceptance criteria 4, 6, and 10.
Reproduction: Run `python3 tools/strength/strength_test.py` with no arguments; it exits 2 after argparse usage output, rather than infrastructure status 3 and the human-readable infrastructure verdict.
Expected: All invalid or incomplete invocation/configuration paths map to INFRASTRUCTURE ERROR (exit 3), with clear output and report preservation where an output path is available; automated tests must exercise the real command/entry path and distinguish this from INCONCLUSIVE.

Verification:
- `python3 tools/strength/strength_test.py`: exits 2 (reproduced)
- `python3 -m unittest discover -s tools/strength -p 'test_*.py' -v`: PASS (7 tests; missing end-to-end invalid-CLI coverage)
- `cargo fmt --check`: PASS
- `cargo test --workspace`: FAIL only at pre-existing engine::tt::tests::gen_bound on recorded base
- `git diff --check dc8f6cef131c0cdfdb3feb3e914dd6e938c3a87d..613ba0c013b6c2ce14d364beecb539d56f6f28bf`: PASS
---
<!-- COMMENTS:END -->
