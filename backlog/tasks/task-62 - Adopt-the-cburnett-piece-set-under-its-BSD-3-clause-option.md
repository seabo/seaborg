---
id: TASK-62
title: Adopt the cburnett piece set under its BSD 3-clause option
status: To Do
assignee: []
created_date: '2026-07-19 01:27'
updated_date: '2026-07-19 01:34'
labels: []
dependencies: []
references:
  - 'https://commons.wikimedia.org/wiki/Category:SVG_chess_pieces'
  - 'https://commons.wikimedia.org/wiki/File:Chess_klt45.svg'
documentation:
  - >-
    backlog/docs/architecture/local-browser-ui/doc-1 -
    Local-browser-chess-UI-architecture.md
ordinal: 61000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the custom twelve-piece SVG sprite in the browser UI with the cburnett piece set (author Colin M.L. Burnett), sourced from Wikimedia Commons and used under the BSD 3-clause option of its multi-license, and ship the attribution that option requires.

## Why this is now unblocked

This was raised during TASK-1.4 as finding HUMAN-1 and parked. The research there concluded the set was unusable because Lichess `COPYING.md` declares its bundled `public/piece/cburnett` copy GPLv2+. That conclusion was about Lichess's redistribution, not about the artwork's own terms.

The Wikimedia Commons originals are multi-licensed and state verbatim "You may select the license of your choice", offering:

1. GFDL 1.2+
2. CC BY-SA 3.0
3. BSD 3-clause
4. GPL 2+

Confirmed on File:Chess_klt45.svg and File:Chess_qdt45.svg. The BSD 3-clause option is permissive with no copyleft or share-alike obligation, which satisfies the original human constraint that the artwork be MIT-compatible. Electing BSD requires only that the copyright notice, conditions, and disclaimer reach whoever receives the distribution.

## Provenance requirement

Download the twelve SVGs from Wikimedia Commons directly. Do NOT copy them out of Lichess or any other downstream project: inheriting a downstream bundle inherits that project's chosen license (GPLv2+ in Lichess's case) and forfeits the BSD election. Provenance must be unambiguous from the repo record.

The files follow the pattern `Chess_<piece><color>t45.svg`, where piece is one of k/q/r/b/n/p, color is l (white) or d (black), and the `t` variant is the transparent-background artwork. Twelve files total, in a 45x45 coordinate system.

## Integration shape

Assets are embedded in the executable and served from loopback; keep that model. They are declared as `include_str!` consts in `engine/src/ui/server.rs:24-28` and served from explicit route arms at `engine/src/ui/server.rs:483-507`, with `/pieces.svg` served as `image/svg+xml`. Adding any new served asset requires three coordinated edits: a new const, a new `("GET", "/path")` arm, and an addition to the method-not-allowed list at `engine/src/ui/server.rs:520-523`.

The existing sprite `engine/src/ui/assets/pieces.svg` is a single `<svg viewBox="0 0 100 100">` holding twelve `<symbol>` elements with ids `white-pawn`, `black-pawn`, ... `black-king`. The frontend builds `<use href="/pieces.svg#" + pieceAssetId(piece)>` in `createPiece` at `engine/src/ui/frontend/app.ts:85-93`, where `pieceAssetId` (`engine/src/ui/frontend/board.ts:106`) interpolates `${color}-${kind}`. That interpolation is load-bearing: symbol ids must keep matching the `white-pawn` / `black-queen` pattern exactly or pieces silently fail to render.

Preserving the sprite structure and those exact symbol ids means the artwork swap needs no TypeScript change and no regeneration of committed JavaScript. Prefer that over reshaping the asset contract.

Merging the per-piece source files into the sprite means stripping each file's XML preamble, `<metadata>`, and editor cruft, and reconciling the source 45x45 coordinate system with the sprite's `0 0 100 100` viewBox.

## The coloring model has to change

This is the substantive design problem, not a detail. The current twelve symbols are **identical geometry for white and black**, differentiated purely by CSS. `engine/src/ui/assets/style.css:245-271` sets `.piece { color: #182121 }`, `.piece-white { color: #f4f1dd }`, `.piece-black { color: #182121 }`, `.piece-white use { stroke: #25302f }`, `.piece-black use { stroke: #dce4d5 }`, and the sprite's own `<defs><style>` maps `.body { fill: currentColor; stroke: inherit }` and `.detail { fill: none; stroke: inherit }`.

