---
id: TASK-27
title: Add a reproducible engine strength-regression test script
status: In Review
assignee:
  - '@codex'
created_date: '2026-07-17 18:54'
updated_date: '2026-07-18 00:33'
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
Pivoted the runner from cutechess-cli to FastChess (an accepted runner per the task) so the tool is genuinely runnable in local dev and validated against a real binary.
1. Retarget build_command/parse_result/runner_version to FastChess, verified against a real FastChess v1.5.0 (1eedf82) build.
2. Add --engine-arg (both engines; use =-form for dashes, e.g. --engine-arg=-u) so seaborg's UCI mode works, and a robust interactive uci_preflight that keeps stdin open until bestmove.
3. Generalise --time-control to --limit (tc/st/depth/nodes); authoritative mode requires a time-based limit.
4. Add --match-timeout so a hung runner/engine fails closed instead of hanging the tool.
5. Replace invented cutechess fixtures with REAL captured FastChess output; guard the Illegal-PV-move false positive; add a deterministic live runner-version test against the real binary.
6. Resolve prior findings REV-3-01/02/03 (paired colour-reversal, run() PASS coverage, dead code / reserved fields) carried into the FastChess implementation.
7. File seaborg-side defects found during validation as TASK-32 (time-allocation null move) and TASK-34 (self-play deadlock, illegal PV, EOF null move); these are out of TASK-27 scope.
8. Update docs, run gates, commit an immutable target, hand off.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented a Python 3 cutechess-cli SPRT orchestrator, pinned CC0 opening suite, fail-closed UCI/runner validation, immutable JSON/log/PGN artifacts, smoke-mode gate isolation, unit fixtures, and operator/statistical documentation. No automatic change classification or invocation policy was added.

Verification found the task branch original base fails engine::tt::tests::gen_bound (assertion gen < 64); TASK-27 modifies no Rust sources. The current primary branch contains an independent fix after TASK-27's recorded base.

Resolved REV-1-01: argparse now raises InfrastructureError for missing/invalid CLI values, so the real entry point emits INFRASTRUCTURE ERROR and exits 3. When --output is recoverable, parse failures preserve report.json. Added subprocess-level regression coverage for both missing arguments and invalid typed values.

Resolved REV-2-01: infrastructure-error rendering now prints SPRT statistics only when a complete parsed result exists. Malformed, incomplete, crash-marked, and nonzero runner outcomes preserve report.json, emit INFRASTRUCTURE ERROR, and return exit 3 without a secondary exception. Added orchestration-level regression coverage for all representative failure paths.

Resolved REV-3-01: build_command now emits '-rounds max_games/2 -games 2 -repeat 2' so cutechess plays each opening twice with colours reversed (canonical fishtest paired setup). Total games still equal max_games (validated even), so cap/max_games accounting is unchanged. Added test_command_plays_each_opening_as_colour_reversed_pair asserting -games=2, -repeat=2, -rounds=max_games/2, and rounds*games==max_games. cutechess-cli is not installed in this environment; flags were verified against the cutechess-cli.6 manual and canonical fishtest usage rather than a live binary.

Resolved REV-3-02: added test_run_success_path_passes_and_records_results, which mocks setup and a PASS runner output at exit 0 and asserts exit 0, report verdict PASS, populated results (games/W/D/L) and sprt (llr/lower_bound/upper_bound). Extracted a shared run_with_mocks harness reused by the failure tests.

Resolved REV-3-03: removed the dead write_report() (run() inlines dir creation and report.json writes); documented Result.forfeits and Result.crashes as reserved fail-closed-zero counters in both the dataclass and docs/strength-testing.md, since any crash/forfeit fails closed to INFRASTRUCTURE ERROR before a Result exists.

FastChess pivot (validated against a real binary):
The orchestrator now targets FastChess (an accepted runner per the task) instead of cutechess-cli. Rationale: cutechess-cli has no brew formula and needs a Qt source build, whereas FastChess is a dependency-light single binary — the difference between the tool being runnable by reviewer agents in local dev and not. FastChess v1.7.0-alpha (commit 1eedf82, reports 1.5.0) was built and installed to ~/.local/bin, and the tool was exercised against it.

