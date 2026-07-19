//! The loopback HTTP server exposing the game controller to a local browser.
//!
//! The surface is deliberately fixed: six embedded assets, one state document, one event
//! stream, and five bounded commands. There is no file-path routing and no general engine
//! command endpoint, so nothing outside this list is reachable however the request is spelled.

use super::http::{
    self, read_request, write_error, write_event_stream_head, write_json, write_response, Request,
    Status,
};
use super::json::{self, Json};
use super::session::{self, Session};
use super::wire::{command_error_code, parse_engine_limit, parse_player};
use crate::search::SearchLimit;
use core::position::Player;
use std::fmt;
use std::io::{self, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const INDEX_HTML: &str = include_str!("assets/index.html");
const APP_JS: &str = include_str!("assets/app.js");
const BOARD_JS: &str = include_str!("assets/board.js");
const FORMAT_JS: &str = include_str!("assets/format.js");
const STYLE_CSS: &str = include_str!("assets/style.css");
const PIECES_SVG: &str = include_str!("assets/pieces.svg");

/// The marker in the embedded page replaced with this process's session token.
const TOKEN_PLACEHOLDER: &str = "__SEABORG_TOKEN__";

/// The header carrying the session token on mutating requests.
const TOKEN_HEADER: &str = "X-Seaborg-Token";

/// How long a blocked write is tolerated before a slow client's connection is dropped.
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the accept loop pauses after a failed `accept` before trying again.
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(50);

/// The most connections served at once, across requests and event streams.
///
/// One thread is dedicated to each connection, so without a cap an unbounded number of peers —
/// hostile, or merely a page reloaded in a loop — becomes an unbounded number of threads, and the
/// process dies when the thread limit is reached. A browser opens a handful of connections per
/// tab, so this is generous for the local single-user UI this server exists to host.
pub const MAX_CONNECTIONS: usize = 64;

/// How long writing a refusal to a connection that is never served may block.
///
/// Refusals are written on the accept-loop thread, so unlike [`WRITE_TIMEOUT`] this bounds how
/// long a peer being turned away can delay every other peer. A refusal is far smaller than a
/// socket buffer, so in practice this never elapses.
const REFUSE_WRITE_TIMEOUT: Duration = Duration::from_millis(250);

/// How long a refusal spends discarding the request the refused peer already sent.
const REFUSE_DRAIN_DEADLINE: Duration = Duration::from_millis(100);

/// How long any single refusal read waits before giving up on more arriving.
const REFUSE_DRAIN_POLL: Duration = Duration::from_millis(10);

/// The most of a refused request that is read and discarded.
const MAX_REFUSE_DRAIN: u64 = 64 * 1024;

/// The most of a rejected request that is read and discarded so the response can be delivered.
const MAX_DRAIN: u64 = 1024 * 1024;

/// How long draining a rejected request may take in total before the connection is dropped.
const DRAIN_DEADLINE: Duration = Duration::from_secs(2);

/// How long an idle event stream waits before sending a comment to prove the peer is alive.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);

/// How long the browser waits before reconnecting a dropped stream, in milliseconds.
const RECONNECT_DELAY_MS: u64 = 1_000;

/// Every asset and state response is uncached: the page embeds a per-process token, the state is
/// live, and the assets are compiled in, so there is never a reason to serve a stored copy.
const NO_STORE: &str = "no-store";

/// How the UI server should be started.
#[derive(Clone, Copy, Debug)]
pub struct UiConfig {
    /// A fixed port, or `None` to let the operating system choose an available one.
    pub port: Option<u16>,
    /// Whether to launch the default browser once the listener is ready.
    pub open_browser: bool,
    pub human_side: Player,
    pub search_limit: SearchLimit,
    pub hash_size_mb: usize,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            port: None,
            open_browser: true,
            human_side: Player::WHITE,
            search_limit: SearchLimit::Time(Duration::from_secs(1)),
            hash_size_mb: 16,
        }
    }
}

