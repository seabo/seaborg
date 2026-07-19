# Browser UI manual checks

The automated suite covers the protocol, the controller, and every pure frontend rule. What it
cannot cover is the part that only exists in a real browser: layout at real widths, pointer and
touch input, animation, focus order, and what the developer console says. This procedure covers
that remainder.

Run it before releasing a change that touches `engine/src/ui/assets`,
`engine/src/ui/frontend`, or any endpoint they use.

## Automated checks to run first

From the repository root:

```sh
cargo test --workspace
tsc -p engine/src/ui/frontend/tsconfig.json     # must leave the committed JS unchanged
git diff --exit-code engine/src/ui/assets       # proves the shipped JS matches its source
node --test engine/src/ui/frontend/board.test.mjs engine/src/ui/frontend/format.test.mjs
```

The compiled JavaScript is committed and embedded with `include_str!`, so a change to a `.ts`
file that is not recompiled ships stale behaviour. The `git diff --exit-code` step is what
catches that.

## Setup

```sh
cargo run --release -- --ui
```

Open the printed URL. Keep the developer console open for the whole run: **no check below
passes if the console reports an error**.

## 1. No external network requests

In the Network panel, reload with the cache disabled and confirm every request is to
`127.0.0.1` on the printed port — the page, `app.js`, `board.js`, `format.js`, `style.css`,
`pieces.svg`, `/api/state`, and `/api/events` — and nothing else. There must be no font
service, CDN, analytics, or source-map fetch. Covers AC #5.

## 2. Desktop layout, both colours

At roughly 1440×1000:

- Start a game as White. The board shows White at the bottom and accepts a move.
- Start a game as Black. Seaborg opens, the board shows Black at the bottom, and the engine
  panel fills in while it thinks.
- The companion panel shows move history in SAN, whose turn it is, the engine state, evaluation,
  depth, nodes, NPS, hash, and a principal variation — without crowding the board off screen.

Covers AC #1, #2.

## 3. Narrow layout

At 390×844 (a phone viewport, with touch emulation on):

- The board stays square and fully visible, and the panel stacks beneath it.
- A move can be played by touch drag and by tap-tap.
- The move list scrolls internally rather than stretching the page.

Covers AC #1, #2, #6.

## 4. Special moves

Play these positions out — each is easiest reached by starting a new game and steering, or by
playing on until they arise:

- **Castling**: both sides, king-side and queen-side. The rook animates with the king.
- **En passant**: the captured pawn disappears from its own square, not the destination.
- **Promotion**: the chooser opens, offers queen, rook, bishop, and knight, is reachable by
  keyboard alone, and Cancel leaves the pawn where it was.

Covers AC #6.

## 5. Rejected input and error feedback

- Drag a piece to an illegal square. It snaps back and the board message explains why.
- Play a move, then immediately try to move again while Seaborg is thinking. The board is locked
  and the status says the engine is thinking.
- Stop the server with Ctrl-C while the page is open. The connection indicator changes to
  `Reconnecting…` and the board message says the connection was lost. Restart the server on the
  same fixed port and confirm the page recovers on its own and clears the warning.

Covers AC #4.

## 6. Occupied fixed port

With the UI already running on a fixed port, start a second instance on the same port:

```sh
cargo run --release -- --ui --ui-port 7777    # in two terminals
```

The second must exit non-zero with a message naming the port and suggesting `--ui-port`. It must
not start a half-working server. Covers AC #4.

## 7. Reload during a search

Set the thinking time to 10s, play a move, and while Seaborg is thinking:

- Reload the page. The position, history, and the *same* running search reappear — the engine
  does not restart and no move is duplicated.
- Open a second tab. Both tabs show the same search and update together.

`reloading_during_a_search_reconstructs_the_game_without_duplicating_it` asserts this over the
protocol; this check confirms the page rebuilds from it correctly. Covers AC #3.

## 8. Engine limit and controls

- Change the thinking time. The next engine turn visibly takes about that long; a search already
  running keeps its old limit.
- Choose a fixed depth. The reported depth stops at that value.
- **Undo** rewinds a full turn and hands the move back to you, including while Seaborg is
  thinking.
- **Restart** starts a new game on the same side.
- **Flip board** reverses the orientation, and the arrow keys then follow what is on screen.
  Starting a new game returns the board to the side being played.

Covers AC #1, #2.

## 9. Terminal states

Play a game through to **checkmate** — the quickest reliable route is to set the thinking time
to 0.25s and play the Scholar's-mate try, or to walk your own king into a mate. Confirm:

- The result line names checkmate and who won.
- The board locks and further moves are refused with a readable message.
- Undo still works, so the game can be continued from before the mate.

Repeat for a draw if one arises (stalemate, threefold, or the fifty-move rule). Covers AC #5.

## 10. Keyboard and assistive technology

Using only the keyboard: tab to the board, move with the arrow keys, select and play a move with
Enter, and clear a selection with Escape. Every control in the panel is reachable by Tab and has
a visible focus ring. With a screen reader running, squares announce their coordinate and
occupant. Covers AC #1, #6.

## 11. Reduced motion

Turn on the operating system's reduce-motion setting (macOS: System Settings → Accessibility →
Display → Reduce motion) and reload. Moves, castling, and snapback resolve instantly rather than
sliding, and nothing is left mid-animation. Covers AC #6.

## 12. Quit

Press **Quit Seaborg**. The page reports that Seaborg has stopped and stops trying to reconnect,
and the terminal prints `Seaborg UI stopped.` and returns to the shell with exit status 0.
Covers AC #1.

## 13. Refused quit

A refused quit must not leave the page claiming a running server has stopped. Reach one by
leaving a tab open across a server restart, which invalidates its session token while the
untokened event stream reconnects:

1. Start the UI on a fixed port and open the page.
2. Stop the server with Ctrl-C and start it again on the same port.
3. Wait for the connection indicator to return to `Connected`, then press **Quit Seaborg**.

The page must say the page is out of date and to reload it — not that Seaborg has stopped. The
controls, including Quit, must stay enabled, and the server must still be running (the terminal
has not printed `Seaborg UI stopped.`). Reloading the page then makes Quit work normally.

`quit_needs_the_session_token` asserts the server half of this, and
`only an accepted or unanswered quit stops the session, never a refused one` asserts the rule the
page decides by; this check confirms the two meet correctly in a real browser. Covers AC #4.