parse_result and build_command are validated against REAL captured FastChess output (Games/Wins/Losses/Draws, Ptnml(0-2), 'LLR: llr (pct%) (lower, upper) [e0, e1]', 'Finished match', and real failure phrasings). Test fixtures are real captured strings, not invented. A regression test guards FastChess's 'Illegal PV move' warning from being misread as a game failure. A live test drives the real FastChess binary's -version.

Added capabilities required to make it genuinely usable: --engine-arg (applied to both engines; use =-form for dash args, e.g. --engine-arg=-u for seaborg UCI mode); a robust interactive uci_preflight that keeps stdin open until bestmove; --limit generalising the resource budget (tc/st/depth/nodes) with authoritative mode requiring a time-based limit; --match-timeout to fail closed on a hung runner; and running the match in the artifact dir so FastChess's config.json is archived rather than left in the repo.

Real end-to-end validation uncovered a bug mocks hid: subprocess.TimeoutExpired.output is undecoded bytes even under text=True, which crashed the timeout handler; fixed to decode, with a bytes-output regression test.

Resolved REV-3-01: FastChess plays each opening as a colour-reversed pair via -rounds max_games/2 -games 2 -repeat 2 (total games == max_games); test asserts the paired configuration.
Resolved REV-3-02: added a run()-level PASS success test (mocked runner PASS output, exit 0) asserting exit 0, verdict PASS, populated results/sprt.
Resolved REV-3-03: removed dead write_report(); documented forfeits/crashes as reserved fail-closed-zero fields in code and docs.

Out of scope (filed as new tickets): seaborg cannot currently complete fast timed self-play (TASK-32: time-allocation null move / illegal move at starved TCs) and intermittently deadlocks during self-play under a runner, reports illegal PV moves, and returns bestmove 0000 on stdin EOF (TASK-34). Because of TASK-34, no automated test depends on seaborg running a search; a full seaborg match was verified manually when it did not deadlock, and the parsing paths are covered by real captured fixtures.
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

author: @codex
created: 2026-07-17 19:26
---
Implementation handoff
Branch: task-27-strength-regression
Worktree: /Users/seabo/seaborg-worktrees/task-27-strength-regression
Base: dc8f6cef131c0cdfdb3feb3e914dd6e938c3a87d
Implementation target: e044fd54e019efff8dbbd64c91d014cfb07a7605
Resolved findings: REV-1-01
Verification:
- python3 -m unittest discover -s tools/strength -p 'test_*.py' -v: PASS (9 tests)\n- python3 tools/strength/strength_test.py: PASS (emits INFRASTRUCTURE ERROR and exits 3 as required)\n- cargo fmt --check: PASS\n- git diff --check: PASS\n- cargo test --workspace: FAIL only at pre-existing engine::tt::tests::gen_bound assertion gen < 64\nKnown failures: engine::tt::tests::gen_bound fails on recorded base dc8f6ce; TASK-27 changes no Rust sources, and current primary contains an independent later fix.
---

author: @codex
created: 2026-07-17 19:32
---
Review attempt: 2
Reviewed branch: task-27-strength-regression
Reviewed implementation: e044fd54e019efff8dbbd64c91d014cfb07a7605
Verdict: changes_requested

REV-2-01 [P1] Post-setup infrastructure failures crash while printing the verdict
Location: tools/strength/strength_test.py:297
Impact: After the report has been initialized with SPRT configuration, any runner-output InfrastructureError before result fields are populated (including malformed/incomplete output, crash/forfeit detection, or a runner exceeding the cap) is caught and report.json records INFRASTRUCTURE ERROR, but the final print path treats the presence of the sprt object as proof that llr/results exist. It raises KeyError and the real process exits 1, not the documented infrastructure status 3. Automation therefore misclassifies exactly the fail-safe paths required by acceptance criteria 4 and 6, and acceptance criterion 10 lacks orchestration-level coverage of them.
Reproduction: Mock a successful setup and runner execution returning malformed output, then call strength_test.run([]). The caught InfrastructureError writes report.json, prints the infrastructure verdict, and then raises KeyError: llr at line 299. This was reproduced against e044fd54 with the runner subprocess mocked to return CompletedProcess([], 0, "malformed runner output\\n", "").
Expected: Every caught infrastructure failure completes without a secondary exception, prints the human-readable INFRASTRUCTURE ERROR verdict, preserves available artifacts/report data, and returns exit 3. Automated tests should exercise run() or the real entry point for representative post-setup malformed, incomplete, crash/forfeit, and nonzero-runner failures.

