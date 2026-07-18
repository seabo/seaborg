export type Color = "white" | "black";
export type Orientation = Color;
export type PieceKind = "pawn" | "knight" | "bishop" | "rook" | "queen" | "king";

export interface Piece {
  readonly color: Color;
  readonly kind: PieceKind;
  readonly square: string;
}

export interface MoveOption {
  readonly uci: string;
  readonly from: string;
  readonly to: string;
  readonly promotion: PieceKind | null;
  readonly capture: boolean;
  readonly captureSquare: string | null;
}

export interface MoveTransition {
  readonly from: string;
  readonly to: string;
  readonly role: "piece" | "castle-rook";
  readonly captureSquare: string | null;
}

const files = "abcdefgh";
const ranks = "12345678";

const pieceKinds: Readonly<Record<string, PieceKind>> = {
  p: "pawn",
  n: "knight",
  b: "bishop",
  r: "rook",
  q: "queen",
  k: "king",
};

const promotionKinds: Readonly<Record<string, PieceKind>> = {
  q: "queen",
  r: "rook",
  b: "bishop",
  n: "knight",
};

export function isSquare(value: string): boolean {
  return value.length === 2 && files.includes(value[0] ?? "") && ranks.includes(value[1] ?? "");
}

/** Parse only the board field. Chess legality remains entirely on the Rust side. */
export function parseFen(fen: string): ReadonlyMap<string, Piece> {
  const boardField = fen.trim().split(/\s+/u)[0];
  if (boardField === undefined) throw new Error("FEN has no board field");
  const rows = boardField.split("/");
  if (rows.length !== 8) throw new Error("FEN board must contain eight ranks");

  const board = new Map<string, Piece>();
  rows.forEach((row, rowIndex) => {
    let fileIndex = 0;
    for (const symbol of row) {
      if (/^[1-8]$/u.test(symbol)) {
        fileIndex += Number(symbol);
        continue;
      }
      const kind = pieceKinds[symbol.toLowerCase()];
      if (kind === undefined || fileIndex >= 8) throw new Error(`Invalid FEN board symbol: ${symbol}`);
      const color: Color = symbol === symbol.toUpperCase() ? "white" : "black";
      const square = `${files[fileIndex]}${8 - rowIndex}`;
      board.set(square, { color, kind, square });
      fileIndex += 1;
    }
    if (fileIndex !== 8) throw new Error(`FEN rank ${8 - rowIndex} does not contain eight files`);
  });
  return board;
}

export function orderedSquares(orientation: Orientation): readonly string[] {
  const visibleFiles = orientation === "white" ? files : [...files].reverse().join("");
  const visibleRanks = orientation === "white" ? "87654321" : ranks;
  return [...visibleRanks].flatMap((rank) => [...visibleFiles].map((file) => `${file}${rank}`));
}

export function visualCoordinates(
  square: string,
  orientation: Orientation,
): { readonly column: number; readonly row: number } {
  if (!isSquare(square)) throw new Error(`Invalid square: ${square}`);
  const file = files.indexOf(square[0] ?? "");
  const rank = Number(square[1]);
  return orientation === "white"
    ? { column: file, row: 8 - rank }
    : { column: 7 - file, row: rank - 1 };
}

export function squareFromVisualCoordinates(
  column: number,
  row: number,
  orientation: Orientation,
): string | null {
  if (column < 0 || column > 7 || row < 0 || row > 7) return null;
  const fileIndex = orientation === "white" ? column : 7 - column;
  const rank = orientation === "white" ? 8 - row : row + 1;
  return `${files[fileIndex]}${rank}`;
}

export function pieceAssetId(piece: Pick<Piece, "color" | "kind">): string {
  return `${piece.color}-${piece.kind}`;
}

export function pieceName(piece: Pick<Piece, "color" | "kind">): string {
  return `${piece.color} ${piece.kind}`;
}

export function describeSquare(square: string, piece: Piece | undefined): string {
  const coordinate = `${square[0]?.toUpperCase()} ${square[1]}`;
  return piece === undefined ? `${coordinate}, empty` : `${coordinate}, ${pieceName(piece)}`;
}

function promotionKind(uci: string): PieceKind | null {
  const suffix = uci[4];
  return suffix === undefined ? null : (promotionKinds[suffix] ?? null);
}

export function legalMovesFrom(
  legalMoves: readonly string[],
  from: string,
  board: ReadonlyMap<string, Piece>,
): readonly MoveOption[] {
  const movingPiece = board.get(from);
  if (movingPiece === undefined) return [];
  return legalMoves
    .filter((uci) => uci.length >= 4 && uci.slice(0, 2) === from && isSquare(uci.slice(2, 4)))
    .map((uci) => {
      const to = uci.slice(2, 4);
      const occupied = board.has(to);
      const diagonalPawnMove = movingPiece.kind === "pawn" && from[0] !== to[0];
      return {
        uci,
        from,
        to,
        promotion: promotionKind(uci),
        capture: occupied || diagonalPawnMove,
        captureSquare: occupied ? to : diagonalPawnMove ? `${to[0]}${from[1]}` : null,
      };
    });
}

export function legalMovesTo(
  legalMoves: readonly string[],
  from: string,
  to: string,
  board: ReadonlyMap<string, Piece>,
): readonly MoveOption[] {
  return legalMovesFrom(legalMoves, from, board).filter((move) => move.to === to);
}

export function moveTransitions(
  previousFen: string,
  nextFen: string,
  uci: string,
): readonly MoveTransition[] {
  if (uci.length < 4) return [];
  const from = uci.slice(0, 2);
  const to = uci.slice(2, 4);
  if (!isSquare(from) || !isSquare(to)) return [];
  const before = parseFen(previousFen);
  const after = parseFen(nextFen);
  const movingPiece = before.get(from);
  if (movingPiece === undefined || !after.has(to)) return [];

  const target = before.get(to);
  const diagonalPawnMove = movingPiece.kind === "pawn" && from[0] !== to[0];
  const captureSquare = target !== undefined ? to : diagonalPawnMove ? `${to[0]}${from[1]}` : null;
  const transitions: MoveTransition[] = [{ from, to, role: "piece", captureSquare }];

  const fromFile = files.indexOf(from[0] ?? "");
  const toFile = files.indexOf(to[0] ?? "");
  if (movingPiece.kind === "king" && Math.abs(toFile - fromFile) === 2) {
    const rank = from[1];
    const kingSide = toFile > fromFile;
    const rookFrom = `${kingSide ? "h" : "a"}${rank}`;
    const rookTo = `${kingSide ? "f" : "d"}${rank}`;
    const rookBefore = before.get(rookFrom);
    const rookAfter = after.get(rookTo);
    if (rookBefore?.kind === "rook" && rookAfter?.kind === "rook") {
      transitions.push({ from: rookFrom, to: rookTo, role: "castle-rook", captureSquare: null });
    }
  }
  return transitions;
}

export function transitionOffset(
  from: string,
  to: string,
  orientation: Orientation,
): { readonly columns: number; readonly rows: number } {
  const start = visualCoordinates(from, orientation);
  const end = visualCoordinates(to, orientation);
  return { columns: start.column - end.column, rows: start.row - end.row };
}
