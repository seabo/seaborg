import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const formatSource = await readFile(new URL("../assets/format.js", import.meta.url), "utf8");
const {
  describeCommandError,
  describeEngineLimit,
  engineLimitValue,
  formatCount,
  formatHashfull,
  formatScore,
  movePairs,
  parseEngineLimitValue,
  quitEndsTheSession,
  shouldAdopt,
} = await import(`data:text/javascript;base64,${Buffer.from(formatSource).toString("base64")}`);

// Regression: `render` used to return early when a newer snapshot had already arrived, which
// skipped the whole frame rather than just the stale state. A command response landing after the
// event-stream update therefore left `Sending move…` and the disabled controls painted on screen
// until some unrelated event repainted them — reproduced in a real browser, where the UI sat
// frozen while the server was idle.
test("a stale snapshot is not adopted, but the caller is still told to paint", () => {
  assert.equal(shouldAdopt(null, { revision: 0 }), true, "the first snapshot is always adopted");
  assert.equal(shouldAdopt({ revision: 4 }, { revision: 5 }), true, "newer wins");
  assert.equal(shouldAdopt({ revision: 5 }, { revision: 3 }), false, "older is not adopted");
  // Equal revisions must still be adopted: a re-render of the current state is how a change to
  // local-only state, such as a command finishing, reaches the screen at all.
  assert.equal(shouldAdopt({ revision: 5 }, { revision: 5 }), true, "same revision repaints");
});

// Regression (REV-1-01): `quit` used to discard the command result, so any refusal branded a
// live server as stopped. That state closes the event stream and disables every control including
// the quit button, so the page could not be recovered without a reload — and it overwrote the
// accurate refusal message with "Seaborg has stopped". Both refusals below are reachable: 403
// `invalid_token` when a tab outlives a server restart, and 503 `too_many_connections` once the
// accept loop is saturated.
test("only an accepted or unanswered quit stops the session, never a refused one", () => {
  assert.equal(quitEndsTheSession("ok"), true, "an accepted quit stops the session");
  // The socket dying under the request is the ordinary end of a successful shutdown, not a fault.
  assert.equal(quitEndsTheSession("unreachable"), true, "a dropped connection stops the session");
  assert.equal(
    quitEndsTheSession("rejected"),
    false,
    "a server that answers a refusal is still running, so the page must stay usable",
  );
});

test("evaluations are shown from White regardless of which side the engine plays", () => {
  // The engine scores relative to the side it searches for, which is the side the human is not
  // playing. An advantage for the engine is an advantage for White only when the engine is White.
  const winning = { kind: "cp", centipawns: 250 };
  assert.equal(formatScore(winning, "black"), "+2.50", "engine is White and is winning");
  assert.equal(formatScore(winning, "white"), "−2.50", "engine is Black and is winning");

  const losing = { kind: "cp", centipawns: -75 };
  assert.equal(formatScore(losing, "black"), "−0.75");
  assert.equal(formatScore(losing, "white"), "+0.75");

  assert.equal(formatScore({ kind: "cp", centipawns: 0 }, "white"), "+0.00");
});

test("mate and infinite scores keep the same White-relative sign convention", () => {
  assert.equal(formatScore({ kind: "mate", moves: 3 }, "black"), "+#3", "White mates in 3");
  assert.equal(formatScore({ kind: "mate", moves: 3 }, "white"), "−#3", "Black mates in 3");
  assert.equal(formatScore({ kind: "mate", moves: -2 }, "black"), "−#2");
  assert.equal(formatScore({ kind: "mate", moves: -2 }, "white"), "+#2");

  assert.equal(formatScore({ kind: "inf" }, "black"), "+∞");
  assert.equal(formatScore({ kind: "inf" }, "white"), "−∞");
  assert.equal(formatScore({ kind: "-inf" }, "black"), "−∞");
  assert.equal(formatScore({ kind: "-inf" }, "white"), "+∞");
});

test("counts and hash occupancy abbreviate without losing the reading", () => {
  assert.equal(formatCount(0), "0");
  assert.equal(formatCount(999), "999");
  assert.equal(formatCount(1_500), "1.5k");
  assert.equal(formatCount(42_000), "42k");
  assert.equal(formatCount(1_250_000), "1.3M");
  assert.equal(formatCount(84_000_000), "84M");
  assert.equal(formatCount(2_500_000_000), "2.5B");
  assert.equal(formatCount(Number.NaN), "—");

  // A count just short of the next unit must carry into it rather than render a four-digit
  // mantissa: the stat row is sized for at most `9.9k`-style readings, and `1000k` overflows it.
  assert.equal(formatCount(999_999), "1.0M");
  assert.equal(formatCount(999_999_999), "1.0B");

  assert.equal(formatHashfull(0), "0.0%");
  assert.equal(formatHashfull(55), "6%");
  assert.equal(formatHashfull(1_000), "100%");
});

test("engine limit values round-trip through the select and the command body", () => {
  for (const limit of [
    { kind: "time", milliseconds: 250 },
    { kind: "time", milliseconds: 60_000 },
    { kind: "depth", plies: 8 },
  ]) {
    const value = engineLimitValue(limit);
    const parsed = parseEngineLimitValue(value);
    assert.equal(parsed.kind, limit.kind);
    assert.equal(parsed.value, limit.milliseconds ?? limit.plies);
  }

  assert.equal(engineLimitValue({ kind: "infinite" }), "infinite");
  // `infinite` carries no amount, so it is not a selectable option and must not parse into one.
  assert.equal(parseEngineLimitValue("infinite"), null);
  assert.equal(parseEngineLimitValue("time:"), null);
  assert.equal(parseEngineLimitValue("time:abc"), null);
  assert.equal(parseEngineLimitValue("time:0"), null);
  assert.equal(parseEngineLimitValue("time:-5"), null);
});

test("engine limits are described in the words the control uses", () => {
  assert.equal(describeEngineLimit({ kind: "time", milliseconds: 250 }), "0.25s");
  assert.equal(describeEngineLimit({ kind: "time", milliseconds: 1_000 }), "1s");
  assert.equal(describeEngineLimit({ kind: "time", milliseconds: 2_500 }), "2.5s");
  assert.equal(describeEngineLimit({ kind: "depth", plies: 6 }), "depth 6");
  assert.equal(describeEngineLimit({ kind: "infinite" }), "unlimited");
});

test("history is numbered as a scoresheet, including an unanswered White move", () => {
  assert.deepEqual(movePairs(["e4", "e5", "Nf3", "Nc6", "Bb5"]), [
    { number: 1, white: "e4", black: "e5" },
    { number: 2, white: "Nf3", black: "Nc6" },
    { number: 3, white: "Bb5", black: null },
  ]);
  assert.deepEqual(movePairs([]), []);
});

test("every command error the server can return reads as a sentence", () => {
  for (const code of [
    "stale_revision",
    "not_human_turn",
    "game_over",
    "illegal_move",
    "nothing_to_undo",
    "invalid_engine_limit",
    "invalid_token",
    "too_many_connections",
  ]) {
    const message = describeCommandError(code);
    assert.ok(message.length > 0, `${code} has a message`);
    assert.ok(!message.includes("_"), `${code} is not shown as a raw code`);
  }
  // An unrecognised code stays visible rather than being swallowed.
  assert.match(describeCommandError("some_new_code"), /some new code/u);
});