cburnett ships genuinely distinct white and black artwork with fills and strokes baked into the source. `currentColor` and `stroke: inherit` will not drive it. The task must decide, deliberately, between keeping the artwork's own baked colors and retiring the CSS coloring layer, or re-parameterizing the imported artwork onto the existing `.body` / `.detail` contract. Either is defensible; leaving the two models half-merged is not. Whichever is chosen, the drag, arrival, capture, and snapback animation classes at `style.css:306-320` must still work.

## Known risks to check, not assume

- Content Security Policy is `default-src 'none'; ... img-src 'self' data:; ...` at `engine/src/ui/http.rs:277`, asserted by a test at `engine/src/ui/tests.rs:424`. The current sprite already carries an inline `<defs><style>` block and renders fine, so inline SVG styling is evidently tolerated today; treat CSP as a thing to confirm in a real browser with the console open rather than a known blocker. Do not loosen the CSP to accommodate imported artwork.
- Board geometry. TASK-1.4 finding HUMAN-2 was specifically that intrinsic SVG content resized grid tracks. That was fixed with explicit `minmax(0, 1fr)` tracks and zero-minimum squares. Artwork with a different intrinsic size must not regress it.
- Dependency-free constraint. TASK-1.4 AC #6 forbids third-party JavaScript, frameworks, bundlers, CDNs, font services, and runtime network assets. Locally embedded third-party artwork is none of those, and doc-1 explicitly contemplates embedded SVG assets. Confirm this reading holds rather than treating the constraint as blocking.
- The promotion chooser at `engine/src/ui/assets/index.html:74-77` uses Unicode glyphs (♛ ♜ ♝ ♞), not the sprite. Decide whether it should switch to cburnett symbols for visual consistency, and say which was chosen and why.

## Attribution obligation

The repo has no root LICENSE, NOTICE, or third-party attribution file, and neither the root nor member `Cargo.toml` declares a `license` field. There is no attribution infrastructure to extend; this task creates the first. The nearest precedent is `tools/strength/openings-v1.LICENSE.md`, a per-asset `<name>.LICENSE.md` sitting beside the asset it covers, which is a reasonable format to follow.

A repo file alone is likely insufficient. Seaborg ships as an executable, and BSD 3-clause requires the notice to reach the recipient of the binary form, so the notice needs a route to someone running the built binary, not only to someone reading the source tree. Choosing that route is part of this task.

The recorded attribution must name the author, state that the BSD 3-clause option of the multi-license was elected, and reproduce the license text with its conditions and disclaimer.

## Licensing of Seaborg itself

Resolved, and no longer an open question for this task. Seaborg is MIT licensed as of commit 5d37179: a root `LICENSE` file plus `license = "MIT"` on the workspace root package and both member crates.

BSD 3-clause artwork is compatible with an MIT project, since both are permissive and require only notice retention, so the import raises no conflict.

Note that this does **not** discharge the attribution obligation described above. The MIT license covers Seaborg's own code; the imported artwork carries its own separate BSD 3-clause notice requirement, which still has to be recorded and still has to reach the user of the built binary.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The browser UI renders all twelve piece types in both colors using cburnett artwork, in both board orientations
- [ ] #2 The twelve source SVGs are obtained from Wikimedia Commons originals, and the repository record makes that provenance explicit and traceable
- [ ] #3 A third-party attribution file exists that names Colin M.L. Burnett, states that the BSD 3-clause option was elected from the multi-license, and reproduces the BSD 3-clause notice, conditions, and disclaimer
- [ ] #4 The attribution is reachable by a user of the built executable, not only by a reader of the source tree
- [ ] #5 Piece artwork remains embedded in the executable and served from loopback, with no runtime network request, CDN, or third-party JavaScript introduced
- [ ] #6 A real-browser run shows no Content Security Policy violation or console error arising from the new artwork
- [ ] #7 Board geometry is unregressed: all 64 squares remain equal and square at desktop and narrow viewports, matching the rigid-grid guarantee established by TASK-1.4 HUMAN-2
- [ ] #8 Existing frontend model tests and the repository-required Rust checks pass, and any committed JavaScript remains byte-identical unless the task deliberately changes it
- [ ] #9 The white and black pieces are visually distinguishable under the chosen coloring model, and the drag, arrival, capture, and snapback animations still render correctly
- [ ] #10 Implementation notes state which coloring model was chosen and whether the promotion chooser moved off Unicode glyphs, with the reasoning for each
<!-- AC:END -->
