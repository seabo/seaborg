import { describeSquare, legalMovesFrom, legalMovesTo, moveTransitions, orderedSquares, parseFen, pieceAssetId, squareFromVisualCoordinates, transitionOffset, visualCoordinates, } from "./board.js";
import { describeCommandError, describeEngineLimit, engineLimitValue, formatCount, formatHashfull, formatScore, movePairs, parseEngineLimitValue, shouldAdopt, } from "./format.js";
function element(selector) {
    const found = document.querySelector(selector);
    if (found === null)
        throw new Error(`Missing required element: ${selector}`);
    return found;
}
const token = element('meta[name="seaborg-token"]').content;
const boardElement = element("#board");
const connectionElement = element("#connection-status");
const turnElement = element("#turn-status");
const boardMessage = element("#board-message");
const historyElement = element("#history");
const historyScroll = element("#history-scroll");
const undoButton = element("#undo");
const restartButton = element("#restart");
const flipButton = element("#flip");
const quitButton = element("#quit");
const newWhiteButton = element("#new-white");
const newBlackButton = element("#new-black");
const limitSelect = element("#engine-limit");
const engineStateElement = element("#engine-state");
const evaluationElement = element("#engine-evaluation");
const depthElement = element("#engine-depth");
const nodesElement = element("#engine-nodes");
const npsElement = element("#engine-nps");
const hashElement = element("#engine-hash");
const variationElement = element("#engine-variation");
const promotionDialog = element("#promotion-dialog");
let snapshot = null;
let selectedSquare = null;
let focusedSquare = null;
let gesture = null;
let commandPending = false;
let lastAnimatedRevision = -1;
let errorMessage = "";
// The side shown at the bottom. Null follows the side being played; flipping pins it.
let flippedOrientation = null;
// Set once the server has been told to stop, so the page stops trying to reconnect to it.
let quitting = false;
// Whether the keyboard is working inside the board, so a repaint can hand focus back to it.
let boardOwnsFocus = false;
function orientationOf(state) {
    return flippedOrientation ?? state.humanSide;
}
function createPiece(piece, className = "piece") {
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.classList.add(...className.split(" "));
    svg.classList.add(`piece-${piece.color}`);
    svg.setAttribute("viewBox", "0 0 100 100");
    svg.setAttribute("aria-hidden", "true");
    svg.setAttribute("focusable", "false");
    const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
    use.setAttribute("href", `/pieces.svg#${pieceAssetId(piece)}`);
    svg.append(use);
    return svg;
}
function canInteract(state) {
    return (!commandPending &&
        !quitting &&
        state.gameStatus.kind === "ongoing" &&
        state.engineStatus.kind === "idle" &&
        state.sideToMove === state.humanSide);
}
function describeGame(state) {
    if (quitting)
        return "Seaborg has stopped";
    if (state.gameStatus.kind === "checkmate") {
        const winner = state.gameStatus.winner;
        return `Checkmate — ${capitalize(winner)} wins (${winner === state.humanSide ? "you win" : "Seaborg wins"})`;
    }
    if (state.gameStatus.kind === "draw") {
        return `Draw — ${state.gameStatus.reason.replaceAll("_", " ")}`;
    }
    if (commandPending)
        return "Sending move…";
    if (state.engineStatus.kind === "thinking")
        return "Seaborg is thinking…";
    if (state.inCheck)
        return `${capitalize(state.sideToMove)} is in check`;
    return `${capitalize(state.sideToMove)} to move`;
}
function capitalize(value) {
    return `${value.slice(0, 1).toUpperCase()}${value.slice(1)}`;
}
function squareButton(square) {
    return boardElement.querySelector(`.square[data-square="${square}"]`);
}
function render(next) {
    // An older snapshot is not adopted, but the frame is still painted — see `shouldAdopt`. An
    // early return here would strand local state that changed without the revision moving, which
    // is how a finished command could keep reading as "Sending move…" indefinitely.
    const previous = snapshot;
    if (shouldAdopt(previous, next))
        snapshot = next;
    const state = snapshot ?? next;
    if (!canInteract(state))
        selectedSquare = null;
    const shouldAnimate = previous !== null &&
        previous.revision !== state.revision &&
        state.revision !== lastAnimatedRevision &&
        state.lastMove !== null;
    const transitions = shouldAnimate
        ? moveTransitions(previous.fen, state.fen, state.lastMove?.uci ?? "")
        : [];
    if (shouldAnimate)
        lastAnimatedRevision = state.revision;
    renderBoard(state, previous, transitions);
    renderStatus(state);
}
function renderBoard(state, previous, transitions) {
    const board = parseFen(state.fen);
    const orientation = orientationOf(state);
    const available = canInteract(state);
    const selectedMoves = selectedSquare === null ? [] : legalMovesFrom(state.legalMoves, selectedSquare, board);
    const lastFrom = state.lastMove?.uci.slice(0, 2) ?? null;
    const lastTo = state.lastMove?.uci.slice(2, 4) ?? null;
    const checkedKing = state.inCheck
        ? [...board.values()].find((piece) => piece.color === state.sideToMove && piece.kind === "king")?.square ?? null
        : null;
    const transitionByDestination = new Map(transitions.map((item) => [item.to, item]));
    boardElement.dataset.orientation = orientation;
    boardElement.setAttribute("aria-label", `Chess board, ${capitalize(orientation)} at the bottom`);
    boardElement.setAttribute("aria-busy", String(!available));
    boardElement.classList.toggle("is-locked", !available);
    rememberBoardFocus();
    boardElement.replaceChildren();
    const squares = orderedSquares(orientation);
    for (const [index, square] of squares.entries()) {
        const piece = board.get(square);
        const button = document.createElement("button");
        const coordinates = visualCoordinates(square, orientation);
        button.type = "button";
        button.className = `square ${(Number(square[0]?.charCodeAt(0)) + Number(square[1])) % 2 === 0 ? "dark" : "light"}`;
        button.dataset.square = square;
        button.setAttribute("role", "gridcell");
        button.setAttribute("aria-label", describeSquare(square, piece));
        button.setAttribute("aria-selected", String(square === selectedSquare));
        button.disabled = !available;
        button.tabIndex = square === focusedSquare || (focusedSquare === null && index === 0) ? 0 : -1;
        if (button.tabIndex === 0)
            focusedSquare = square;
        if (square === selectedSquare)
            button.classList.add("selected");
        if (square === lastFrom || square === lastTo)
            button.classList.add("last-move");
        if (square === checkedKing)
            button.classList.add("in-check");
        const target = selectedMoves.find((move) => move.to === square);
        if (target !== undefined)
            button.classList.add(target.capture ? "legal-capture" : "legal-target");
        if (coordinates.column === 0) {
            const rank = document.createElement("span");
            rank.className = "coordinate rank";
            rank.textContent = square[1] ?? "";
            button.append(rank);
        }
        if (coordinates.row === 7) {
            const file = document.createElement("span");
            file.className = "coordinate file";
            file.textContent = square[0] ?? "";
            button.append(file);
        }
        if (piece !== undefined) {
            const transition = transitionByDestination.get(square);
            if (transition !== undefined) {
                const offset = transitionOffset(transition.from, transition.to, orientation);
                button.style.setProperty("--move-x", `${offset.columns * 100}%`);
                button.style.setProperty("--move-y", `${offset.rows * 100}%`);
                button.classList.add("arriving");
            }
            button.append(createPiece(piece));
        }
        boardElement.append(button);
    }
    if (previous !== null)
        renderCapturedPieces(previous, transitions);
    restoreBoardFocus();
}
/**
 * Note whether the board is the part of the page the keyboard is working in.
 *
 * Repainting replaces every square, and while the engine thinks the replacements are disabled and
 * cannot hold focus at all. Without this, selecting a piece — or simply waiting for a reply —
 * drops a keyboard user out onto the document and makes them tab back in before every move.
 *
 * Focus landing on the body is what happens when the squares are torn out from under it, so it
 * does not count as the user leaving. Only focusing some other real control does.
 */
