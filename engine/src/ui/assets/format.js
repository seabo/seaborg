/**
 * Pure presentation helpers for the companion panel.
 *
 * Nothing here touches the DOM or the network, so every rule the panel reads by can be unit
 * tested directly against the shapes the server publishes.
 */
/**
 * Decide whether an arriving snapshot replaces the one on screen.
 *
 * Command responses and the event stream travel independently, so a fast engine can publish
 * revision N+1 before the browser has read the response for N. The older snapshot must not be
 * adopted — but the arrival still has to be painted, because local state that is not part of any
 * snapshot (a command in flight, the board flip, an error message) changes without the revision
 * moving. Returning "do not paint" here would strand that change on screen: a finished command
 * would keep reading as still sending until some unrelated event happened to repaint.
 */
export function shouldAdopt(current, incoming) {
    return current === null || incoming.revision >= current.revision;
}
/**
 * Decide whether a quit attempt puts the page into its terminal stopped state.
 *
 * The stopped state is unrecoverable by design — it closes the event stream and disables every
 * control, including the quit button — so it must only be entered when the process really is
 * going away. An accepted quit obviously qualifies, and so does a request that never came back:
 * the server tearing the socket down mid-shutdown is the ordinary way a successful quit ends.
 *
 * A refusal does not. A server that answers 403 `invalid_token` (a tab left open across a restart
 * keeps reading the untokened event stream, so it looks connected while every command is refused)
 * or 503 `too_many_connections` is still running and still reachable. Branding it stopped would
 * strand the page in a false terminal state that only a reload clears, and would overwrite the
 * accurate message the refusal produced.
 */
export function quitEndsTheSession(outcome) {
    return outcome !== "rejected";
}
/** The value a limit `<option>` carries, so the select and the command agree on one spelling. */
export function engineLimitValue(limit) {
    if (limit.kind === "time")
        return `time:${limit.milliseconds}`;
    if (limit.kind === "depth")
        return `depth:${limit.plies}`;
    return "infinite";
}
/** Split an `<option>` value back into the fields `/api/engine-limit` expects. */
export function parseEngineLimitValue(value) {
    const [kind, amount] = value.split(":");
    if (kind === undefined || amount === undefined)
        return null;
    const parsed = Number(amount);
    if (!Number.isSafeInteger(parsed) || parsed <= 0)
        return null;
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
export function formatScore(score, humanSide) {
    const engineIsWhite = humanSide === "black";
    if (score.kind === "inf")
        return engineIsWhite ? "+∞" : "−∞";
    if (score.kind === "-inf")
        return engineIsWhite ? "−∞" : "+∞";
    if (score.kind === "mate") {
        const moves = engineIsWhite ? score.moves : -score.moves;
        // A mate score names the side delivering it: `#3` is White mating, `−#3` is Black mating.
        return `${moves < 0 ? "−" : "+"}#${Math.abs(moves)}`;
    }
    const centipawns = engineIsWhite ? score.centipawns : -score.centipawns;
    const pawns = Math.abs(centipawns) / 100;
    return `${centipawns < 0 ? "−" : "+"}${pawns.toFixed(2)}`;
}
/**
 * Abbreviate a node or node-rate count so the stat row keeps a stable width.
 *
 * Each unit is chosen from what the value rounds to rather than from the value itself, so 999,999
 * reads as `1.0M` rather than as the four-digit `1000k` that truncating toward the smaller unit
 * would produce.
 */
export function formatCount(value) {
    if (!Number.isFinite(value) || value < 0)
        return "—";
    if (value < 1_000)
        return String(Math.trunc(value));
    const units = [
        { scale: 1_000, suffix: "k" },
        { scale: 1_000_000, suffix: "M" },
        { scale: 1_000_000_000, suffix: "B" },
    ];
    for (const { scale, suffix } of units) {
        const scaled = value / scale;
        // One decimal below 10, none above, so the field never exceeds four characters.
        const digits = scaled < 10 ? 1 : 0;
        const rendered = scaled.toFixed(digits);
        // `1000` here means the rounding carried into the next unit, so let that unit render it.
        if (Number(rendered) < 1_000)
            return `${rendered}${suffix}`;
    }
    return `${(value / 1_000_000_000).toFixed(1)}B`;
}
/** Render a hash occupancy reading, which the engine reports in permille. */
export function formatHashfull(hashfull) {
    if (!Number.isFinite(hashfull) || hashfull < 0)
        return "—";
    return `${(hashfull / 10).toFixed(hashfull < 10 ? 1 : 0)}%`;
}
/** Describe an engine limit in the words the control uses. */
export function describeEngineLimit(limit) {
    if (limit.kind === "depth")
        return `depth ${limit.plies}`;
    if (limit.kind === "infinite")
        return "unlimited";
    // A number renders without trailing zeros, so 250ms reads as `0.25s` and 1000ms as `1s`.
    return `${limit.milliseconds / 1_000}s`;
}
/** Number a move list the way a scoresheet does, pairing each White move with Black's reply. */
export function movePairs(moves) {
    const pairs = [];
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
export function describeCommandError(code) {
    const messages = {
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
