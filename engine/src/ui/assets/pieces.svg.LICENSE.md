# Chess piece artwork: cburnett

The twelve chess piece symbols in `pieces.svg` are the "cburnett" set by
**Colin M.L. Burnett**, published on Wikimedia Commons on 2006-12-27 as the
user `Cburnett`.

Seaborg embeds this artwork in its executable and serves it from the local UI
server. This file is the notice that obligation requires. It is reproduced in
the built binary and reachable without the source tree: run `seaborg
--licenses`, or open the "Piece artwork" link in the browser UI.

## The elected license

Each source file is multi-licensed by its author, whose Commons file pages all
carry the licensing template `{{self|GFDL|migration=relicense|BSD|GPL}}`. That
offers the recipient four alternatives, of which any one may be chosen:

1. GNU Free Documentation License 1.2 or later
2. Creative Commons Attribution-ShareAlike 3.0 Unported
3. BSD 3-clause License
4. GNU General Public License 2 or later

**Seaborg elects the BSD 3-clause License.** It is permissive and imposes no
copyleft or share-alike obligation, so it is compatible with Seaborg's own MIT
license; it asks only that this notice reach whoever receives the distribution.
Electing it does not restrict anyone else, who remains free to take the same
artwork from Commons under any of the four.

Note that the machine-readable license field Commons exposes through its API
reports only CC BY-SA 3.0. That field records a single license and cannot
express the multi-license; the file pages themselves are authoritative, and they
offer all four.

Do not re-source these files from a downstream project that bundles the same
artwork. Those projects have already made their own election — Lichess, for
instance, declares its bundled copy GPLv2+ — and taking the files from them
inherits that choice and forfeits the BSD election recorded here.

## BSD 3-clause License

Copyright (c) 2006, Colin M.L. Burnett
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Provenance

Every file was downloaded from Wikimedia Commons directly, through
`https://commons.wikimedia.org/wiki/Special:FilePath/<file>`, which resolves to
the current original upload. The SHA-256 of each file as retrieved is recorded
below so the import can be re-derived and checked.

| Symbol         | Commons file       | SHA-256 of the source file                                         |
| -------------- | ------------------ | ------------------------------------------------------------------ |
| `white-pawn`   | `Chess_plt45.svg`  | `cc7de30708dcec8f4d593a89d10893d5f9c063682039a1c441e86c44cf2096db` |
| `black-pawn`   | `Chess_pdt45.svg`  | `4413bf7c18a341f9723d97e6f92c985e30b6167b037e80842cea59b7541bb074` |
| `white-rook`   | `Chess_rlt45.svg`  | `4d42ab45afd862c704eb9b35317102d453a7a6b9b71d40f18958c8eadc829e4b` |
| `black-rook`   | `Chess_rdt45.svg`  | `6abf617a9e26902e0734d85897c9ca55e29d7be2928142aa21032c38967e34ba` |
| `white-knight` | `Chess_nlt45.svg`  | `5486791207156f7ae8b8678187648df45085d726334c2862e73b077dea00641e` |
| `black-knight` | `Chess_ndt45.svg`  | `735cc58315b123a56632d4877a6b976c827481fa97bf9a5c8f459ec969bc2549` |
| `white-bishop` | `Chess_blt45.svg`  | `1d7beace24d455c923ee80d27125963eaf0287b956c5576dbf790c97ac0b97eb` |
| `black-bishop` | `Chess_bdt45.svg`  | `ba67da76ce919addc60ecb8b46801def073dd54149b2c038a2d07a16d904d5e4` |
| `white-queen`  | `Chess_qlt45.svg`  | `b72b864e2a5b6c8f8afb7f260130c10e649ff063f4ef58190c00a35c56364327` |
| `black-queen`  | `Chess_qdt45.svg`  | `70191a3fbc729ef629661e2419a66ab8024c49277aab8ccae3a5ef61372ab802` |
| `white-king`   | `Chess_klt45.svg`  | `56f55c784843b1ac272b8745d740aa2a3e6c585513ef889978916f88e5d0b70b` |
| `black-king`   | `Chess_kdt45.svg`  | `025eea92e0ef8eb1fd06b1c58d0d112948f08bf66cea6b5d003659569949b41c` |

## What was changed

The artwork itself is unmodified. Assembling the twelve files into one sprite
required only mechanical edits, applied uniformly:

- Each file's XML declaration and `DOCTYPE` were dropped, and its root `<svg>`
  element was replaced by a `<symbol>` carrying the original's `0 0 45 45`
  coordinate system.
- Inkscape's generated element `id` attributes were removed, since ids inside a
  shared sprite must not collide.
- Attributes spread over several lines in the sources were joined onto one line.

No path data, fill, stroke, or transform was altered.