Verification:
- python3 -m unittest discover -s tools/strength -p test_*.py -v: PASS (9 tests; no post-setup run-path failure coverage)
- mocked strength_test.run([]) with malformed runner output: FAIL, raises KeyError llr after writing the infrastructure report
- cargo fmt --check: PASS
- cargo test --workspace: FAIL only at pre-existing engine::tt::tests::gen_bound on the recorded base
---

author: @codex
created: 2026-07-17 20:22
---
Implementation handoff
Branch: task-27-strength-regression
Worktree: /Users/seabo/seaborg-worktrees/task-27-strength-regression
Base: dc8f6cef131c0cdfdb3feb3e914dd6e938c3a87d
Implementation target: cdae8f24fe1c30892f2ad88923a4c6a7057f6dea
Resolved findings: REV-2-01
Verification:
- python3 -m unittest discover -s tools/strength -p 'test_*.py' -v: PASS (11 tests)\n- cargo fmt --check: PASS\n- git diff --check: PASS\n- cargo test --workspace: FAIL only at pre-existing engine::tt::tests::gen_bound assertion gen < 64\nKnown failures: engine::tt::tests::gen_bound fails on recorded base dc8f6ce; TASK-27 changes no Rust sources, and current primary contains an independent later fix.
---

author: @georgeseabridge
created: 2026-07-17 21:58
---
Review attempt: 3
Reviewed branch: task-27-strength-regression
Reviewed implementation: cdae8f24fe1c30892f2ad88923a4c6a7057f6dea
Verdict: changes_requested

REV-3-01 [P1] cutechess `-games 1 -repeat 2` cannot deliver colour-reversed paired openings
Location: tools/strength/strength_test.py build_command (-rounds/-games/-repeat)
Impact: The command builds `-rounds max_games -games 1 -repeat 2`. `-repeat 2` asks cutechess to replay each opening with colours reversed, but `-games 1` allows only one game per encounter, so the second (colour-reversed) game of each pair is never played. This contradicts docs/strength-testing.md ("each opening twice with colours reversed"), the report field `paired_colour_reversal: true`, and acceptance criterion 2 (paired self-play with colours reversed). It also makes the effective game count ambiguous versus the documented `max_games`. No test detects this because build_command is only asserted structurally.
Expected: Emit a runner invocation that genuinely plays each opening as a colour-reversed pair (conventionally `-games 2 -repeat 2` with `-rounds` counting pairs), keep the total-game/cap accounting consistent with the docs and `max_games`, and add a test asserting the paired/colour-reversal configuration. Please verify the final flags against the installed cutechess-cli version.

REV-3-02 [P2] No success-path (PASS / exit 0) test for run()
Location: tools/strength/test_strength_test.py
Impact: Tests cover verdict()/parse_result() in isolation and every run() failure path, but nothing drives run() with a mocked runner returning a PASS log and return code 0 to assert exit 0 and that report.json records verdict PASS with populated results/sprt. The happy path (report assembly, sprt.update, results serialization) is therefore untested end-to-end, and acceptance criterion 10 asks for verdict/exit-code mapping coverage. assert_run_failure is already almost the required harness.
Expected: Add a run()-level test that mocks setup and a PASS runner output at return code 0, asserting exit 0, verdict PASS in report.json, and populated results/sprt.

REV-3-03 [P3] Dead code / unused fields
Location: tools/strength/strength_test.py write_report(); Result.forfeits/Result.crashes
Impact: write_report() is defined but never called — run() inlines mkdir + report.json writes at both sites. Result.forfeits and Result.crashes always serialize as 0 because parse_result raises on failure words rather than counting them, giving a misleading impression in report.json.
Expected: Remove write_report() (or use it to de-duplicate the two write sites), and either populate or drop forfeits/crashes (or document them as reserved).