/// A failure that prevents the UI from starting or being presented.
#[derive(Debug)]
pub enum UiError {
    /// The loopback listener could not be created.
    Bind { port: u16, source: io::Error },
    /// The listener started but the default browser could not be launched.
    BrowserLaunch { url: String, source: io::Error },
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UiError::Bind { port, source } => {
                write!(
                    f,
                    "could not start the Seaborg UI on 127.0.0.1:{port}: {source}"
                )?;
                if source.kind() == io::ErrorKind::AddrInUse {
                    write!(
                        f,
                        "\nAnother process is already using port {port}. \
                         Choose a different port with --ui-port, or omit it to pick one automatically."
                    )?;
                }
                Ok(())
            }
            UiError::BrowserLaunch { url, source } => write!(
                f,
                "could not open the default browser: {source}\nThe Seaborg UI is running at {url}"
            ),
        }
    }
}

impl std::error::Error for UiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            UiError::Bind { source, .. } | UiError::BrowserLaunch { source, .. } => Some(source),
        }
    }
}

/// State shared by every connection thread.
struct ServerState {
    session: Arc<Session>,
    /// The page with the session token already substituted.
    index_html: String,
    port: u16,
    /// Everything needed to stop the server from a connection thread, for `/api/quit`.
    shutdown: ShutdownSignal,
}

/// The three things that together stop a running server.
///
/// `UiHandle` and the `/api/quit` route must do exactly the same thing, so they share this rather
/// than each open-coding the sequence — a quit that forgot to wake the accept loop would leave the
/// process alive with the browser already told it had stopped.
#[derive(Clone)]
struct ShutdownSignal {
    session: Arc<Session>,
    local_addr: SocketAddr,
    accepting: Arc<AtomicBool>,
}

impl ShutdownSignal {
    /// Stop the server and release every open stream.
    ///
    /// The accept loop blocks in `accept`, so it is woken by dialing the listener once; the loop
    /// then observes the cleared flag and returns.
    fn trigger(&self) {
        self.accepting.store(false, Ordering::SeqCst);
        self.session.shutdown();
        let _ = TcpStream::connect_timeout(&self.local_addr, Duration::from_secs(1));
    }
}

/// A bound, not-yet-serving UI server.
pub struct UiServer {
    listener: TcpListener,
    state: Arc<ServerState>,
    local_addr: SocketAddr,
    accepting: Arc<AtomicBool>,
    connections: Arc<AtomicUsize>,
}

/// The right to occupy one of the [`MAX_CONNECTIONS`] serving slots.
///
/// The slot is returned when this is dropped, so it is released however the connection thread
/// ends — a completed response, a dropped peer, an I/O failure, or a panic while unwinding.
struct ConnectionPermit(Arc<AtomicUsize>);

impl ConnectionPermit {
    /// Claim a slot, or return `None` when the server is already at capacity.
    fn acquire(connections: &Arc<AtomicUsize>) -> Option<Self> {
        let mut count = connections.load(Ordering::SeqCst);
        loop {
            if count >= MAX_CONNECTIONS {
                return None;
            }
            // Compare-and-swap rather than `fetch_add`, so the cap is never briefly exceeded by a
            // claim that has to be given back.
            match connections.compare_exchange_weak(
                count,
                count + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return Some(Self(Arc::clone(connections))),
                Err(actual) => count = actual,
            }
        }
    }
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Answer a connection that will not be served, without blocking the accept loop.
fn refuse(stream: &mut TcpStream, code: &str) {
    let _ = stream.set_write_timeout(Some(REFUSE_WRITE_TIMEOUT));
    let _ = write_error(stream, Status::ServiceUnavailable, code);

    // For the reason given on `drain_rejected_request`: closing while the peer's request is still
    // queued makes the kernel send RST, and the refusal the peer needs to see is lost with it.
    //
    // This runs on the accept-loop thread, so it is bounded far more tightly than that path, and
    // each read gives up after `REFUSE_DRAIN_POLL` rather than waiting out the whole deadline. A
    // peer that already sent a request has it buffered and is drained immediately; a peer that
    // sent nothing has nothing queued to cause an RST, so there is no reason to wait for it — and
    // that is the case a flood consists of, which is exactly when the accept loop must stay free.
    let deadline = Instant::now() + REFUSE_DRAIN_DEADLINE;
    let mut buffer = [0_u8; 4 * 1024];
    let mut remaining = MAX_REFUSE_DRAIN;

    while remaining > 0 {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero()
            || stream
                .set_read_timeout(Some(left.min(REFUSE_DRAIN_POLL)))
                .is_err()
        {
            return;
        }
        let wanted = buffer.len().min(remaining as usize);
        match stream.read(&mut buffer[..wanted]) {
            Ok(0) | Err(_) => return,
            Ok(read) => remaining -= read as u64,
        }
    }
}

/// A cloneable handle for stopping a running server.
#[derive(Clone)]
pub struct UiHandle {
    shutdown: ShutdownSignal,
}

impl UiHandle {
    /// Stop the server and release every open stream.
    pub fn shutdown(&self) {
        self.shutdown.trigger();
    }
}

/// Bind the loopback listener, failing before anything is announced to the user.
pub fn bind(config: &UiConfig) -> Result<UiServer, UiError> {
    let port = config.port.unwrap_or(0);
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .map_err(|source| UiError::Bind { port, source })?;
    let local_addr = listener
        .local_addr()
        .map_err(|source| UiError::Bind { port, source })?;

    let session = Session::new(config.human_side, config.search_limit, config.hash_size_mb);
    let index_html = INDEX_HTML.replace(TOKEN_PLACEHOLDER, session.token());
    let accepting = Arc::new(AtomicBool::new(true));
    let shutdown = ShutdownSignal {
        session: Arc::clone(&session),
        local_addr,
        accepting: Arc::clone(&accepting),
    };

    Ok(UiServer {
        listener,
        state: Arc::new(ServerState {
            session,
            index_html,
            port: local_addr.port(),
            shutdown,
        }),
        local_addr,
        accepting,
        connections: Arc::new(AtomicUsize::new(0)),
    })
}

impl UiServer {
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// The URL a browser should open.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.local_addr.port())
    }

