---
id: TASK-1.4
title: Build the dependency-free interactive chessboard
status: Ready to Merge
assignee:
  - '@codex'
created_date: '2026-07-17 15:40'
updated_date: '2026-07-18 21:25'
labels: []
dependencies:
  - TASK-1.3
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
parent_task_id: TASK-1
type: task
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create the owned HTML, CSS, TypeScript, and SVG board experience that renders authoritative controller snapshots and turns mouse, touch, pen, click, and keyboard interaction into narrow move commands. Author the web app logic in TypeScript and compile it to locally served JavaScript for the browser.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 The board renders every FEN position correctly in either orientation using locally bundled assets
- [x] #2 Users can move by drag-and-drop or click-click with mouse, touch, and pen input
- [x] #3 Selection, legal destinations, captures, the previous move, check, rejected-move snapback, and engine-thinking lockout have clear visual states
- [x] #4 Castling and en passant animate correctly and promotion presents an accessible queen, rook, bishop, or knight chooser
- [x] #5 The board is responsive, keyboard operable, labelled for assistive technology, and respects reduced-motion preferences
- [x] #6 The web app source is TypeScript compiled to locally served JavaScript, and the client runtime uses no third-party JavaScript, framework, bundler, CDN, font service, or runtime network asset
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Rework after human feedback on target 9319c0d27963a721fd1c04c3d02e8cb2e8f56eb0.

1. Resolve HUMAN-2 with eight explicit equal grid tracks on both axes, zero-minimum square sizing, a focused asset regression, and real-browser rectangle measurements across occupied, sparse, stressed, desktop, and narrow layouts.
2. Record the human decision to park HUMAN-1 after confirming the Lichess default set is not MIT; retain the current locally owned piece sprite and create no follow-up work.
3. Run frontend verification and all repository-required Rust gates, then hand the new immutable target to independent review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented the dependency-free browser board as strict TypeScript compiled to committed ES modules, with a semantic responsive HTML shell, owned CSS, and a locally embedded twelve-piece SVG sprite. The client parses only FEN placement, while Rust remains authoritative for legal moves and now publishes an explicit inCheck flag. Pointer Events provide click-click and drag input across mouse, touch, and pen; a roving grid focus model provides arrow-key and Enter/Space operation. Legal target dots, capture rings, selection, previous-move and check highlights, engine/command lockout, persistent rejection feedback, and snapback are represented as explicit states. Promotion uses a modal four-piece chooser. Snapshot diffs drive ordinary, capture, castling-rook, and en-passant capture animations, with a stale-revision guard for POST/SSE races and a reduced-motion override.

Frontend verification: strict tsc passed; a clean temporary compilation matched both committed JavaScript files byte-for-byte; node --test passed 7 model tests covering FENs/all piece kinds, both orientations, keyboard coordinate mapping, ordinary/en-passant targets, all promotion choices, both castling directions, en-passant transitions, and assistive labels. A real Chrome run against the built loopback binary rendered 64 squares/32 pieces at 1440x1000 and 390x844 with no overflow, console/CSP error, or external resource; it applied click-click, keyboard, mouse-drag, pen-drag, and touch-drag moves, reconstructed Black orientation, showed invalid-drop snapback and live engine-thinking lockout, and reduced animation duration to 0.001ms under emulated prefers-reduced-motion.

Rust verification: cargo fmt --check passed; cargo clippy --workspace --all-targets --all-features -- -D warnings passed; cargo test --workspace passed 200 tests with 0 failures and 1 ignored; cargo build passed. No move-generation or search hot path changed, so benchmarks were not applicable.

Rework after human feedback:

Resolved HUMAN-2 in 9370e8f. The board now declares eight explicit minmax(0, 1fr) rows and columns, and squares have zero minimum dimensions, so intrinsic SVG or empty-row contents cannot resize a track. The focused embedded-asset regression passed. Chrome geometry verification measured all 64 squares: desktop occupied and sparse/stressed positions both stayed exactly 735.625px square with every cell 91.953125px; at 390px viewport the board stayed 348.03125px square with only normal subpixel distribution (43.5-43.515625px).

HUMAN-1 remains pending human choice. Lichess COPYING.md declares its default public/piece/cburnett set GPLv2+, not MIT, so it was not imported. Lichess declares fantasy, spatial, and celtic piece sets MIT; its separate public/images/staunton collection is also MIT except named subdirectories, but it is not the default 2D set.

HUMAN-1 disposition: the human explicitly chose to park the piece-set preference after reviewing the MIT-compatible alternatives. The current locally owned sprite remains in scope, no third-party artwork is imported, and no follow-up task is requested.