function rememberBoardFocus() {
    const active = document.activeElement;
    if (boardElement.contains(active)) {
        boardOwnsFocus = true;
    }
    else if (active !== null && active !== document.body && active !== document.documentElement) {
        boardOwnsFocus = false;
    }
}
/** Hand focus back to the square the keyboard was on, once a square can hold it again. */
function restoreBoardFocus() {
    if (!boardOwnsFocus || focusedSquare === null)
        return;
    const target = squareButton(focusedSquare);
    // A disabled square cannot take focus. Ownership is kept, so the next unlocking repaint
    // returns the keyboard to the board rather than stranding it on the document.
    if (target !== null && !target.disabled)
        target.focus();
}
function renderCapturedPieces(previous, transitions) {
    const before = parseFen(previous.fen);
    for (const transition of transitions) {
        if (transition.captureSquare === null)
            continue;
        const captured = before.get(transition.captureSquare);
        const square = squareButton(transition.captureSquare);
        if (captured === undefined || square === null)
            continue;
        square.append(createPiece(captured, "piece captured-piece"));
    }
}
function renderStatus(state) {
    turnElement.textContent = describeGame(state);
    turnElement.classList.toggle("thinking", state.engineStatus.kind === "thinking" && !quitting);
    boardMessage.textContent = errorMessage;
    const busy = commandPending || quitting;
    undoButton.disabled = busy || state.moveHistory.length === 0;
    restartButton.disabled = busy;
    newWhiteButton.disabled = busy;
    newBlackButton.disabled = busy;
    quitButton.disabled = busy;
    limitSelect.disabled = busy;
    flipButton.textContent = `Flip board (${capitalize(orientationOf(state))} at the bottom)`;
    renderEngineLimit(state.engineLimit);
    renderEnginePanel(state);
    renderHistory(state.moveHistory);
}
/**
 * Show the limit the server is actually using.
 *
 * The select is only rewritten when it disagrees with the server, so a snapshot arriving while
 * the menu is open does not reset what the user is looking at. A limit the menu does not offer —
 * one set from the command line — is added so the control never misreports the live setting.
 */