    pub fn token(&self) -> &str {
        self.state.session.token()
    }

    pub fn handle(&self) -> UiHandle {
        UiHandle {
            shutdown: self.state.shutdown.clone(),
        }
    }

    /// Serve until the server is shut down, driving the controller on a background thread.
    pub fn run(self) {
        let driver = {
            let session = Arc::clone(&self.state.session);
            thread::spawn(move || session::drive(session))
        };

        for stream in self.listener.incoming() {
            if !self.accepting.load(Ordering::SeqCst) {
                break;
            }
            let mut stream = match stream {
                Ok(stream) => stream,
                // Retrying immediately would spin this loop at full speed for as long as the
                // condition lasts, which for descriptor exhaustion is until a connection closes.
                Err(_) => {
                    thread::sleep(ACCEPT_ERROR_BACKOFF);
                    continue;
                }
            };

            // Serving is capped rather than unbounded, so a flood of connections is turned away
            // one at a time instead of accumulating threads until the process cannot make one.
            let Some(permit) = ConnectionPermit::acquire(&self.connections) else {
                refuse(&mut stream, "too_many_connections");
                continue;
            };

            let state = Arc::clone(&self.state);
            // A failed spawn is a transient resource failure exactly like a failed accept, so it
            // is stepped over. `thread::spawn` would panic instead, and because this runs on the
            // accept-loop thread that panic would unwind `run` and take the engine process down.
            // The connection and its permit are owned by the closure, so a rejected spawn drops
            // both: the peer sees a close and the slot is returned.
            let spawned = thread::Builder::new().spawn(move || {
                let _permit = permit;
                handle_connection(stream, &state);
            });
            if spawned.is_err() {
                thread::sleep(ACCEPT_ERROR_BACKOFF);
            }
        }

        self.state.session.shutdown();
        let _ = driver.join();
    }
}

/// Open `url` in the default browser.
pub fn open_browser(url: &str) -> Result<(), UiError> {
    open::that(url).map_err(|source| UiError::BrowserLaunch {
        url: url.to_owned(),
        source,
    })
}

fn handle_connection(mut stream: TcpStream, state: &ServerState) {
    // `read_request` sets the read timeout itself, from the time left before the request deadline.
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let _ = stream.set_nodelay(true);

    let Ok(peer) = stream.peer_addr() else { return };
    // The listener is loopback-bound, so this is belt and braces against a routing surprise.
    if !peer.ip().is_loopback() {
        let _ = write_error(&mut stream, Status::Forbidden, "forbidden_peer");
        return;
    }

    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    });

    let request = match read_request(&mut reader, Instant::now() + http::REQUEST_DEADLINE) {
        Ok(request) => request,
        Err(error) => {
            if let Some(status) = error.status() {
                let _ = write_error(&mut stream, status, "bad_request");
                drain_rejected_request(&mut reader);
            }
            return;
        }
    };

    if let Err(code) = check_origin_headers(&request, state.port) {
        let _ = write_error(&mut stream, Status::Forbidden, code);
        return;
    }

    let _ = route(&mut stream, &request, state);
}

