import { describeSquare, legalMovesFrom, legalMovesTo, moveTransitions, orderedSquares, parseFen, pieceAssetId, squareFromVisualCoordinates, transitionOffset, visualCoordinates, } from "./board.js";
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
const undoButton = element("#undo");
const newWhiteButton = element("#new-white");
const newBlackButton = element("#new-black");
const promotionDialog = element("#promotion-dialog");
let snapshot = null;
let selectedSquare = null;
let focusedSquare = null;
let gesture = null;
let commandPending = false;
let lastAnimatedRevision = -1;
let errorMessage = "";
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
        state.gameStatus.kind === "ongoing" &&
        state.engineStatus.kind === "idle" &&
        state.sideToMove === state.humanSide);
}
function describeGame(state) {
    if (state.gameStatus.kind === "checkmate")
        return `Checkmate — ${state.gameStatus.winner} wins`;
    if (state.gameStatus.kind === "draw")
        return `Draw — ${state.gameStatus.reason.replaceAll("_", " ")}`;
    if (commandPending)
        return "Sending move…";
    if (state.engineStatus.kind === "thinking") {
        const progress = state.engineStatus.progress;
        return progress === null
            ? "Seaborg is thinking…"
            : `Seaborg is thinking — depth ${progress.depth}, ${progress.nodes.toLocaleString()} nodes`;
    }
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
    // POST responses and SSE updates travel independently. A fast engine can publish revision N+1
    // before the browser receives the command response for N, so never repaint an older snapshot.
    if (snapshot !== null && next.revision < snapshot.revision)
        return;
    const previous = snapshot;
    snapshot = next;
    if (!canInteract(next))
        selectedSquare = null;
    const shouldAnimate = previous !== null &&
        previous.revision !== next.revision &&
        next.revision !== lastAnimatedRevision &&
        next.lastMove !== null;
    const transitions = shouldAnimate
        ? moveTransitions(previous.fen, next.fen, next.lastMove?.uci ?? "")
        : [];
    if (shouldAnimate)
        lastAnimatedRevision = next.revision;
    renderBoard(next, previous, transitions);
    renderStatus(next);
}
function renderBoard(state, previous, transitions) {
    const board = parseFen(state.fen);
    const orientation = state.humanSide;
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
    turnElement.classList.toggle("thinking", state.engineStatus.kind === "thinking");
    boardMessage.textContent = errorMessage;
    undoButton.disabled = commandPending || state.moveHistory.length === 0;
    newWhiteButton.disabled = commandPending;
    newBlackButton.disabled = commandPending;
    historyElement.replaceChildren(...state.moveHistory.map((record) => {
        const item = document.createElement("li");
        item.textContent = record.san;
        item.title = record.uci;
        return item;
    }));
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
async function postCommand(path, body) {
    errorMessage = "";
    boardMessage.textContent = errorMessage;
    try {
        const response = await fetch(path, {
            method: "POST",
            headers: { "Content-Type": "application/json", "X-Seaborg-Token": token },
            body: JSON.stringify(body),
        });
        if (!response.ok) {
            const failure = (await response.json().catch(() => ({ error: "request_failed" })));
            errorMessage = capitalize((failure.error ?? "request_failed").replaceAll("_", " "));
            boardMessage.textContent = errorMessage;
            return null;
        }
        errorMessage = "";
        return (await response.json());
    }
    catch {
        errorMessage = "Seaborg could not be reached";
        boardMessage.textContent = errorMessage;
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
        const visual = visualCoordinates(current, state.humanSide);
        const next = squareFromVisualCoordinates(visual.column + movement[0], visual.row + movement[1], state.humanSide);
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
newWhiteButton.addEventListener("click", () => void sendControl("/api/new-game", { humanSide: "white" }));
newBlackButton.addEventListener("click", () => void sendControl("/api/new-game", { humanSide: "black" }));
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
events.addEventListener("open", () => {
    connectionElement.textContent = "Connected";
    connectionElement.classList.add("connected");
});
events.addEventListener("message", (event) => {
    connectionElement.textContent = "Connected";
    connectionElement.classList.add("connected");
    render(JSON.parse(event.data));
});
events.addEventListener("error", () => {
    connectionElement.textContent = "Reconnecting…";
    connectionElement.classList.remove("connected");
});