Verification:
- python3 -m unittest discover -s tools/strength -p 'test_*.py': PASS (11 tests)
- shasum -a 256 tools/strength/openings-v1.epd: PASS (matches embedded SUITE_SHA256 eca44927b4cabdaa96cb9ab24a66c54e7c7444ac1c3e28d97b4436c110c4e275)
Note: REV-1-01 and REV-2-01 confirmed resolved. The pre-existing engine::tt::tests::gen_bound failure on the recorded base is unrelated to this diff (no Rust sources changed).
---

author: @codex
created: 2026-07-17 23:31
---
Implementation handoff
Branch: task-27-strength-regression
Worktree: /Users/seabo/seaborg-worktrees/task-27-strength-regression
Base: dc8f6cef131c0cdfdb3feb3e914dd6e938c3a87d
Implementation target: 73a318213f6f4c1aa323cc95fceef554e6666ac1
Resolved findings: REV-3-01, REV-3-02, REV-3-03
Verification:
- python3 -m unittest discover -s tools/strength -p 'test_*.py': PASS (13 tests)
- cargo fmt --check: PASS
- git diff --check dc8f6ce..73a3182: PASS
- shasum -a 256 tools/strength/openings-v1.epd: PASS (matches embedded SUITE_SHA256 eca44927b4cabdaa96cb9ab24a66c54e7c7444ac1c3e28d97b4436c110c4e275)
- cargo test --workspace: FAIL only at pre-existing engine::tt::tests::gen_bound (assertion gen < 64) on recorded base
Known failures: engine::tt::tests::gen_bound fails on recorded base dc8f6ce; TASK-27 changes no Rust sources (git diff --name-only dc8f6ce..73a3182 -- '*.rs' is empty). Current primary (299f5ec) independently reworked engine/src/tt.rs after this base, replacing the debug_assert!(gen < 64) path.
Note: cutechess-cli is not installed in this environment; the paired-pairing flags were verified against the cutechess-cli.6 manual and canonical fishtest usage.
---

author: @codex
created: 2026-07-18 00:33
---
Implementation handoff
Branch: task-27-strength-regression
Worktree: /Users/seabo/seaborg-worktrees/task-27-strength-regression
Base: dc8f6cef131c0cdfdb3feb3e914dd6e938c3a87d
Implementation target: b7a8ca68decae557cd2ea3a661fc2f54ca8720f5
Resolved findings: REV-3-01, REV-3-02, REV-3-03
Runner change: retargeted from cutechess-cli to FastChess (accepted by the task). FastChess v1.7.0-alpha (commit 1eedf82) built and installed to ~/.local/bin.
Verification:
- python3 -m unittest discover -s tools/strength -p 'test_*.py': PASS (19 tests; live FastChess -version test runs, seaborg-search tests intentionally omitted)
- Real FastChess match executed and output captured; parse_result validated against real Games/Ptnml/LLR output
- --match-timeout observed firing against a hung seaborg match (fails closed to INFRASTRUCTURE ERROR + report.json), not hanging
- cargo fmt --check: PASS
- git diff --check dc8f6ce..b7a8ca6: PASS
- shasum -a 256 tools/strength/openings-v1.epd: PASS (matches embedded SUITE_SHA256)
- cargo test --workspace: FAIL only at pre-existing engine::tt::tests::gen_bound (assertion gen < 64) on recorded base
Known failures: engine::tt::tests::gen_bound fails on recorded base dc8f6ce; TASK-27 changes no Rust sources (git diff --name-only dc8f6ce..b7a8ca6 -- '*.rs' is empty). Current primary (299f5ec) independently reworked engine/src/tt.rs after this base.
Follow-up tickets filed for seaborg-side defects found during validation: TASK-32 (time-allocation null/illegal move at fast TCs) and TASK-34 (self-play deadlock, illegal PV moves, EOF null move). These are out of TASK-27 scope and block seaborg self-play, not the tool.
---
<!-- COMMENTS:END -->