/// Consume a bounded amount of a rejected request so the client can read the response.
///
/// Closing a socket that still has unread data queued makes the kernel send RST, which discards
/// whatever the client had not yet read — including the status explaining the rejection. Reading
/// the remainder first allows an ordinary close.
///
/// Two bounds keep a client that keeps sending from occupying this thread: [`MAX_DRAIN`] caps the
/// bytes and [`DRAIN_DEADLINE`] caps the elapsed time. The deadline is recomputed before every
/// read rather than installed once, for the reason [`http::apply_deadline`] does the same on the
/// request path: a socket timeout restarts on each read, so a client dribbling one byte just
/// inside it would otherwise hold this thread for as long as it took to reach the byte cap.
fn drain_rejected_request(reader: &mut BufReader<TcpStream>) {
    let deadline = Instant::now() + DRAIN_DEADLINE;
    let mut buffer = [0_u8; 8 * 1024];
    let mut remaining = MAX_DRAIN;

    while remaining > 0 {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || reader.get_ref().set_read_timeout(Some(left)).is_err() {
            return;
        }
        let wanted = buffer.len().min(remaining as usize);
        match reader.read(&mut buffer[..wanted]) {
            // Nothing more is coming, or the read timed out, or the peer failed. Either way the
            // response has been written and there is nothing left worth waiting for.
            Ok(0) | Err(_) => return,
            Ok(read) => remaining -= read as u64,
        }
    }
}

/// Reject requests that did not originate from this server's own loopback page.
///
/// A `Host` allowlist defeats DNS rebinding, where a hostile site resolves its own name to
/// 127.0.0.1 so the browser treats this server as that site's origin.
///
/// `Origin` is validated whenever present, but its absence is accepted rather than rejected,
/// because a browser legitimately omits it on same-origin navigations and subresource loads — the
/// very requests that fetch this UI. That leniency is safe on the two axes that matter separately
/// from each other. Mutation is unreachable: browsers do send `Origin` on cross-origin requests,
/// so a hostile page reaching a POST is rejected here, and it could not supply the token anyway.
/// Disclosure is unreachable: no `Access-Control-Allow-Origin` is emitted, so a cross-origin read
/// cannot see a response body, and `nosniff` with exact content types plus `frame-ancestors
/// 'none'` blocks the tag-based smuggling that would otherwise sidestep that.
///
/// What the absence of `Origin` does leave open is a cross-origin GET being *made* — an `<img>`
/// or `<script>` pointed at `/api/events` pins a connection even though it can read nothing. That
/// is a resource question rather than an access-control one, and it is bounded by
/// [`MAX_CONNECTIONS`] rather than by this check.
fn check_origin_headers(request: &Request, port: u16) -> Result<(), &'static str> {
    let host = request.header("host").ok_or("missing_host")?;
    if !is_own_authority(host, port) {
        return Err("forbidden_host");
    }
    match request.header("origin") {
        None => Ok(()),
        // An opaque origin (`null`) and any non-`http` scheme fail the prefix check, and an empty
        // authority has no port so `is_own_authority` rejects it.
        Some(origin) => match origin.strip_prefix("http://") {
            Some(authority) if is_own_authority(authority, port) => Ok(()),
            _ => Err("forbidden_origin"),
        },
    }
}

/// True when `authority` names this server's own loopback address and port.
fn is_own_authority(authority: &str, port: u16) -> bool {
    // Split the port off from the right so an IPv6 literal's own colons stay with the host.
    // An authority with no port implies port 80, which this loopback server never binds.
    let Some((host, port_text)) = authority.rsplit_once(':') else {
        return false;
    };
    if port_text.parse::<u16>() != Ok(port) {
        return false;
    }
    // The host component of an authority is case-insensitive.
    ["127.0.0.1", "localhost", "[::1]"]
        .iter()
        .any(|allowed| host.eq_ignore_ascii_case(allowed))
}