function renderEngineLimit(limit) {
    const value = engineLimitValue(limit);
    if (limitSelect.value === value)
        return;
    if (!Array.from(limitSelect.options).some((option) => option.value === value)) {
        const option = document.createElement("option");
        option.value = value;
        option.textContent = describeEngineLimit(limit);
        limitSelect.append(option);
    }
    limitSelect.value = value;
}
function renderEnginePanel(state) {
    const thinking = state.engineStatus.kind === "thinking";
    engineStateElement.textContent = thinking ? "Thinking" : "Idle";
    engineStateElement.classList.toggle("is-thinking", thinking);
    // A new game has nothing to report yet, and last game's figures would be read as this game's.
    if (state.moveHistory.length === 0)
        clearEnginePanel();
    if (state.engineStatus.kind !== "thinking")
        return;
    // Everything below is kept on screen after the search ends rather than blanked. The figures
    // that produced the move just played are worth reading, and the state chip already says the
    // engine is idle, so they cannot be mistaken for a search still in progress. Stats and the
    // variation persist together: clearing one and not the other would suggest the remaining half
    // was the newer of the two.
    const progress = state.engineStatus.progress;
    if (progress !== null) {
        evaluationElement.textContent = formatScore(progress.score, state.humanSide);
        depthElement.textContent = String(progress.depth);
        nodesElement.textContent = formatCount(progress.nodes);
        npsElement.textContent = formatCount(progress.nps);
        hashElement.textContent = formatHashfull(progress.hashfull);
    }
    const variation = state.engineStatus.principalVariationSan;
    if (variation.length > 0) {
        variationElement.textContent = variation.join(" ");
        variationElement.classList.remove("is-empty");
    }
}
function clearEnginePanel() {
    for (const stat of [evaluationElement, depthElement, nodesElement, npsElement, hashElement]) {
        stat.textContent = "—";
    }
    variationElement.textContent = "—";
    variationElement.classList.add("is-empty");
}
function renderHistory(moves) {
    historyElement.replaceChildren(...movePairs(moves).map((pair) => {
        const row = document.createElement("tr");
        const number = document.createElement("th");
        number.scope = "row";
        number.textContent = `${pair.number}.`;
        row.append(number, historyCell(pair.white), historyCell(pair.black));
        return row;
    }));
    // Keep the most recent move in view without stealing focus from the board.
    historyScroll.scrollTop = historyScroll.scrollHeight;
}
function historyCell(record) {
    const cell = document.createElement("td");
    if (record !== null) {
        cell.textContent = record.san;
        cell.title = record.uci;
    }
    return cell;
}
function selectSquare(square) {
    selectedSquare = square;
    focusedSquare = square ?? focusedSquare;
    if (snapshot !== null)
        render(snapshot);
}
function activateSquare(square) {
    const state = snapshot;
    if (state === null || !canInteract(state))
        return;
    const board = parseFen(state.fen);
    if (selectedSquare !== null) {
        const candidates = legalMovesTo(state.legalMoves, selectedSquare, square, board);
        if (candidates.length > 0) {
            void playCandidates(candidates);
            return;
        }
    }
    const selectable = legalMovesFrom(state.legalMoves, square, board).length > 0;
    selectSquare(selectable && selectedSquare !== square ? square : null);
}
async function choosePromotion(candidates) {
    const allowed = new Map(candidates.map((move) => [move.promotion, move]));
    for (const button of promotionDialog.querySelectorAll("[data-promotion]")) {
        const promotion = button.dataset.promotion ?? "";
        button.disabled = !allowed.has(promotion);
    }
    promotionDialog.returnValue = "";
    promotionDialog.showModal();
    await new Promise((resolve) => promotionDialog.addEventListener("close", () => resolve(), { once: true }));
    return allowed.get(promotionDialog.returnValue) ?? null;
}
async function playCandidates(candidates) {
    const choice = candidates.length === 1 ? candidates[0] : await choosePromotion(candidates);
    if (choice === undefined || choice === null)
        return;
    selectedSquare = null;
    await playMove(choice);
}
async function playMove(move) {
    const state = snapshot;
    if (state === null)
        return;
    commandPending = true;
    render(state);
    const next = await postCommand("/api/move", { uci: move.uci, revision: state.revision });
    commandPending = false;
    if (next === null) {
        render(state);
        animateSnapback(move.from, move.to, parseFen(state.fen).get(move.from));
    }
    else {
        render(next);
    }
}
function reportError(message) {
    errorMessage = message;
    boardMessage.textContent = message;
}
/**
 * Send a command and return the snapshot the server answered with.
 *
 * A rejection is reported as a sentence rather than as a protocol code, and returning null lets
 * the caller undo whatever it did optimistically — a snapback for a refused move, or repainting
 * the last known state for a refused control.
 */
