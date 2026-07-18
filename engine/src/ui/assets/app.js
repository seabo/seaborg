// Placeholder client for the local Seaborg UI.
//
// It exercises the whole protocol — snapshot GET, command POST, and the Server-Sent Events
// stream — while rendering state as text. TASK-1.4 replaces this with the interactive board.

const token = document
  .querySelector('meta[name="seaborg-token"]')
  .getAttribute("content");

/** The latest authoritative snapshot, and the revision commands are based on. */
let snapshot = null;

const elements = {
  status: document.getElementById("status"),
  fen: document.getElementById("fen"),
  sideToMove: document.getElementById("side-to-move"),
  gameStatus: document.getElementById("game-status"),
  engineStatus: document.getElementById("engine-status"),
  history: document.getElementById("history"),
  moveError: document.getElementById("move-error"),
  uci: document.getElementById("uci"),
};

function describeGame(status) {
  if (status.kind === "checkmate") return `checkmate — ${status.winner} wins`;
  if (status.kind === "draw") return `draw — ${status.reason.replace(/_/g, " ")}`;
  return "ongoing";
}

function describeEngine(status) {
  if (status.kind !== "thinking") return "idle";
  if (!status.progress) return "thinking…";
  const { depth, score, nodes } = status.progress;
  const evaluation =
    score.kind === "mate" ? `mate in ${score.moves}` : `${score.centipawns} cp`;
  return `thinking — depth ${depth}, ${evaluation}, ${nodes} nodes`;
}

function render(next) {
  snapshot = next;
  elements.fen.textContent = next.fen;
  elements.sideToMove.textContent = next.sideToMove;
  elements.gameStatus.textContent = describeGame(next.gameStatus);
  elements.engineStatus.textContent = describeEngine(next.engineStatus);

  elements.history.replaceChildren(
    ...next.moveHistory.map((record) => {
      const item = document.createElement("li");
      item.textContent = `${record.san} (${record.uci})`;
      return item;
    }),
  );
}

async function send(path, body) {
  elements.moveError.textContent = "";
  const response = await fetch(path, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Seaborg-Token": token,
    },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    const failure = await response.json().catch(() => ({ error: "request_failed" }));
    elements.moveError.textContent = failure.error.replace(/_/g, " ");
    return false;
  }
  render(await response.json());
  return true;
}

document.getElementById("move-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const uci = elements.uci.value.trim();
  if (uci && (await send("/api/move", { uci, revision: snapshot.revision }))) {
    elements.uci.value = "";
  }
});

document.getElementById("undo").addEventListener("click", () => {
  send("/api/undo", { revision: snapshot.revision });
});

document.getElementById("new-white").addEventListener("click", () => {
  send("/api/new-game", { humanSide: "white" });
});

document.getElementById("new-black").addEventListener("click", () => {
  send("/api/new-game", { humanSide: "black" });
});

// EventSource reconnects on its own and replays `Last-Event-ID`, so the stream survives a
// reload or a dropped connection without any client-side retry logic.
const events = new EventSource("/api/events");
events.addEventListener("open", () => {
  elements.status.textContent = "Connected";
});
events.addEventListener("message", (event) => {
  elements.status.textContent = "Connected";
  render(JSON.parse(event.data));
});
events.addEventListener("error", () => {
  elements.status.textContent = "Reconnecting…";
});