fn route(stream: &mut TcpStream, request: &Request, state: &ServerState) -> io::Result<()> {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => write_response(
            stream,
            Status::Ok,
            "text/html; charset=utf-8",
            NO_STORE,
            state.index_html.as_bytes(),
        ),
        ("GET", "/app.js") => write_response(
            stream,
            Status::Ok,
            "text/javascript; charset=utf-8",
            NO_STORE,
            APP_JS.as_bytes(),
        ),
        ("GET", "/board.js") => write_response(
            stream,
            Status::Ok,
            "text/javascript; charset=utf-8",
            NO_STORE,
            BOARD_JS.as_bytes(),
        ),
        ("GET", "/format.js") => write_response(
            stream,
            Status::Ok,
            "text/javascript; charset=utf-8",
            NO_STORE,
            FORMAT_JS.as_bytes(),
        ),
        ("GET", "/style.css") => write_response(
            stream,
            Status::Ok,
            "text/css; charset=utf-8",
            NO_STORE,
            STYLE_CSS.as_bytes(),
        ),
        ("GET", "/pieces.svg") => write_response(
            stream,
            Status::Ok,
            "image/svg+xml",
            NO_STORE,
            PIECES_SVG.as_bytes(),
        ),
        ("GET", "/api/state") => {
            let (_, json) = state.session.current();
            write_json(stream, Status::Ok, &json)
        }
        ("GET", "/api/events") => stream_events(stream, request, state),
        ("POST", "/api/move")
        | ("POST", "/api/undo")
        | ("POST", "/api/new-game")
        | ("POST", "/api/engine-limit")
        | ("POST", "/api/quit") => handle_command(stream, request, state),
        (_, "/")
        | (_, "/app.js")
        | (_, "/board.js")
        | (_, "/format.js")
        | (_, "/style.css")
        | (_, "/pieces.svg")
        | (_, "/api/state")
        | (_, "/api/events") => write_error(stream, Status::MethodNotAllowed, "method_not_allowed"),
        (_, "/api/move")
        | (_, "/api/undo")
        | (_, "/api/new-game")
        | (_, "/api/engine-limit")
        | (_, "/api/quit") => write_error(stream, Status::MethodNotAllowed, "method_not_allowed"),
        _ => write_error(stream, Status::NotFound, "not_found"),
    }
}

/// Apply a mutating command after authenticating and validating it.
fn handle_command(
    stream: &mut TcpStream,
    request: &Request,
    state: &ServerState,
) -> io::Result<()> {
    if !authorized(request, state.session.token()) {
        return write_error(stream, Status::Forbidden, "invalid_token");
    }

    // A JSON content type keeps a simple HTML form, which cannot set the token header, from
    // reaching this endpoint at all.
    let content_type = request.header("content-type").unwrap_or_default();
    if !content_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    {
        return write_error(stream, Status::UnsupportedMediaType, "expected_json");
    }

    let Ok(body) = std::str::from_utf8(&request.body) else {
        return write_error(stream, Status::BadRequest, "malformed_json");
    };
    let Ok(document) = json::parse(body) else {
        return write_error(stream, Status::BadRequest, "malformed_json");
    };
    if !matches!(document, Json::Object(_)) {
        return write_error(stream, Status::BadRequest, "malformed_json");
    }

    let outcome = match request.path.as_str() {
        "/api/move" => {
            let Some(uci) = document.get("uci").and_then(Json::as_str) else {
                return write_error(stream, Status::UnprocessableContent, "missing_uci");
            };
            let Some(revision) = document.get("revision").and_then(Json::as_u64) else {
                return write_error(stream, Status::UnprocessableContent, "missing_revision");
            };
            state.session.play_move(uci, revision)
        }
        "/api/undo" => {
            let Some(revision) = document.get("revision").and_then(Json::as_u64) else {
                return write_error(stream, Status::UnprocessableContent, "missing_revision");
            };
            state.session.undo(revision)
        }
        "/api/new-game" => {
            let Some(side) = document.get("humanSide").and_then(Json::as_str) else {
                return write_error(stream, Status::UnprocessableContent, "missing_human_side");
            };
            let Some(side) = parse_player(side) else {
                return write_error(stream, Status::UnprocessableContent, "invalid_human_side");
            };
            state.session.new_game(side);
            Ok(())
        }
        "/api/engine-limit" => {
            let Some(kind) = document.get("kind").and_then(Json::as_str) else {
                return write_error(stream, Status::UnprocessableContent, "missing_engine_limit");
            };
            let Some(value) = document.get("value").and_then(Json::as_u64) else {
                return write_error(stream, Status::UnprocessableContent, "missing_engine_limit");
            };
            match parse_engine_limit(kind, value) {
                Ok(limit) => {
                    state.session.set_engine_limit(limit);
                    Ok(())
                }
                Err(code) => return write_error(stream, Status::UnprocessableContent, code),
            }
        }
        "/api/quit" => {
            // The response is written before the server is stopped, so the browser learns the
            // request was accepted rather than seeing the connection drop under it. Shutting down
            // first would close this socket while the reply was still queued.
            write_json(stream, Status::Ok, "{\"quitting\":true}")?;
            state.shutdown.trigger();
            return Ok(());
        }
        // `route` dispatches only the paths above. Answering anything else explicitly keeps a
        // command route added there later from silently falling into one of these branches.
        _ => return write_error(stream, Status::NotFound, "not_found"),
    };

    match outcome {
        // Returning the resulting snapshot lets the client act on the new revision without
        // waiting for the stream to deliver it.
        Ok(()) => {
            let (_, json) = state.session.current();
            write_json(stream, Status::Ok, &json)
        }
        Err(error) => write_error(stream, Status::Conflict, command_error_code(&error)),
    }
}

