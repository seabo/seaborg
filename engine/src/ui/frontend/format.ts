/**
 * Pure presentation helpers for the companion panel.
 *
 * Nothing here touches the DOM or the network, so every rule the panel reads by can be unit
 * tested directly against the shapes the server publishes.
 */

export type Color = "white" | "black";

export type Score =
  | { readonly kind: "cp"; readonly centipawns: number }
  | { readonly kind: "mate"; readonly moves: number }
  | { readonly kind: "inf" }
  | { readonly kind: "-inf" };

export type EngineLimit =
  | { readonly kind: "time"; readonly milliseconds: number }
  | { readonly kind: "depth"; readonly plies: number }
  | { readonly kind: "infinite" };

/** The value a limit `<option>` carries, so the select and the command agree on one spelling. */
export function engineLimitValue(limit: EngineLimit): string {
  if (limit.kind === "time") return `time:${limit.milliseconds}`;
  if (limit.kind === "depth") return `depth:${limit.plies}`;
  return "infinite";
}

/** Split an `<option>` value back into the fields `/api/engine-limit` expects. */
export function parseEngineLimitValue(
  value: string,
): { readonly kind: string; readonly value: number } | null {
  const [kind, amount] = value.split(":");
  if (kind === undefined || amount === undefined) return null;
  const parsed = Number(amount);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) return null;
  return { kind, value: parsed };
}

/**
 * Render an engine score from White's point of view.
 *
 * The engine reports its scores relative to the side it is searching for, which is always the
 * side the human is not playing. Chess convention shows evaluations from White, so a score is
 * negated when the engine is Black. Getting this backwards would silently invert every
 * evaluation the panel shows, which is why it is a named function with its own tests.
 */
export function formatScore(score: Score, humanSide: Color): string {
  const engineIsWhite = humanSide === "black";
  if (score.kind === "inf") return engineIsWhite ? "+∞" : "−∞";
  if (score.kind === "-inf") return engineIsWhite ? "−∞" : "+∞";
  if (score.kind === "mate") {
    const moves = engineIsWhite ? score.moves : -score.moves;
    // A mate score names the side delivering it: `#3` is White mating, `−#3` is Black mating.
    return `${moves < 0 ? "−" : "+"}#${Math.abs(moves)}`;
  }
  const centipawns = engineIsWhite ? score.centipawns : -score.centipawns;
  const pawns = Math.abs(centipawns) / 100;
  return `${centipawns < 0 ? "−" : "+"}${pawns.toFixed(2)}`;
}

/** Abbreviate a node or node-rate count so the stat row keeps a stable width. */
export function formatCount(value: number): string {
  if (!Number.isFinite(value) || value < 0) return "—";
  if (value < 1_000) return String(Math.trunc(value));
  if (value < 1_000_000) return `${(value / 1_000).toFixed(value < 10_000 ? 1 : 0)}k`;
  if (value < 1_000_000_000) return `${(value / 1_000_000).toFixed(value < 10_000_000 ? 1 : 0)}M`;
  return `${(value / 1_000_000_000).toFixed(1)}B`;
}

/** Render a hash occupancy reading, which the engine reports in permille. */
export function formatHashfull(hashfull: number): string {
  if (!Number.isFinite(hashfull) || hashfull < 0) return "—";
  return `${(hashfull / 10).toFixed(hashfull < 10 ? 1 : 0)}%`;
}

/** Describe an engine limit in the words the control uses. */
export function describeEngineLimit(limit: EngineLimit): string {
  if (limit.kind === "depth") return `depth ${limit.plies}`;
  if (limit.kind === "infinite") return "unlimited";
  // A number renders without trailing zeros, so 250ms reads as `0.25s` and 1000ms as `1s`.
  return `${limit.milliseconds / 1_000}s`;
}

/** One numbered row of a scoresheet. */
export interface MovePair<T> {
  readonly number: number;
  readonly white: T | null;
  readonly black: T | null;
}

/** Number a move list the way a scoresheet does, pairing each White move with Black's reply. */
export function movePairs<T>(moves: readonly T[]): ReadonlyArray<MovePair<T>> {
  const pairs: Array<MovePair<T>> = [];
  for (let index = 0; index < moves.length; index += 2) {
    pairs.push({
      number: index / 2 + 1,
      white: moves[index] ?? null,
      black: moves[index + 1] ?? null,
    });
  }
  return pairs;
}

/**
 * Turn a server error code into a sentence that says what to do next.
 *
 * The codes are a protocol contract, but a person reading `stale_revision` learns nothing, so
 * every code the server can return on a command is spelled out here. Unknown codes fall back to
 * the code itself rather than to silence, so a protocol addition is still visible.
 */
export function describeCommandError(code: string): string {
  const messages: Readonly<Record<string, string>> = {
    stale_revision: "The board moved on before that arrived. The position shown is current.",
    not_human_turn: "It is not your turn yet — Seaborg is still thinking.",
    game_over: "This game has finished. Start a new game or undo to keep playing.",
    illegal_move: "That move is not legal in this position.",
    nothing_to_undo: "There is nothing left to undo.",
    invalid_engine_limit: "Seaborg would not accept that thinking limit.",
    missing_engine_limit: "Seaborg would not accept that thinking limit.",
    invalid_token: "This page is out of date. Reload it to reconnect.",
    too_many_connections: "Too many tabs are open. Close some and reload.",
    not_found: "Seaborg did not recognise that request.",
    method_not_allowed: "Seaborg did not recognise that request.",
    request_failed: "Seaborg could not complete that request.",
  };
  return messages[code] ?? `Seaborg refused the request (${code.replaceAll("_", " ")}).`;
}