async function postCommand(path, body) {
    reportError("");
    try {
        const response = await fetch(path, {
            method: "POST",
            headers: { "Content-Type": "application/json", "X-Seaborg-Token": token },
            body: JSON.stringify(body),
        });
        if (!response.ok) {
            const failure = (await response.json().catch(() => ({})));
            // A 5xx carries no useful code, so the status is what the message is built from.
            reportError(response.status >= 500
                ? `Seaborg hit an internal error (${response.status}). The position shown may be out of date.`
                : describeCommandError(failure.error ?? "request_failed"));
            return null;
        }
        return (await response.json());
    }
    catch {
        reportError(quitting
            ? "Seaborg has stopped. You can close this tab."
            : "Seaborg could not be reached. Check the terminal it was started from.");
        return null;
    }
}
async function sendControl(path, body) {
    if (commandPending)
        return;
    commandPending = true;
    if (snapshot !== null)
        render(snapshot);
    const next = await postCommand(path, body);
    commandPending = false;
    if (next !== null) {
        selectedSquare = null;
        focusedSquare = null;
        render(next);
    }
    else if (snapshot !== null) {
        render(snapshot);
    }
}
function animateSnapback(from, to, piece) {
    if (piece === undefined)
        return;
    const source = squareButton(from);
    const target = squareButton(to);
    if (source === null || target === null)
        return;
    const boardRect = boardElement.getBoundingClientRect();
    const sourceRect = source.getBoundingClientRect();
    const targetRect = target.getBoundingClientRect();
    const ghost = document.createElement("span");
    ghost.className = "snapback-piece";
    ghost.style.left = `${targetRect.left - boardRect.left}px`;
    ghost.style.top = `${targetRect.top - boardRect.top}px`;
    ghost.style.width = `${targetRect.width}px`;
    ghost.style.height = `${targetRect.height}px`;
    ghost.style.setProperty("--snap-x", `${sourceRect.left - targetRect.left}px`);
    ghost.style.setProperty("--snap-y", `${sourceRect.top - targetRect.top}px`);
    ghost.append(createPiece(piece));
    boardElement.append(ghost);
    ghost.addEventListener("animationend", () => ghost.remove(), { once: true });
}
function closestSquare(target) {
    return target instanceof Element ? target.closest(".square") : null;
}
boardElement.addEventListener("pointerdown", (event) => {
    const state = snapshot;
    const square = closestSquare(event.target);
    if (state === null || square === null || !canInteract(state))
        return;
    const from = square.dataset.square;
    if (from === undefined)
        return;
    const draggable = legalMovesFrom(state.legalMoves, from, parseFen(state.fen)).length > 0;
    gesture = {
        pointerId: event.pointerId,
        from,
        startX: event.clientX,
        startY: event.clientY,
        draggable,
        moved: false,
        ghost: null,
    };
    boardElement.setPointerCapture(event.pointerId);
    event.preventDefault();
});
boardElement.addEventListener("pointermove", (event) => {
    const active = gesture;
    const state = snapshot;
    if (active === null || state === null || active.pointerId !== event.pointerId || !active.draggable)
        return;
    const distance = Math.hypot(event.clientX - active.startX, event.clientY - active.startY);
    if (!active.moved && distance >= 6) {
        active.moved = true;
        selectedSquare = active.from;
        render(state);
        const piece = parseFen(state.fen).get(active.from);
        const square = squareButton(active.from);
        if (piece !== undefined && square !== null) {
            const rect = square.getBoundingClientRect();
            const ghost = document.createElement("span");
            ghost.className = "drag-piece";
            ghost.style.width = `${rect.width}px`;
            ghost.style.height = `${rect.height}px`;
            ghost.append(createPiece(piece));
            document.body.append(ghost);
            active.ghost = ghost;
            square.classList.add("drag-origin");
        }
    }
    if (active.ghost !== null) {
        active.ghost.style.left = `${event.clientX}px`;
        active.ghost.style.top = `${event.clientY}px`;
    }
    event.preventDefault();
});
function finishPointer(event, cancelled) {
    const active = gesture;
    if (active === null || active.pointerId !== event.pointerId)
        return;
    gesture = null;
    active.ghost?.remove();
    if (boardElement.hasPointerCapture(event.pointerId))
        boardElement.releasePointerCapture(event.pointerId);
    if (cancelled) {
        selectSquare(null);
        return;
    }
    const target = document.elementFromPoint(event.clientX, event.clientY);
    const destination = closestSquare(target)?.dataset.square ?? active.from;
    if (!active.moved) {
        activateSquare(destination);
        return;
    }
    const state = snapshot;
    if (state === null)
        return;
    const candidates = legalMovesTo(state.legalMoves, active.from, destination, parseFen(state.fen));
    if (candidates.length > 0) {
        void playCandidates(candidates);
    }
    else {
        const movingPiece = parseFen(state.fen).get(active.from);
        selectSquare(null);
        animateSnapback(active.from, destination, movingPiece);
    }
}
boardElement.addEventListener("pointerup", (event) => finishPointer(event, false));
boardElement.addEventListener("pointercancel", (event) => finishPointer(event, true));
boardElement.addEventListener("keydown", (event) => {
    const state = snapshot;
    const square = closestSquare(event.target);
    const current = square?.dataset.square;
    if (state === null || current === undefined)
        return;
    const delta = {
        ArrowLeft: [-1, 0],
        ArrowRight: [1, 0],
        ArrowUp: [0, -1],
        ArrowDown: [0, 1],
    };
    const movement = delta[event.key];
    if (movement !== undefined) {
        // Arrow keys follow what is on screen, so navigation stays correct after a flip.
        const orientation = orientationOf(state);
        const visual = visualCoordinates(current, orientation);
        const next = squareFromVisualCoordinates(visual.column + movement[0], visual.row + movement[1], orientation);
        if (next !== null) {
            focusedSquare = next;
            const nextButton = squareButton(next);
            for (const button of boardElement.querySelectorAll(".square"))
                button.tabIndex = -1;
            if (nextButton !== null) {
                nextButton.tabIndex = 0;
                nextButton.focus();
            }
        }
        event.preventDefault();
    }
    else if (event.key === "Enter" || event.key === " ") {
        activateSquare(current);
        event.preventDefault();
    }
    else if (event.key === "Escape") {
        selectSquare(null);
        squareButton(current)?.focus();
        event.preventDefault();
    }
});
undoButton.addEventListener("click", () => {
    if (snapshot !== null)
        void sendControl("/api/undo", { revision: snapshot.revision });
});
newWhiteButton.addEventListener("click", () => void startGame("white"));
newBlackButton.addEventListener("click", () => void startGame("black"));
// Restarting is a new game on the side already being played, so it needs no command of its own.
restartButton.addEventListener("click", () => {
    if (snapshot !== null)
        void startGame(snapshot.humanSide);
});
flipButton.addEventListener("click", () => {
    if (snapshot === null)
        return;
    flippedOrientation = orientationOf(snapshot) === "white" ? "black" : "white";
    render(snapshot);
});
limitSelect.addEventListener("change", () => {
    const parsed = parseEngineLimitValue(limitSelect.value);
    if (parsed === null)
        return;
    void sendControl("/api/engine-limit", parsed);
});
quitButton.addEventListener("click", () => void quit());
/**
 * Start a new game and return the board to that side's point of view.
 *
 * A flip is a way of looking at the current game, so it does not outlive it.
 */
