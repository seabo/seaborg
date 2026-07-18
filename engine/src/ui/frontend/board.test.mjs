import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const boardSource = await readFile(new URL("../assets/board.js", import.meta.url), "utf8");
const {
  describeSquare,
  legalMovesFrom,
  moveTransitions,
  orderedSquares,
  parseFen,
  squareFromVisualCoordinates,
  transitionOffset,
  visualCoordinates,
} = await import(`data:text/javascript;base64,${Buffer.from(boardSource).toString("base64")}`);

test("FEN parsing places every piece without depending on orientation", () => {
  const board = parseFen("r3k2r/1P3ppp/8/3pP3/8/8/PPP3P1/R3K2R w KQkq d6 0 23");
  assert.deepEqual(board.get("a8"), { color: "black", kind: "rook", square: "a8" });
  assert.deepEqual(board.get("e8"), { color: "black", kind: "king", square: "e8" });
  assert.deepEqual(board.get("b7"), { color: "white", kind: "pawn", square: "b7" });
  assert.deepEqual(board.get("e5"), { color: "white", kind: "pawn", square: "e5" });
  assert.deepEqual(board.get("h1"), { color: "white", kind: "rook", square: "h1" });
  assert.equal(board.has("d6"), false);
});

test("FEN parsing covers all twelve piece appearances and rejects malformed ranks", () => {
  const board = parseFen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
  assert.equal(board.size, 32);
  assert.deepEqual(
    new Set([...board.values()].map((piece) => `${piece.color}-${piece.kind}`)),
    new Set([
      "white-pawn", "white-knight", "white-bishop", "white-rook", "white-queen", "white-king",
      "black-pawn", "black-knight", "black-bishop", "black-rook", "black-queen", "black-king",
    ]),
  );
  assert.throws(() => parseFen("8/8/8/8/8/8/8/7 w - - 0 1"), /eight files/u);
  assert.throws(() => parseFen("8/8/8/8/8/8/8/7x w - - 0 1"), /Invalid FEN/u);
});

test("both orientations map every square and keyboard direction consistently", () => {
  assert.deepEqual(orderedSquares("white").slice(0, 9), ["a8", "b8", "c8", "d8", "e8", "f8", "g8", "h8", "a7"]);
  assert.deepEqual(orderedSquares("black").slice(0, 9), ["h1", "g1", "f1", "e1", "d1", "c1", "b1", "a1", "h2"]);
  for (const orientation of ["white", "black"]) {
    for (const square of orderedSquares(orientation)) {
      const visual = visualCoordinates(square, orientation);
      assert.equal(squareFromVisualCoordinates(visual.column, visual.row, orientation), square);
    }
  }
  assert.deepEqual(transitionOffset("e2", "e4", "white"), { columns: 0, rows: 2 });
  assert.deepEqual(transitionOffset("e2", "e4", "black"), { columns: 0, rows: -2 });
});

test("legal move metadata distinguishes targets, captures, en passant, and promotion", () => {
  const board = parseFen("4k3/6P1/8/3pP3/8/8/8/4K3 w - d6 0 1");
  const pawnMoves = legalMovesFrom(["e5e6", "e5d6"], "e5", board);
  assert.deepEqual(pawnMoves[0], {
    uci: "e5e6", from: "e5", to: "e6", promotion: null, capture: false, captureSquare: null,
  });
  assert.deepEqual(pawnMoves[1], {
    uci: "e5d6", from: "e5", to: "d6", promotion: null, capture: true, captureSquare: "d5",
  });
  const promotions = legalMovesFrom(["g7g8q", "g7g8r", "g7g8b", "g7g8n"], "g7", board);
  assert.deepEqual(promotions.map((move) => move.promotion), ["queen", "rook", "bishop", "knight"]);
});

test("castling emits coordinated king and rook animation metadata", () => {
  const before = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
  const afterKingSide = "r3k2r/8/8/8/8/8/8/R4RK1 b kq - 1 1";
  assert.deepEqual(moveTransitions(before, afterKingSide, "e1g1"), [
    { from: "e1", to: "g1", role: "piece", captureSquare: null },
    { from: "h1", to: "f1", role: "castle-rook", captureSquare: null },
  ]);
  const afterQueenSide = "r3k2r/8/8/8/8/8/8/2KR3R b kq - 1 1";
  assert.deepEqual(moveTransitions(before, afterQueenSide, "e1c1"), [
    { from: "e1", to: "c1", role: "piece", captureSquare: null },
    { from: "a1", to: "d1", role: "castle-rook", captureSquare: null },
  ]);
});

test("en passant transition removes the pawn from its actual capture square", () => {
  const before = "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1";
  const after = "4k3/8/3P4/8/8/8/8/4K3 b - - 0 1";
  assert.deepEqual(moveTransitions(before, after, "e5d6"), [
    { from: "e5", to: "d6", role: "piece", captureSquare: "d5" },
  ]);
});

test("assistive labels name pieces and empty squares", () => {
  const board = parseFen("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
  assert.equal(describeSquare("e1", board.get("e1")), "E 1, white king");
  assert.equal(describeSquare("a4", board.get("a4")), "A 4, empty");
});