Rework verification at 9370e8f: strict TypeScript build passed and regenerated JavaScript matched committed outputs byte-for-byte; 7/7 frontend model tests passed; cargo fmt --check passed; strict workspace Clippy passed with zero warnings; cargo test --workspace passed 201 tests with zero failures and one ignored. The live Chrome geometry check remains the focused behavioral evidence for HUMAN-2.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @codex
created: 2026-07-18 19:59
---
Implementation handoff
Branch: task-1.4-interactive-chessboard
Worktree: /Users/seabo/seaborg-worktrees/task-1.4-interactive-chessboard
Base: 9c4cc1896a6f9b121412bb9c5cf86b338891367d
Implementation target: 9319c0d27963a721fd1c04c3d02e8cb2e8f56eb0
Resolved findings: none
Verification:
- tsc -p engine/src/ui/frontend/tsconfig.json: passed
- clean temporary tsc build compared with committed app.js and board.js: byte-identical
- node --test engine/src/ui/frontend/board.test.mjs: passed, 7/7
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed, zero warnings
- cargo test --workspace: passed, 200 tests, 0 failures, 1 ignored
- cargo build: passed
- Chrome against target/debug/seaborg --ui: desktop 1440x1000 and narrow 390x844 rendered without overflow; click-click, keyboard, mouse/pen/touch drag, both orientations, snapback, engine lockout, and reduced motion passed; no browser console/CSP errors or external resources
Known failures: none
---

author: @codex
created: 2026-07-18 20:11
---
Human feedback on implementation target 9319c0d27963a721fd1c04c3d02e8cb2e8f56eb0:

HUMAN-1: Replace the current custom piece artwork; the preferred Lichess default set may be used only if MIT licensed. Licence research found Lichess public/piece/cburnett is GPLv2+, not MIT, so importing it is pending a human choice of an MIT-compatible alternative.

HUMAN-2: The board rows resize according to their piece contents. The 8x8 board geometry must remain completely rigid for every position.
---

author: @codex
created: 2026-07-18 21:09
---
Human disposition for rework: park HUMAN-1 without changing artwork or creating follow-up work; proceed to review with HUMAN-2 fixed.
---

author: @codex
created: 2026-07-18 21:11
---
Implementation handoff
Branch: task-1.4-interactive-chessboard
Worktree: /Users/seabo/seaborg-worktrees/task-1.4-interactive-chessboard
Base: 9c4cc1896a6f9b121412bb9c5cf86b338891367d
Implementation target: 9370e8ff108e3ec011a981c2524f201f3949670a
Resolved findings: HUMAN-2; HUMAN-1 parked by explicit human direction
Verification:
- tsc -p engine/src/ui/frontend/tsconfig.json: passed
- clean temporary tsc build compared with committed app.js and board.js: byte-identical
- node --test engine/src/ui/frontend/board.test.mjs: passed, 7/7
- cargo fmt --check: passed
- cargo clippy --workspace --all-targets --all-features -- -D warnings: passed, zero warnings
- cargo test --workspace: passed, 201 tests, 0 failures, 1 ignored
- Chrome rigid-grid regression: desktop occupied and sparse/stressed layouts remained 735.625px square with equal 91.953125px cells; narrow board remained 348.03125px square with only subpixel track distribution
Known failures: none
---

author: @codex
created: 2026-07-18 21:25
---
Review attempt: 1\nReviewed branch: task-1.4-interactive-chessboard\nReviewed implementation: 9370e8ff108e3ec011a981c2524f201f3949670a\nVerdict: approved\n\nAll six acceptance criteria are proven by the immutable base-to-target diff, focused frontend/Rust tests, and the recorded live Chrome interaction and geometry verification. HUMAN-1 was explicitly parked by human direction; HUMAN-2 is resolved by rigid eight-track sizing and verified geometry.\n\nVerification:\n- cargo fmt --check: passed\n- clean CARGO_TARGET_DIR cargo clippy --workspace --all-targets --all-features -- -D warnings: passed, zero warnings\n- cargo test --workspace: passed, 201 tests, 0 failures, 1 ignored\n- tsc -p engine/src/ui/frontend/tsconfig.json: passed; committed JavaScript remained byte-identical\n- node --test engine/src/ui/frontend/board.test.mjs: passed, 7/7\n- recorded Chrome interaction and rigid-grid geometry run: passed at desktop and narrow viewports with mouse, touch, pen, click-click, keyboard, both orientations, snapback, lockout, reduced motion, CSP, and local-only resources
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Approved implementation 9370e8ff108e3ec011a981c2524f201f3949670a: delivers the owned dependency-free TypeScript chessboard with authoritative snapshot rendering, multimodal and keyboard move input, special-move and rejection animation, accessible promotion and board semantics, responsive rigid geometry, reduced-motion support, and locally embedded SVG assets. Verified with strict TypeScript compilation and byte-identical generated JavaScript, 7/7 frontend model tests, cargo fmt, fresh-target strict Clippy, the full workspace suite (201 passed, 1 ignored), and the recorded live Chrome interaction/geometry run.
<!-- SECTION:FINAL_SUMMARY:END -->