fn authorized(request: &Request, token: &str) -> bool {
    request
        .header(TOKEN_HEADER)
        .is_some_and(|presented| constant_time_eq(presented.as_bytes(), token.as_bytes()))
}

/// Compare without an early exit, so a wrong token leaks no information about how much matched.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (a, b) in left.iter().zip(right) {
        difference |= a ^ b;
    }
    difference == 0
}

/// Stream authoritative snapshots until the client disconnects or the server stops.
fn stream_events(stream: &mut TcpStream, request: &Request, state: &ServerState) -> io::Result<()> {
    write_event_stream_head(stream)?;
    write!(stream, "retry: {RECONNECT_DELAY_MS}\n\n")?;
    stream.flush()?;

    // On reconnect the browser replays the last event it processed. State is a complete snapshot
    // rather than a delta, so a resuming client is simply brought up to date, and is sent nothing
    // at all if it already holds the newest event.
    //
    // An ID above this session's counter is not a resume point: event IDs restart at zero each
    // process, so a tab left open across a restart reconnects quoting the previous process's IDs.
    // Trusting it would leave that tab waiting for events this session reaches only much later,
    // rendering stale state indefinitely. Such an ID is treated as a fresh connection instead.
    let (current_id, current_json) = state.session.current();
    let resume_from = request
        .header("last-event-id")
        .and_then(|value| value.trim().parse::<u64>().ok());

    let mut last_sent = match resume_from {
        Some(id) if id <= current_id => id,
        _ => {
            write_event(stream, current_id, &current_json)?;
            current_id
        }
    };

    while state.session.is_running() {
        match state.session.wait_for_update(last_sent, KEEPALIVE_INTERVAL) {
            Some((id, json)) => {
                write_event(stream, id, &json)?;
                last_sent = id;
            }
            // A comment is a no-op for the client but still a write, so a vanished peer surfaces
            // here as an error rather than leaking this thread for the life of the process.
            None => {
                stream.write_all(b": keepalive\n\n")?;
                stream.flush()?;
            }
        }
    }
    Ok(())
}

/// Write one Server-Sent Event.
///
/// The JSON writer escapes newlines, so a snapshot is always a single `data:` line.
fn write_event(stream: &mut TcpStream, id: u64, json: &str) -> io::Result<()> {
    debug_assert!(!json.contains('\n'), "a snapshot must be one data line");
    write!(stream, "id: {id}\ndata: {json}\n\n")?;
    stream.flush()
}

/// Expose the request limits so the CLI and tests describe the same contract.
pub const MAX_REQUEST_BODY: usize = http::MAX_BODY;
