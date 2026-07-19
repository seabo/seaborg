---
id: TASK-62
title: Adopt the cburnett piece set under its BSD 3-clause option
status: Ready to Merge
assignee:
  - '@claude'
created_date: '2026-07-19 01:27'
updated_date: '2026-07-19 14:51'
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
- [x] #1 The browser UI renders all twelve piece types in both colors using cburnett artwork, in both board orientations
- [x] #2 The twelve source SVGs are obtained from Wikimedia Commons originals, and the repository record makes that provenance explicit and traceable
- [x] #3 A third-party attribution file exists that names Colin M.L. Burnett, states that the BSD 3-clause option was elected from the multi-license, and reproduces the BSD 3-clause notice, conditions, and disclaimer
- [x] #4 The attribution is reachable by a user of the built executable, not only by a reader of the source tree
- [x] #5 Piece artwork remains embedded in the executable and served from loopback, with no runtime network request, CDN, or third-party JavaScript introduced
- [x] #6 A real-browser run shows no Content Security Policy violation or console error arising from the new artwork
- [x] #7 Board geometry is unregressed: all 64 squares remain equal and square at desktop and narrow viewports, matching the rigid-grid guarantee established by TASK-1.4 HUMAN-2
- [x] #8 Existing frontend model tests and the repository-required Rust checks pass, and any committed JavaScript remains byte-identical unless the task deliberately changes it
- [x] #9 The white and black pieces are visually distinguishable under the chosen coloring model, and the drag, arrival, capture, and snapback animations still render correctly
- [x] #10 Implementation notes state which coloring model was chosen and whether the promotion chooser moved off Unicode glyphs, with the reasoning for each
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Download the twelve cburnett originals from Wikimedia Commons (Special:FilePath/Chess_<piece><color>t45.svg) into a scratch dir; record each source URL and SHA-256 so provenance is traceable and not inherited from any downstream bundle.
2. Rebuild engine/src/ui/assets/pieces.svg from those files: wrap each source <g> in <symbol id="<color>-<kind>" viewBox="0 0 45 45">, preserving cburnett fills and strokes verbatim; keep the twelve existing symbol ids exactly so pieceAssetId interpolation and the committed JS are untouched. Drop the old <defs><style> .body/.detail contract, which the new artwork does not use.
3. Coloring model: keep the artwork's baked colors and retire the CSS coloring layer. Remove .piece color, .piece-white/.piece-black color, and the .piece-white use / .piece-black use stroke rules from style.css; keep sizing, drop-shadow, and every drag/arrival/capture/snapback class.
4. Add engine/src/ui/assets/pieces.svg.LICENSE.md following the tools/strength/openings-v1.LICENSE.md precedent: names Colin M.L. Burnett, states the BSD 3-clause election from the Commons multi-license, reproduces the full BSD 3-clause notice/conditions/disclaimer, and lists per-file source URLs with checksums.
5. Make the notice reach a user of the built binary two ways: export it as a pub const from the engine ui module, serve it at GET /licenses as text/plain with a footer link on the page (new const + route arm + method-not-allowed entry), and add a --licenses flag to the CLI that prints it and exits.
6. Extend engine/src/ui/tests.rs: /licenses content type and body markers, POST /licenses is 405, and the sprite defines all twelve <symbol> ids.
7. Real-browser run against the loopback UI: confirm no CSP violation or console error, all twelve pieces render in both orientations, 64 squares stay equal and square at desktop and narrow widths, and the animations still play.
8. Run cargo fmt --check, strict clippy, cargo test --workspace, the node frontend tests, and confirm engine/src/ui/assets/*.js is byte-identical.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Implementation

Replaced the twelve hand-drawn sprite symbols with the cburnett set by Colin M.L. Burnett, sourced from the Wikimedia Commons originals.

### Provenance and the license election

Each of the twelve files was downloaded from `https://commons.wikimedia.org/wiki/Special:FilePath/Chess_<piece><color>t45.svg`, never from a downstream bundle. The BSD election was verified rather than assumed: every one of the twelve Commons file pages carries the licensing template `{{self|GFDL|migration=relicense|BSD|GPL}}`, dated 2006-12-27, author `Cburnett`, and Commons' `{{BSD}}` template defaults to the 3-clause variant. The author's real name is confirmed from the linked en.wikipedia user page ('User page of Colin M.L. Burnett').

One finding worth recording: the Commons API's machine-readable license field (`extmetadata.LicenseShortName`) reports **only CC BY-SA 3.0**. That field holds a single license and cannot express a multi-license, so it would have made the BSD option look unavailable. The file-page wikitext is authoritative. This is noted in the attribution file so nobody re-derives the wrong conclusion from the API later.

Per-file SHA-256 checksums are recorded in `engine/src/ui/assets/pieces.svg.LICENSE.md` so the import is re-derivable.

### Coloring model chosen: keep the artwork's baked colors, retire the CSS layer

The old sprite drew white and black from **identical geometry**, separated purely by CSS via `currentColor` and `stroke: inherit`. cburnett draws them as genuinely different artwork — the black knight, king, queen, and rook carry white interior detail strokes, and the black bishop's cross is white where the white bishop's is black. No single inherited stroke colour can express that.

Re-parameterizing onto the old `.body`/`.detail` contract would therefore mean repainting third-party artwork rather than styling it: it would discard the detail lines that make the black pieces legible, and it would break the byte-for-byte provenance the checksums record. So the artwork's own fills and strokes stand, and `.piece { color }`, `.piece-white`/`.piece-black` colour, and the `.piece-white use`/`.piece-black use` stroke rules were removed.

`.piece-white` and `.piece-black` are still emitted by `createPiece` and remain as hooks, deliberately carrying no colour. Sizing, the drop shadow, and every animation class are untouched. A comment in `style.css` states why nothing colours the artwork, so the removal does not read as an oversight.

### Promotion chooser: kept on Unicode glyphs

It stays on ♛ ♜ ♝ ♞. The chooser is side-agnostic — it offers the same four pieces whichever colour is promoting, and the glyphs inherit the dialog's text colour, so they stay legible against the panel treatment. cburnett artwork has colour baked in, so putting it there would force an arbitrary fixed side (a white queen shown to a promoting Black player) or require threading the promoting colour through `app.ts`, regenerating the committed JavaScript for a purely cosmetic gain in a transient dialog. Neither is worth it.

### Attribution routes

`engine/src/ui/assets/pieces.svg.LICENSE.md` sits beside the asset, following the `tools/strength/openings-v1.LICENSE.md` precedent. Because a source-tree file does not reach someone who only runs the binary, it is `include_str!`ed and exposed two ways: served at `GET /licenses` as `text/plain` and linked from the page footer, and printed by a new `seaborg --licenses` flag (in the mode `ArgGroup`, so it is mutually exclusive with the other run modes).

Adding the route required the three coordinated edits the task described: the const, the `("GET", "/licenses")` arm, and the method-not-allowed list.

## Verification

Required checks and the frontend suite all pass; the committed JavaScript is byte-identical (no `.ts` file changed).

Real-browser run against `--ui --ui-port 7862`, driven through the loopback page:

- All twelve piece types resolve to non-zero `getBBox()` geometry — 32/32 pieces render, and white and black report different bounding boxes for king, knight, and queen, which is the extra detail geometry the two-colour artwork carries.
- **No CSP violation and no console error.** A `securitypolicyviolation` listener plus a freshly injected `<use href="/pieces.svg#white-king">` under the live policy recorded nothing; `/pieces.svg`, `/licenses`, and `/style.css` all fetch 200. The sprite no longer carries an inline `<defs><style>` block, so the change reduces rather than adds CSP exposure. The CSP was not loosened.
- **No off-origin request.** Every `performance` resource entry is same-origin.
- **Geometry unregressed.** Desktop: 64 squares, all 77.12/77.13 px, width equal to height. Narrow (390×844): 64 squares all 43.54/43.55 px, board 348.3×348.3, zero horizontal overflow. Both orientations checked — flipping to 'Black at the bottom' keeps 32/32 rendering and all squares equal and square.
- **Animations.** Playing 1.e4 a6 2.Bxa6 bxa6 through the real UI, `piece-arrive` was observed live on both the white and the black pawn, and `piece-captured` on the captured black pawn and the recaptured white bishop, fading opacity 1 → 0 — each with the cburnett artwork attached. The drag ghost (`.drag-piece`) renders `#white-pawn` during a drag.

  Snapback was **not** observed end-to-end: it is only reachable through a drag gesture, and injected pointer events cannot drive it because `setPointerCapture` throws `NotFoundError` for a synthetic pointer id. What was verified instead is that a reconstructed `.snapback-piece` ghost still resolves to `animation-name: piece-snapback` at 0.23s with the artwork painting inside it, and that `animateSnapback` builds its ghost through the same `createPiece` call already proven by the drag ghost. The keyframes were not modified. A human should confirm snapback visually via `docs/browser-ui-manual-checks.md` §5.

Also note that frame-by-frame animation playback was not measurable in the automation tab, which renders hidden; what is asserted above is that the correct animation is applied to elements carrying the new artwork, not that playback was timed.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: @claude
created: 2026-07-19 14:42
---
Implementation handoff
Branch: task-62-cburnett-pieces
Worktree: /Users/seabo/seaborg-worktrees/task-62-cburnett-pieces
Base: cb9d36a14365103ab22ccf0e4389c73550d3973d
Implementation target: 950b626
Resolved findings: none
Verification:
- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass (exit 0, no warnings)
- cargo test --workspace: pass (237 + 43 + 5 + 1 tests, 0 failed, 2 ignored)
- node --test engine/src/ui/frontend/board.test.mjs engine/src/ui/frontend/format.test.mjs: pass (16/16)
- git diff --exit-code engine/src/ui/assets/{app,board,format}.js: clean (committed JavaScript byte-identical; no .ts file changed)
- real browser against --ui --ui-port 7862: 12/12 symbols render, 32/32 pieces, no CSP violation, no console error, no off-origin request, 64 equal square cells at 1066px and at 390x844, both orientations, piece-arrive and piece-captured observed live during real moves
Known failures: none

Reviewer notes:
- master advanced to 7449461 during implementation (an unrelated game.rs refactor). This branch is based on cb9d36a and does not include it.
- Snapback is the one acceptance-criterion element not observed end-to-end: injected pointer events cannot drive a drag because setPointerCapture rejects synthetic pointer ids. The CSS and the shared createPiece path were verified instead; see the implementation notes for exactly what was and was not observed. docs/browser-ui-manual-checks.md section 5 covers it for a human.
- The BSD election was verified against the Commons file-page wikitext for all twelve files, not the API metadata field, which reports only CC BY-SA 3.0 and cannot express a multi-license. That caveat is recorded in the attribution file.
---

author: @claude
created: 2026-07-19 14:50
---
Review verdict: APPROVED

Implementation target: 950b626 (immutable; ed0c4b7 adds handoff metadata only, no implementation file changed between them)
Base: cb9d36a
Branch/worktree: task-62-cburnett-pieces @ /Users/seabo/seaborg-worktrees/task-62-cburnett-pieces

Reviewed the full cb9d36a..950b626 diff. All ten acceptance criteria are proven by objective evidence. No blocking findings.

## Independent provenance verification (AC #2, #3)

I did not take the provenance record on trust. I downloaded all twelve files fresh from 'https://commons.wikimedia.org/wiki/Special:FilePath/Chess_<piece><colour>t45.svg' and:

- All twelve SHA-256 checksums reproduce the values recorded in pieces.svg.LICENSE.md exactly.
- Every path 'd' attribute in each sprite symbol is byte-for-byte the original's, confirming the 'no path data, fill, stroke, or transform was altered' claim.

The BSD election was verified rather than assumed. The Commons file-page wikitext for Chess_klt45, Chess_qdt45, and Chess_ndt45 all carry '{{self|GFDL|migration=relicense|BSD|GPL}}' dated 2006-12-27, author {{U|Cburnett}}. Commons Template:BSD defaults 'version' to 3 and categorises an unversioned tag as '3-clause BSD License', so electing BSD from that tag is an election of the 3-clause variant. The implementation notes' warning about 'extmetadata.LicenseShortName' reporting only CC BY-SA 3.0 is correct and worth having recorded.

## Required checks, run on the target

- cargo fmt --check: pass
- cargo clippy --workspace --all-targets --all-features -- -D warnings: pass. Re-run with a clean CARGO_TARGET_DIR (full dependency rebuild, 12.82s) since the cached run finished in 0.52s and clippy conformance is load-bearing for a diff that adds Rust code. No warnings.
- cargo test --workspace: pass, exit 0
- node --test board.test.mjs format.test.mjs: 16/16
- No '#[allow]' added anywhere in the diff.

## Independent runtime verification

Live loopback run on port 7962 against the target build:

- 'GET /licenses' -> 200, 'text/plain; charset=utf-8', 5748 bytes, full notice present
- 'POST /licenses' -> 405, so the method-not-allowed list entry is real
- Security and caching headers present on /licenses; CSP byte-identical to base and not loosened
- Page footer emits 'href="/licenses" target="_blank" rel="noopener"'
- '/pieces.svg' serves 12 '<symbol id=' elements
- 'seaborg --licenses' output is byte-identical to pieces.svg.LICENSE.md ('diff' clean)
- 'seaborg --licenses --uci' is rejected by the mode ArgGroup

Also confirmed: 'git diff cb9d36a 950b626' touches no file under engine/src/ui/frontend/ and no engine/src/ui/assets/*.js, so AC #8's byte-identical requirement holds by construction rather than by regeneration luck.

## On the snapback gap (AC #9)

The handoff is candid that snapback was not driven end-to-end, because setPointerCapture rejects synthetic pointer ids. I checked whether that leaves the criterion unproven and concluded it does not: the style.css diff adds and removes no animation-related rule at all — no @keyframes, no snapback/arrive/captured/drag selector is touched — and the removed rules were purely colour and stroke. Arrival, capture, and the drag ghost were observed live carrying the new artwork through the same createPiece path animateSnapback uses. With nothing in the diff able to reach snapback, the residual risk is negligible. docs/browser-ui-manual-checks.md §5 still covers it for a human.

## Design and comment quality

The coloring-model decision is the right one and is argued, not asserted: re-parameterising onto the old .body/.detail contract would repaint third-party artwork and destroy the byte-for-byte provenance the checksums record. Keeping .piece-white/.piece-black as deliberately colourless hooks is documented in style.css so the removal cannot read as an oversight. The promotion-chooser reasoning (side-agnostic dialog, colour baked into cburnett) is sound.

Comments were checked against the lifecycle rule: none cite a task ID, acceptance criterion, review finding, or Backlog document, and each states its reason rather than restating the code. The style.css block explains why nothing colours the artwork; the PIECE_ARTWORK_LICENSE doc explains why a source-tree file cannot discharge the obligation; both test doc comments explain the failure mode they guard.

Scope is disciplined — nothing unrelated to the artwork import appears in the diff.

## Non-blocking observation (no action required for this approval)

pieces.svg.LICENSE.md tells the reader to 'open the "Piece artwork" link in the browser UI', but the footer renders that phrase as plain lead-in text and labels the anchor 'Notice'. docs/browser-ui-manual-checks.md correctly says to follow the 'Notice' link. The notice is fully reachable, so AC #4 is met; this is only a wayfinding wording mismatch inside the notice itself, worth tidying if the file is next touched.

Verdict: approved at 950b626. Note that this branch is based on cb9d36a and does not include master's later 7449461 game.rs refactor; integration against the live primary tip is $merge's gate, not this review's.
---
<!-- COMMENTS:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Replaced the hand-drawn twelve-piece sprite with the cburnett set by Colin M.L. Burnett, taken from the Wikimedia Commons originals under the BSD 3-clause option of their multi-license, and shipped the notice that election requires.

The artwork's own fills and strokes stand and the CSS colouring layer was retired, because cburnett draws white and black as genuinely different geometry that no single inherited stroke colour can express. The twelve symbol ids were preserved, so no TypeScript changed and the committed JavaScript is byte-identical. The promotion chooser stays on Unicode glyphs, since it is side-agnostic and cburnett has colour baked in.

Attribution reaches a binary-only recipient two ways: 'GET /licenses' served as text/plain with a footer link, and a new 'seaborg --licenses' flag in the mode ArgGroup.

Verified at 950b626: all twelve recorded SHA-256 checksums reproduced against freshly downloaded Commons originals and every path 'd' attribute confirmed verbatim; the BSD election confirmed against the Commons file-page wikitext ({{self|GFDL|migration=relicense|BSD|GPL}}) and Template:BSD, which defaults to the 3-clause variant; cargo fmt --check, cargo clippy --workspace --all-targets --all-features -- -D warnings on a clean CARGO_TARGET_DIR, and cargo test --workspace all pass; node --test frontend suite 16/16; git diff confirms no .ts or .js file changed; live loopback run confirms /licenses returns 200 text/plain with the full notice, POST /licenses returns 405, the footer links it, twelve symbols are served, and the CSP is unchanged and not loosened; --licenses output is byte-identical to the attribution file and is rejected alongside --uci.
<!-- SECTION:FINAL_SUMMARY:END -->