async function startGame(humanSide) {
    flippedOrientation = null;
    await sendControl("/api/new-game", { humanSide });
}
/**
 * Stop the Seaborg process.
 *
 * `quitting` is set before the request so the reply — or the connection closing under it, which
 * is just as likely once the server begins shutting down — is reported as a successful stop
 * rather than as a lost connection. It also stops the event stream reconnecting to a server that
 * is deliberately gone.
 */
async function quit() {
    if (commandPending || quitting)
        return;
    quitting = true;
    commandPending = true;
    if (snapshot !== null)
        render(snapshot);
    await postCommand("/api/quit", {});
    commandPending = false;
    events.close();
    connectionElement.textContent = "Stopped";
    connectionElement.classList.remove("connected");
    reportError("Seaborg has stopped. You can close this tab.");
    if (snapshot !== null)
        render(snapshot);
}
async function loadInitialState() {
    try {
        const response = await fetch("/api/state");
        if (response.ok)
            render((await response.json()));
    }
    catch {
        connectionElement.textContent = "Waiting for Seaborg";
    }
}
void loadInitialState();
const events = new EventSource("/api/events");
// Whether the last thing the stream reported was a failure. The recovery message is only shown
// for a connection that was actually lost, so an ordinary first connect stays silent.
let connectionLost = false;
function markConnected() {
    connectionElement.textContent = "Connected";
    connectionElement.classList.add("connected");
    if (connectionLost) {
        connectionLost = false;
        // The state that arrives with the reconnection is authoritative, so the warning is stale.
        reportError("");
    }
}
events.addEventListener("open", markConnected);
events.addEventListener("message", (event) => {
    markConnected();
    render(JSON.parse(event.data));
});
events.addEventListener("error", () => {
    if (quitting) {
        // The stream closing is the expected result of the quit that is already in flight.
        events.close();
        return;
    }
    connectionLost = true;
    connectionElement.textContent = "Reconnecting…";
    connectionElement.classList.remove("connected");
    // The board still accepts moves — a command travels over its own request — but the position
    // may be behind, so say so rather than letting the page look merely idle.
    reportError("Lost the connection to Seaborg. Retrying…");
});
