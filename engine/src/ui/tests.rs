//! End-to-end protocol tests driving a real loopback server over real sockets.
//!
//! These exercise the wire behaviour a browser depends on — headers, status codes, framing, and
//! the event stream — rather than the routing functions in isolation, so a regression in the
//! hand-rolled HTTP layer cannot pass by satisfying an internal signature.

use super::server::{bind, UiConfig, UiError, UiHandle, MAX_CONNECTIONS, MAX_REQUEST_BODY};
use crate::search::SearchLimit;
use core::init::init_globals;
use core::position::Player;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// A running server that is always shut down when the test ends.
struct TestServer {
    addr: SocketAddr,
    token: String,
    handle: UiHandle,
    serving: Option<JoinHandle<()>>,
}

fn test_config() -> UiConfig {
    init_globals();
    UiConfig {
        port: None,
        // Nothing in these tests may launch a real browser.
        open_browser: false,
        human_side: Player::WHITE,
        // Keep engine replies fast enough that tests never wait on a real search.
        search_limit: SearchLimit::Depth(1),
        hash_size_mb: 1,
    }
}

impl TestServer {
    fn start() -> Self {
        Self::start_with(&test_config()).expect("the loopback listener should bind")
    }

    fn start_with(config: &UiConfig) -> Result<Self, UiError> {
        let server = bind(config)?;
        let addr = server.local_addr();
        let token = server.token().to_owned();
        let handle = server.handle();
        let serving = std::thread::spawn(move || server.run());
        Ok(Self {
            addr,
            token,
            handle,
            serving: Some(serving),
        })
    }

    fn stop(&mut self) {
        self.handle.shutdown();
        if let Some(serving) = self.serving.take() {
            serving.join().expect("the server thread should stop");
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A parsed HTTP response.
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|_| panic!("expected JSON, got {:?}", self.body))
    }

    /// The `error` code from a rejection body.
    fn error_code(&self) -> String {
        self.json()
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }
}

/// Send a raw request and read the whole response, which `Connection: close` terminates.
fn send_raw(addr: SocketAddr, raw: &[u8]) -> Response {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(5)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    stream.write_all(raw).unwrap();
    stream.flush().unwrap();

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text
        .split_once("\r\n\r\n")
        .expect("a complete response head");

    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap();
    let status = status_line
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("unparseable status line {status_line:?}"));
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_owned(), value.trim().to_owned()))
        .collect();

    Response {
        status,
        headers,
        body: body.to_owned(),
    }
}

/// Build a request, defaulting `Host` to the server's own authority.
fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
    body: Option<&str>,
) -> Response {
    let mut raw = format!("{method} {path} HTTP/1.1\r\n");
    if !extra_headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("host"))
    {
        raw.push_str(&format!("Host: 127.0.0.1:{}\r\n", addr.port()));
    }
    for (name, value) in extra_headers {
        raw.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(body) = body {
        raw.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    raw.push_str("\r\n");
    if let Some(body) = body {
        raw.push_str(body);
    }
    send_raw(addr, raw.as_bytes())
}

fn get(server: &TestServer, path: &str) -> Response {
    request(server.addr, "GET", path, &[], None)
}

/// Post a JSON command carrying the session token.
fn post(server: &TestServer, path: &str, body: &str) -> Response {
    request(
        server.addr,
        "POST",
        path,
        &[
            ("Content-Type", "application/json"),
            ("X-Seaborg-Token", &server.token),
        ],
        Some(body),
    )
}

fn revision(server: &TestServer) -> u64 {
    get(server, "/api/state")
        .json()
        .get("revision")
        .and_then(Value::as_u64)
        .expect("a revision")
}

/// Poll the state endpoint until `predicate` holds, so tests never race the engine.
fn wait_for_state(server: &TestServer, predicate: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let state = get(server, "/api/state").json();
        if predicate(&state) {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "timed out; last state: {state:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// An open event stream, read incrementally.
struct EventStream {
    reader: BufReader<TcpStream>,
}

impl EventStream {
    fn open(server: &TestServer, extra_headers: &[(&str, &str)]) -> Self {
        let mut stream = TcpStream::connect_timeout(&server.addr, Duration::from_secs(5)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        let mut raw = format!(
            "GET /api/events HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n",
            server.addr.port()
        );
        for (name, value) in extra_headers {
            raw.push_str(&format!("{name}: {value}\r\n"));
        }
        raw.push_str("\r\n");
        stream.write_all(raw.as_bytes()).unwrap();
        stream.flush().unwrap();

        let mut reader = BufReader::new(stream);
        // Consume the response head so the caller sees only the event body.
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
        }
        Self { reader }
    }

    /// Read the next `id`/`data` event, skipping keepalive comments and `retry` lines.
    fn next_event(&mut self) -> (u64, Value) {
        let mut id = None;
        loop {
            let mut line = String::new();
            let read = self.reader.read_line(&mut line).unwrap();
            assert_ne!(read, 0, "the stream closed before an event arrived");
            let line = line.trim_end();
            if let Some(value) = line.strip_prefix("id: ") {
                id = Some(value.parse().unwrap());
            } else if let Some(value) = line.strip_prefix("data: ") {
                return (
                    id.expect("an event id precedes its data"),
                    serde_json::from_str(value).unwrap(),
                );
            }
        }
    }

    /// Read the next line of any kind, so keepalives and their absence are observable.
    fn next_line(&mut self) -> String {
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        line
    }
}

// -- startup and shutdown ---------------------------------------------------------------------

#[test]
fn binds_to_loopback_on_an_available_port_and_reports_its_url() {
    let config = test_config();
    let server = bind(&config).unwrap();
    let addr = server.local_addr();

    assert!(addr.ip().is_loopback(), "expected loopback, got {addr}");
    assert_eq!(addr.ip(), Ipv4Addr::LOCALHOST);
    assert_ne!(addr.port(), 0, "an available port should be resolved");
    assert_eq!(server.url(), format!("http://127.0.0.1:{}/", addr.port()));
    assert_eq!(server.token().len(), 32);
}

#[test]
fn the_listener_accepts_connections_before_the_url_is_announced() {
    // `bind` returns a listening socket, so the URL it reports is already connectable. This is
    // what lets the CLI print and open the URL without racing the accept loop.
    let server = bind(&test_config()).unwrap();
    let addr = server.local_addr();
    TcpStream::connect_timeout(&addr, Duration::from_secs(5))
        .expect("the announced address should already accept connections");
}

#[test]
fn serves_on_a_requested_fixed_port() {
    // Probe for a port the operating system considers free, then re-request it explicitly.
    let probe = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let config = UiConfig {
        port: Some(port),
        ..test_config()
    };
    let Ok(server) = TestServer::start_with(&config) else {
        // Another process may claim the port in the gap above; that is not a failure of --ui-port.
        return;
    };
    assert_eq!(server.addr.port(), port);
    assert_eq!(get(&server, "/api/state").status, 200);
}

#[test]
fn reports_a_clear_error_when_the_requested_port_is_unavailable() {
    let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = occupied.local_addr().unwrap().port();

    let config = UiConfig {
        port: Some(port),
        ..test_config()
    };
    let Err(error) = bind(&config) else {
        panic!("binding an occupied port should fail");
    };
    let UiError::Bind {
        port: reported,
        source,
    } = &error
    else {
        panic!("expected a bind error, got {error:?}");
    };
    assert_eq!(*reported, port);
    assert_eq!(source.kind(), std::io::ErrorKind::AddrInUse);

    let message = error.to_string();
    assert!(message.contains(&port.to_string()), "message: {message}");
    assert!(message.contains("--ui-port"), "message: {message}");
}

#[test]
fn shutdown_stops_the_server_and_releases_open_streams() {
    let mut server = TestServer::start();
    let mut stream = EventStream::open(&server, &[]);
    let (_, first) = stream.next_event();
    assert!(first.get("fen").is_some());

    server.stop();

    // The stream ends rather than hanging: an empty read is end-of-file.
    let deadline = Instant::now() + Duration::from_secs(20);
    while !stream.next_line().is_empty() {
        assert!(Instant::now() < deadline, "the stream did not close");
    }
}

// -- embedded assets --------------------------------------------------------------------------

#[test]
fn serves_the_embedded_assets_with_their_content_types() {
    let server = TestServer::start();
    for (path, content_type, marker) in [
        ("/", "text/html; charset=utf-8", "<title>Seaborg</title>"),
        ("/app.js", "text/javascript; charset=utf-8", "EventSource"),
        ("/board.js", "text/javascript; charset=utf-8", "parseFen"),
        (
            "/format.js",
            "text/javascript; charset=utf-8",
            "formatScore",
        ),
        ("/style.css", "text/css; charset=utf-8", "body"),
        ("/pieces.svg", "image/svg+xml", "white-king"),
        (
            "/licenses",
            "text/plain; charset=utf-8",
            "Colin M.L. Burnett",
        ),
    ] {
        let response = get(&server, path);
        assert_eq!(response.status, 200, "{path}");
        assert_eq!(
            response.header("content-type"),
            Some(content_type),
            "{path}"
        );
        assert!(
            response.body.contains(marker),
            "{path} body: {}",
            response.body
        );
    }
}

/// The frontend addresses piece artwork as `/pieces.svg#<colour>-<kind>` and gets no error when
/// the fragment is missing — the square simply renders empty. A sprite whose symbol ids drifted
/// from that pattern would therefore fail silently and only in a browser.
#[test]
fn the_sprite_defines_a_symbol_for_every_piece_the_frontend_can_ask_for() {
    let server = TestServer::start();
    let sprite = get(&server, "/pieces.svg").body;
    for colour in ["white", "black"] {
        for kind in ["pawn", "knight", "bishop", "rook", "queen", "king"] {
            let symbol = format!("<symbol id=\"{colour}-{kind}\"");
            assert!(
                sprite.contains(&symbol),
                "sprite is missing {colour}-{kind}"
            );
        }
    }
}

/// The piece artwork is third-party work taken under a license whose sole condition is that its
/// notice reach whoever receives the binary. Serving the notice from the running executable is how
/// that condition is met for someone who never reads the source tree, so an unreachable or
/// incomplete notice is a licensing defect, not a cosmetic one.
#[test]
fn the_served_artwork_notice_carries_the_terms_it_has_to_convey() {
    let server = TestServer::start();
    let notice = get(&server, "/licenses").body;
    for required in [
        "Colin M.L. Burnett",
        "BSD 3-clause",
        "Redistributions in binary form must reproduce the above copyright notice",
        "THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS \"AS IS\"",
    ] {
        assert!(notice.contains(required), "the notice omits {required:?}");
    }
    assert!(
        get(&server, "/").body.contains("href=\"/licenses\""),
        "the page should link the notice, since the artwork it covers is what the page renders"
    );
}

#[test]
fn board_assets_define_rigid_eight_by_eight_tracks() {
    let server = TestServer::start();
    let style = get(&server, "/style.css").body;
    assert!(style.contains("grid-template-columns: repeat(8, minmax(0, 1fr))"));
    assert!(style.contains("grid-template-rows: repeat(8, minmax(0, 1fr))"));
    assert!(style.contains("min-height: 0"));
}

#[test]
fn the_page_carries_the_session_token_and_no_placeholder() {
    let server = TestServer::start();
    let body = get(&server, "/").body;
    assert!(
        body.contains(&server.token),
        "the page should embed the token"
    );
    assert!(
        !body.contains("__SEABORG_TOKEN__"),
        "the placeholder should be substituted"
    );
}

#[test]
fn every_response_sets_the_security_and_caching_headers() {
    let server = TestServer::start();
    for path in [
        "/",
        "/app.js",
        "/board.js",
        "/style.css",
        "/pieces.svg",
        "/licenses",
        "/api/state",
    ] {
        let response = get(&server, path);
        assert_eq!(response.header("cache-control"), Some("no-store"), "{path}");
        assert_eq!(
            response.header("x-content-type-options"),
            Some("nosniff"),
            "{path}"
        );
        let policy = response
            .header("content-security-policy")
            .unwrap_or_else(|| panic!("{path} has no policy"));
        assert!(policy.contains("default-src 'none'"), "{path}: {policy}");
        assert!(
            policy.contains("frame-ancestors 'none'"),
            "{path}: {policy}"
        );
        assert!(!policy.contains("unsafe-inline"), "{path}: {policy}");
    }
}

// -- state retrieval --------------------------------------------------------------------------

#[test]
fn serves_the_current_state_as_json() {
    let server = TestServer::start();
    let response = get(&server, "/api/state");
    assert_eq!(response.status, 200);
    assert_eq!(
        response.header("content-type"),
        Some("application/json; charset=utf-8")
    );

    let state = response.json();
    assert_eq!(state.get("revision").and_then(Value::as_u64), Some(0));
    assert_eq!(
        state.get("humanSide").and_then(Value::as_str),
        Some("white")
    );
    assert_eq!(
        state.get("sideToMove").and_then(Value::as_str),
        Some("white")
    );
    assert_eq!(state.get("inCheck"), Some(&Value::Bool(false)));
    let Some(Value::Array(moves)) = state.get("legalMoves") else {
        panic!("expected legal moves");
    };
    assert_eq!(moves.len(), 20, "twenty legal opening moves");
}

// -- commands ---------------------------------------------------------------------------------

#[test]
fn applies_a_legal_move_and_returns_the_resulting_snapshot() {
    let server = TestServer::start();
    let response = post(&server, "/api/move", r#"{"uci":"e2e4","revision":0}"#);
    assert_eq!(response.status, 200);

    let state = response.json();
    assert_eq!(state.get("revision").and_then(Value::as_u64), Some(1));
    assert_eq!(
        state
            .get("lastMove")
            .and_then(|last| last.get("san"))
            .and_then(Value::as_str),
        Some("e4")
    );

    // The driver thread polls the controller, so the engine's reply lands without further input.
    let state = wait_for_state(&server, |state| {
        state
            .get("revision")
            .and_then(Value::as_u64)
            .is_some_and(|revision| revision >= 2)
    });
    assert_eq!(
        state.get("sideToMove").and_then(Value::as_str),
        Some("white")
    );
    let Some(Value::Array(history)) = state.get("moveHistory") else {
        panic!("expected a move history");
    };
    assert_eq!(history.len(), 2, "the engine should have replied");
}

#[test]
fn undo_rewinds_to_the_humans_turn() {
    let server = TestServer::start();
    post(&server, "/api/move", r#"{"uci":"e2e4","revision":0}"#);
    wait_for_state(&server, |state| {
        state
            .get("revision")
            .and_then(Value::as_u64)
            .is_some_and(|revision| revision >= 2)
    });

    let response = post(
        &server,
        "/api/undo",
        &format!(r#"{{"revision":{}}}"#, revision(&server)),
    );
    assert_eq!(response.status, 200);
    let state = response.json();
    let Some(Value::Array(history)) = state.get("moveHistory") else {
        panic!("expected a move history");
    };
    assert!(history.is_empty(), "undo should rewind the full turn");
}

#[test]
fn a_new_game_switches_sides_and_lets_the_engine_open() {
    let server = TestServer::start();
    post(&server, "/api/move", r#"{"uci":"e2e4","revision":0}"#);

    let response = post(&server, "/api/new-game", r#"{"humanSide":"black"}"#);
    assert_eq!(response.status, 200);
    assert_eq!(
        response.json().get("humanSide").and_then(Value::as_str),
        Some("black")
    );

    let state = wait_for_state(
        &server,
        |state| matches!(state.get("moveHistory"), Some(Value::Array(history)) if !history.is_empty()),
    );
    assert_eq!(
        state.get("sideToMove").and_then(Value::as_str),
        Some("black")
    );
}

#[test]
fn rejects_stale_illegal_and_out_of_turn_commands() {
    let server = TestServer::start();

    let stale = post(&server, "/api/move", r#"{"uci":"e2e4","revision":99}"#);
    assert_eq!(stale.status, 409);
    assert_eq!(stale.error_code(), "stale_revision");

    let illegal = post(&server, "/api/move", r#"{"uci":"e2e5","revision":0}"#);
    assert_eq!(illegal.status, 409);
    assert_eq!(illegal.error_code(), "illegal_move");

    let nothing = post(&server, "/api/undo", r#"{"revision":0}"#);
    assert_eq!(nothing.status, 409);
    assert_eq!(nothing.error_code(), "nothing_to_undo");

    // Moving for the engine's side while it is the engine's turn is refused.
    post(&server, "/api/move", r#"{"uci":"e2e4","revision":0}"#);
    let wrong_side = post(&server, "/api/move", r#"{"uci":"e7e5","revision":1}"#);
    assert_eq!(wrong_side.status, 409);
    assert!(
        ["not_human_turn", "stale_revision"].contains(&wrong_side.error_code().as_str()),
        "unexpected code {}",
        wrong_side.error_code()
    );
}

#[test]
fn rejects_malformed_and_incomplete_command_bodies() {
    let server = TestServer::start();
    for (body, status, code) in [
        ("not json", 400, "malformed_json"),
        ("[1,2]", 400, "malformed_json"),
        (r#"{"revision":0}"#, 422, "missing_uci"),
        (r#"{"uci":"e2e4"}"#, 422, "missing_revision"),
        (r#"{"uci":"e2e4","revision":-1}"#, 422, "missing_revision"),
        (r#"{"uci":"e2e4","revision":"0"}"#, 422, "missing_revision"),
        (r#"{"uci":42,"revision":0}"#, 422, "missing_uci"),
    ] {
        let response = post(&server, "/api/move", body);
        assert_eq!(response.status, status, "body {body}");
        assert_eq!(response.error_code(), code, "body {body}");
    }

    let bad_side = post(&server, "/api/new-game", r#"{"humanSide":"green"}"#);
    assert_eq!(bad_side.status, 422);
    assert_eq!(bad_side.error_code(), "invalid_human_side");

    // None of the rejected bodies advanced the game.
    assert_eq!(revision(&server), 0);
}

#[test]
fn requires_a_json_content_type_so_html_forms_cannot_reach_commands() {
    let server = TestServer::start();
    let response = request(
        server.addr,
        "POST",
        "/api/move",
        &[
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("X-Seaborg-Token", &server.token),
        ],
        Some("uci=e2e4&revision=0"),
    );
    assert_eq!(response.status, 415);
    assert_eq!(response.error_code(), "expected_json");
}

// -- security ---------------------------------------------------------------------------------

#[test]
fn mutating_requests_require_the_session_token() {
    let server = TestServer::start();
    let body = Some(r#"{"uci":"e2e4","revision":0}"#);

    let missing = request(
        server.addr,
        "POST",
        "/api/move",
        &[("Content-Type", "application/json")],
        body,
    );
    assert_eq!(missing.status, 403);
    assert_eq!(missing.error_code(), "invalid_token");

    let truncated = server.token[..16].to_owned();
    let extended = format!("{}x", server.token);
    for wrong in ["", "deadbeef", &"0".repeat(32), &truncated, &extended] {
        let response = request(
            server.addr,
            "POST",
            "/api/move",
            &[
                ("Content-Type", "application/json"),
                ("X-Seaborg-Token", wrong),
            ],
            body,
        );
        assert_eq!(response.status, 403, "token {wrong:?}");
        assert_eq!(response.error_code(), "invalid_token", "token {wrong:?}");
    }

    // None of the rejected attempts changed the game.
    assert_eq!(revision(&server), 0);
}

#[test]
fn rejects_unexpected_host_values() {
    let server = TestServer::start();
    let port = server.addr.port();
    for host in [
        "evil.example.com".to_owned(),
        format!("evil.example.com:{port}"),
        // A rebinding attacker's own name resolving to loopback still fails the allowlist.
        format!("rebind.evil:{port}"),
        // A mismatched port means the request was meant for a different local server.
        format!("127.0.0.1:{}", port.wrapping_add(1)),
        "127.0.0.1".to_owned(),
    ] {
        let response = request(server.addr, "GET", "/api/state", &[("Host", &host)], None);
        assert_eq!(response.status, 403, "host {host}");
        assert_eq!(response.error_code(), "forbidden_host", "host {host}");
    }

    let missing = send_raw(server.addr, b"GET /api/state HTTP/1.1\r\n\r\n");
    assert_eq!(missing.status, 403);
    assert_eq!(missing.error_code(), "missing_host");
}

#[test]
fn accepts_every_loopback_spelling_of_its_own_authority() {
    let server = TestServer::start();
    let port = server.addr.port();
    for host in [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ] {
        let response = request(server.addr, "GET", "/api/state", &[("Host", &host)], None);
        assert_eq!(response.status, 200, "host {host}");
    }
}

#[test]
fn rejects_unexpected_origin_values() {
    let server = TestServer::start();
    let port = server.addr.port();
    for origin in [
        "http://evil.example.com".to_owned(),
        format!("http://evil.example.com:{port}"),
        // HTTPS is never this server's own origin, so it is a cross-origin caller.
        format!("https://127.0.0.1:{port}"),
        format!("http://127.0.0.1:{}", port.wrapping_add(1)),
        "null".to_owned(),
    ] {
        let response = request(
            server.addr,
            "GET",
            "/api/state",
            &[("Origin", &origin)],
            None,
        );
        assert_eq!(response.status, 403, "origin {origin}");
        assert_eq!(response.error_code(), "forbidden_origin", "origin {origin}");
    }

    let own = format!("http://127.0.0.1:{port}");
    let response = request(server.addr, "GET", "/api/state", &[("Origin", &own)], None);
    assert_eq!(response.status, 200, "the page's own origin is allowed");
}

#[test]
fn a_cross_origin_page_cannot_command_the_engine_even_with_a_stolen_token() {
    let server = TestServer::start();
    let response = request(
        server.addr,
        "POST",
        "/api/move",
        &[
            ("Origin", "http://evil.example.com"),
            ("Content-Type", "application/json"),
            ("X-Seaborg-Token", &server.token),
        ],
        Some(r#"{"uci":"e2e4","revision":0}"#),
    );
    assert_eq!(response.status, 403);
    assert_eq!(response.error_code(), "forbidden_origin");
    assert_eq!(revision(&server), 0, "the game must be untouched");
}

// -- routing and limits -----------------------------------------------------------------------

#[test]
fn unknown_routes_are_not_found_and_no_path_traversal_is_possible() {
    let server = TestServer::start();
    for path in [
        "/nope",
        "/../Cargo.toml",
        "/../../etc/passwd",
        "/assets/index.html",
        "/api",
        "/api/",
        "/api/state/extra",
    ] {
        let response = get(&server, path);
        assert_eq!(response.status, 404, "path {path}");
        assert_eq!(response.error_code(), "not_found", "path {path}");
    }
}

#[test]
fn known_routes_reject_the_wrong_method() {
    let server = TestServer::start();
    for (method, path) in [
        ("POST", "/"),
        ("POST", "/app.js"),
        ("POST", "/board.js"),
        ("POST", "/pieces.svg"),
        ("POST", "/licenses"),
        ("POST", "/api/state"),
        ("DELETE", "/api/events"),
        ("GET", "/api/move"),
        ("PUT", "/api/undo"),
        ("GET", "/api/new-game"),
    ] {
        let response = request(server.addr, method, path, &[], None);
        assert_eq!(response.status, 405, "{method} {path}");
        assert_eq!(
            response.error_code(),
            "method_not_allowed",
            "{method} {path}"
        );
    }
}

#[test]
fn oversized_requests_are_refused_before_the_body_is_buffered() {
    let server = TestServer::start();
    let oversized = "x".repeat(MAX_REQUEST_BODY + 1);
    let response = post(&server, "/api/move", &oversized);
    assert_eq!(response.status, 413);

    let long_target = format!("/{}", "a".repeat(16 * 1024));
    let response = get(&server, &long_target);
    assert_eq!(response.status, 413);

    // A body at the limit is still parsed, and fails on its contents rather than its size.
    let command = r#"{"uci":"e2e4","revision":0}"#;
    let padding = " ".repeat(MAX_REQUEST_BODY - command.len());
    let at_limit = post(&server, "/api/move", &format!("{command}{padding}"));
    assert_eq!(
        at_limit.status, 200,
        "a body at the limit should be accepted"
    );

    // The server survived all of it.
    assert_eq!(get(&server, "/api/state").status, 200);
}

#[test]
fn a_malformed_request_line_is_answered_and_does_not_stop_the_server() {
    let server = TestServer::start();
    let response = send_raw(server.addr, b"GET\r\n\r\n");
    assert_eq!(response.status, 400);
    assert_eq!(get(&server, "/api/state").status, 200);
}

// -- server-sent events -----------------------------------------------------------------------

#[test]
fn a_new_stream_receives_the_current_state_then_live_updates() {
    let server = TestServer::start();
    let mut stream = EventStream::open(&server, &[]);

    let (first_id, first) = stream.next_event();
    assert_eq!(first.get("revision").and_then(Value::as_u64), Some(0));

    post(&server, "/api/move", r#"{"uci":"e2e4","revision":0}"#);

    let (next_id, next) = stream.next_event();
    assert!(next_id > first_id, "event ids must increase");
    assert_eq!(next.get("revision").and_then(Value::as_u64), Some(1));
    assert_eq!(
        next.get("lastMove")
            .and_then(|last| last.get("san"))
            .and_then(Value::as_str),
        Some("e4")
    );
}

#[test]
fn the_stream_head_declares_event_stream_framing_and_a_retry_delay() {
    let server = TestServer::start();
    let mut stream = TcpStream::connect_timeout(&server.addr, Duration::from_secs(5)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    write!(
        stream,
        "GET /api/events HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
        server.addr.port()
    )
    .unwrap();

    let mut reader = BufReader::new(stream);
    let mut head = String::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        head.push_str(&line);
    }
    assert!(head.contains("200 OK"), "{head}");
    assert!(
        head.contains("Content-Type: text/event-stream; charset=utf-8"),
        "{head}"
    );
    assert!(head.contains("Cache-Control: no-store"), "{head}");

    let mut retry = String::new();
    reader.read_line(&mut retry).unwrap();
    assert!(
        retry.starts_with("retry: "),
        "expected a retry line, got {retry:?}"
    );
}

#[test]
fn a_reconnecting_stream_resumes_from_its_last_event_id() {
    let server = TestServer::start();

    // Establish a stream, advance the game, then drop it as a reload would.
    let mut first = EventStream::open(&server, &[]);
    let (initial_id, _) = first.next_event();
    post(&server, "/api/move", r#"{"uci":"e2e4","revision":0}"#);
    first.next_event();
    drop(first);

    // Reconnecting with a stale ID is brought straight up to date.
    let mut stale = EventStream::open(&server, &[("Last-Event-ID", &initial_id.to_string())]);
    let (resumed_id, resumed) = stale.next_event();
    assert!(
        resumed_id > initial_id,
        "expected catch-up past {initial_id}"
    );
    assert!(
        resumed
            .get("revision")
            .and_then(Value::as_u64)
            .is_some_and(|revision| revision >= 1),
        "the resumed snapshot should be current"
    );
    drop(stale);

    // Let the engine finish so no in-flight search publishes during the check below.
    wait_for_state(&server, |state| {
        state
            .get("engineStatus")
            .and_then(|engine| engine.get("kind"))
            .and_then(Value::as_str)
            == Some("idle")
    });

    // Reconnecting already up to date replays nothing: every event that follows is newer than
    // the one the client already had, and the new game is among them.
    let (latest_id, _) = EventStream::open(&server, &[]).next_event();
    let mut current = EventStream::open(&server, &[("Last-Event-ID", &latest_id.to_string())]);
    post(&server, "/api/new-game", r#"{"humanSide":"white"}"#);

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let (after_id, after) = current.next_event();
        assert!(after_id > latest_id, "expected only newer events");
        if matches!(after.get("moveHistory"), Some(Value::Array(history)) if history.is_empty()) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the new game never reached the stream"
        );
    }
}

#[test]
fn a_last_event_id_from_a_previous_process_still_receives_current_state() {
    // Event IDs restart at zero in each process, so a tab left open across a restart reconnects
    // quoting the old process's IDs. Trusting such an ID would leave that tab waiting for events
    // this session will not produce for a long time, rendering stale state the whole while.
    let server = TestServer::start();
    let mut stream = EventStream::open(&server, &[("Last-Event-ID", "999999")]);

    let (id, state) = stream.next_event();
    assert_eq!(id, server_event_id(&server));
    assert_eq!(state.get("revision").and_then(Value::as_u64), Some(0));

    // It is a working stream, not just a one-off snapshot.
    post(&server, "/api/move", r#"{"uci":"e2e4","revision":0}"#);
    let (next_id, next) = stream.next_event();
    assert!(next_id > id);
    assert_eq!(next.get("revision").and_then(Value::as_u64), Some(1));
}

/// The event ID a fresh stream is currently served, used to compare resume behaviour.
fn server_event_id(server: &TestServer) -> u64 {
    EventStream::open(server, &[]).next_event().0
}

#[test]
fn a_stream_already_holding_the_newest_event_is_sent_nothing_until_something_changes() {
    let server = TestServer::start();
    let current = server_event_id(&server);
    let mut stream = EventStream::open(&server, &[("Last-Event-ID", &current.to_string())]);

    // Only the retry line should arrive; a snapshot here would mean state was replayed.
    let retry = stream.next_line();
    assert!(retry.starts_with("retry: "), "got {retry:?}");
    assert_eq!(stream.next_line(), "\n");

    stream
        .reader
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let mut line = String::new();
    let outcome = stream.reader.read_line(&mut line);
    assert!(outcome.is_err(), "expected no replayed data, got {line:?}");
}

#[test]
fn many_concurrent_streams_all_observe_the_same_update() {
    let server = TestServer::start();
    let mut streams: Vec<EventStream> = (0..8).map(|_| EventStream::open(&server, &[])).collect();
    for stream in &mut streams {
        let (_, initial) = stream.next_event();
        assert_eq!(initial.get("revision").and_then(Value::as_u64), Some(0));
    }

    post(&server, "/api/move", r#"{"uci":"d2d4","revision":0}"#);

    for stream in &mut streams {
        let (_, update) = stream.next_event();
        assert_eq!(update.get("revision").and_then(Value::as_u64), Some(1));
    }
}

#[test]
fn search_progress_reaches_the_stream_while_the_engine_thinks() {
    let server = TestServer::start();
    let mut stream = EventStream::open(&server, &[]);
    stream.next_event();
    post(&server, "/api/move", r#"{"uci":"e2e4","revision":0}"#);

    // Progress updates share a revision with the move that started the search, which is why the
    // stream is keyed on an event id rather than the game revision.
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut ids = Vec::new();
    let mut saw_reply = false;
    while Instant::now() < deadline && !saw_reply {
        let (id, event) = stream.next_event();
        ids.push(id);
        saw_reply = event
            .get("revision")
            .and_then(Value::as_u64)
            .is_some_and(|revision| revision >= 2);
    }
    assert!(saw_reply, "the engine's reply should reach the stream");
    assert!(ids.windows(2).all(|pair| pair[1] > pair[0]), "ids: {ids:?}");
}

// --- Connection-flood and drain regressions ---------------------------------------------------

/// The accept loop is capped, so a flood is refused rather than accumulating threads.
///
/// The original loop called `thread::spawn` per connection with no cap. `thread::spawn` panics
/// when the OS refuses a thread, and it ran on the accept-loop thread, so the panic unwound
/// `UiServer::run` and killed the engine process mid-game. This asserts the bound that keeps that
/// condition out of reach: connections past the cap are turned away with a status, and — the part
/// that actually mattered — the server is still serving afterwards.
#[test]
fn connections_past_the_cap_are_refused_and_the_server_keeps_serving() {
    let server = TestServer::start();

    // Hold the cap open with bare sockets that send nothing, which is all the original defect
    // needed: the token and Origin gates both run after the connection thread already exists.
    let mut held = Vec::new();
    for _ in 0..MAX_CONNECTIONS {
        held.push(TcpStream::connect_timeout(&server.addr, Duration::from_secs(5)).unwrap());
    }

    // The permit is claimed on the accept-loop thread, but the connections above are only counted
    // once each has been accepted, so allow the loop a moment to drain its backlog.
    let deadline = Instant::now() + Duration::from_secs(10);
    let refused = loop {
        let response = get(&server, "/api/state");
        if response.status == 503 {
            break response;
        }
        assert!(
            Instant::now() < deadline,
            "the server should reach its connection cap, got {}",
            response.status
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(refused.error_code(), "too_many_connections");

    // Releasing the held connections returns their slots and ordinary service resumes. This is
    // the assertion the defect failed: the process was gone by this point.
    drop(held);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let response = get(&server, "/api/state");
        if response.status == 200 {
            assert!(response.json().get("fen").is_some());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the server should serve again once connections close"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Draining a rejected request is bounded by elapsed time, not just by bytes.
///
/// `DRAIN_TIMEOUT` was installed as a per-read socket timeout, so a client delivering one byte
/// inside each timeout kept the drain productive all the way to the 1 MiB cap — on the order of
/// weeks on one thread. The drain now carries an absolute deadline, so a dripping client is cut
/// off shortly after it, having sent nowhere near the byte cap.
#[test]
fn a_dripping_client_cannot_hold_a_thread_open_after_its_request_is_rejected() {
    let server = TestServer::start();

    let mut stream = TcpStream::connect_timeout(&server.addr, Duration::from_secs(5)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();

    // Declare a body over the limit so the request is rejected before the body is read, which is
    // what puts the connection on the drain path with data still queued.
    let oversized = MAX_REQUEST_BODY + 1;
    stream
        .write_all(
            format!(
                "POST /api/move HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\
                 Content-Type: application/json\r\nContent-Length: {oversized}\r\n\r\n",
                server.addr.port()
            )
            .as_bytes(),
        )
        .unwrap();
    stream.flush().unwrap();

    // Drip a byte at a time, slowly enough that a per-read socket timeout would keep resetting
    // and the drain would stay productive all the way to its byte cap.
    let started = Instant::now();
    let limit = Duration::from_secs(12);
    let mut sent = 0_usize;
    while started.elapsed() < limit {
        // The server abandons the drain and closes with this client's bytes still queued, so the
        // socket dies by RST and writing to it starts failing.
        if stream.write_all(b"x").is_err() || stream.flush().is_err() {
            break;
        }
        sent += 1;
        std::thread::sleep(Duration::from_millis(200));
    }

    let elapsed = started.elapsed();
    assert!(
        elapsed < limit,
        "the drain should end on its deadline, not run for as long as the client drips"
    );
    assert!(
        sent < oversized,
        "the connection should close long before the declared {oversized}-byte body arrives, \
         got {sent} bytes in {elapsed:?}"
    );
    // A client that keeps sending past the deadline forfeits its response to that reset, which is
    // the intended trade: the thread matters more than the courtesy. A client that stops sending
    // still gets its 413 — `oversized_requests_are_refused_before_the_body_is_buffered` covers it.
}

// -- engine limit, quit, reconnection, and complete games ---------------------------------------

/// Read the engine limit the server reports it will use for the next turn.
fn engine_limit(server: &TestServer) -> (String, u64) {
    let state = get(server, "/api/state").json();
    let limit = state.get("engineLimit").expect("a published engine limit");
    let kind = limit
        .get("kind")
        .and_then(Value::as_str)
        .expect("a limit kind")
        .to_owned();
    let amount = limit
        .get("milliseconds")
        .or_else(|| limit.get("plies"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    (kind, amount)
}

#[test]
fn the_snapshot_publishes_the_limit_the_next_engine_turn_will_use() {
    let server = TestServer::start();
    assert_eq!(engine_limit(&server), ("depth".to_owned(), 1));
}

#[test]
fn a_new_engine_limit_is_accepted_and_published_to_every_client() {
    let server = TestServer::start();
    let mut stream = EventStream::open(&server, &[]);
    let (_, first) = stream.next_event();
    assert_eq!(
        first
            .get("engineLimit")
            .and_then(|limit| limit.get("kind"))
            .and_then(Value::as_str),
        Some("depth")
    );

    let response = post(
        &server,
        "/api/engine-limit",
        r#"{"kind":"time","value":750}"#,
    );
    assert_eq!(response.status, 200);
    assert_eq!(engine_limit(&server), ("time".to_owned(), 750));

    // A setting is part of the authoritative snapshot, so a second tab learns about it without
    // being told: the change is published even though no move was made.
    let (_, updated) = stream.next_event();
    let limit = updated.get("engineLimit").expect("a limit on the stream");
    assert_eq!(limit.get("kind").and_then(Value::as_str), Some("time"));
    assert_eq!(limit.get("milliseconds").and_then(Value::as_u64), Some(750));

    // The revision tracks the game, not the settings, so changing a limit must not look to the
    // client like the position moved on and invalidate the command it is about to send.
    assert_eq!(revision(&server), 0);
    assert_eq!(
        post(&server, "/api/move", r#"{"uci":"e2e4","revision":0}"#).status,
        200
    );
}

#[test]
fn out_of_range_and_malformed_engine_limits_are_refused() {
    let server = TestServer::start();
    for body in [
        r#"{"kind":"time","value":10}"#,
        r#"{"kind":"time","value":600000}"#,
        r#"{"kind":"depth","value":0}"#,
        r#"{"kind":"depth","value":64}"#,
        // An unlimited search would never return a move and would lock the board for good.
        r#"{"kind":"infinite","value":1}"#,
        r#"{"kind":"nonsense","value":1}"#,
    ] {
        let response = post(&server, "/api/engine-limit", body);
        assert_eq!(response.status, 422, "{body}");
        assert_eq!(response.error_code(), "invalid_engine_limit", "{body}");
    }

    for body in [r#"{"kind":"time"}"#, r#"{"value":100}"#, "{}"] {
        let response = post(&server, "/api/engine-limit", body);
        assert_eq!(response.status, 422, "{body}");
        assert_eq!(response.error_code(), "missing_engine_limit", "{body}");
    }

    // Every rejection leaves the configured limit alone.
    assert_eq!(engine_limit(&server), ("depth".to_owned(), 1));
}

#[test]
fn an_engine_limit_change_needs_the_session_token() {
    let server = TestServer::start();
    let response = request(
        server.addr,
        "POST",
        "/api/engine-limit",
        &[("Content-Type", "application/json")],
        Some(r#"{"kind":"time","value":750}"#),
    );
    assert_eq!(response.status, 403);
    assert_eq!(response.error_code(), "invalid_token");
    assert_eq!(engine_limit(&server), ("depth".to_owned(), 1));
}

#[test]
fn quit_answers_before_stopping_the_server() {
    let mut server = TestServer::start();

    // The browser must learn the request was accepted. Shutting down first would close this
    // socket with the reply still queued, which reads to the page as a lost connection.
    let response = post(&server, "/api/quit", "{}");
    assert_eq!(response.status, 200);
    assert_eq!(response.json().get("quitting"), Some(&Value::Bool(true)));

    // `stop` joins the serving thread, so this returning at all is the assertion: the accept loop
    // observed the request and returned on its own, without the handle being used to wake it.
    server.stop();
}

#[test]
fn quit_needs_the_session_token() {
    let server = TestServer::start();
    let response = request(
        server.addr,
        "POST",
        "/api/quit",
        &[("Content-Type", "application/json")],
        Some("{}"),
    );
    assert_eq!(response.status, 403);
    assert_eq!(response.error_code(), "invalid_token");
    // The server is still serving, so an unauthenticated peer cannot stop the process.
    assert_eq!(get(&server, "/api/state").status, 200);
}

#[test]
fn reloading_during_a_search_reconstructs_the_game_without_duplicating_it() {
    init_globals();
    let server = TestServer::start_with(&UiConfig {
        // Long enough that the search is still running across the whole reload.
        search_limit: SearchLimit::Time(Duration::from_secs(3)),
        ..test_config()
    })
    .unwrap();

    assert_eq!(
        post(&server, "/api/move", r#"{"uci":"e2e4","revision":0}"#).status,
        200
    );
    let thinking = wait_for_state(&server, |state| {
        state
            .get("engineStatus")
            .and_then(|status| status.get("kind"))
            .and_then(Value::as_str)
            == Some("thinking")
    });
    let search_id = thinking
        .get("engineStatus")
        .and_then(|status| status.get("searchId"))
        .and_then(Value::as_u64)
        .expect("a search id");

    // A reload is a fresh page: it fetches the state and opens a new stream. Neither is a command,
    // so neither may start a second search or replay the move that is already on the board.
    for _ in 0..3 {
        let state = get(&server, "/api/state").json();
        let mut stream = EventStream::open(&server, &[]);
        let (_, streamed) = stream.next_event();
        for reconstructed in [&state, &streamed] {
            assert_eq!(
                reconstructed.get("revision").and_then(Value::as_u64),
                Some(1)
            );
            assert_eq!(
                reconstructed
                    .get("engineStatus")
                    .and_then(|status| status.get("searchId"))
                    .and_then(Value::as_u64),
                Some(search_id),
                "a reload must attach to the running search rather than start another"
            );
            let Some(Value::Array(history)) = reconstructed.get("moveHistory") else {
                panic!("expected a move history");
            };
            assert_eq!(history.len(), 1, "the move must not be duplicated");
            assert_eq!(
                history[0].get("san").and_then(Value::as_str),
                Some("e4"),
                "history is reconstructed in SAN"
            );
        }
    }

    // The single search still completes into a single reply.
    let settled = wait_for_state(&server, |state| {
        state.get("revision").and_then(Value::as_u64) == Some(2)
    });
    let Some(Value::Array(history)) = settled.get("moveHistory") else {
        panic!("expected a move history");
    };
    assert_eq!(history.len(), 2);
}

#[test]
fn a_thinking_snapshot_carries_a_san_variation_the_browser_can_show() {
    init_globals();
    let server = TestServer::start_with(&UiConfig {
        search_limit: SearchLimit::Time(Duration::from_secs(3)),
        ..test_config()
    })
    .unwrap();
    assert_eq!(
        post(&server, "/api/move", r#"{"uci":"e2e4","revision":0}"#).status,
        200
    );

    let state = wait_for_state(&server, |state| {
        state
            .get("engineStatus")
            .and_then(|status| status.get("principalVariationSan"))
            .is_some_and(|san| matches!(san, Value::Array(items) if !items.is_empty()))
    });
    let status = state.get("engineStatus").unwrap();
    let Some(Value::Array(san)) = status.get("principalVariationSan") else {
        panic!("expected a SAN variation");
    };
    let Some(Value::Array(uci)) = status
        .get("progress")
        .and_then(|progress| progress.get("principalVariation"))
    else {
        panic!("expected a UCI variation");
    };
    // The SAN line is the same line, truncated only where a reported move is not playable.
    assert!(!san.is_empty());
    assert!(san.len() <= uci.len());
    for entry in san {
        let text = entry.as_str().expect("SAN is a string");
        assert!(!text.is_empty());
        // SAN never spells a move as four coordinate characters; that is the UCI form.
        assert!(
            text.len() < 4 || text.contains(['x', '+', '#', '=', 'O']),
            "expected SAN, got {text:?}"
        );
    }
}

#[test]
fn a_complete_game_can_be_played_to_a_terminal_status_over_the_command_surface() {
    let server = TestServer::start();

    // Play the human side by always taking the first legal move the server offers. The point is
    // not the quality of the moves but that the surface sustains a whole game: every command is
    // accepted, the engine answers each one, and the game ends in a status the UI can report.
    let mut plies = 0;
    let terminal = loop {
        let state = wait_for_state(&server, |state| {
            state
                .get("engineStatus")
                .and_then(|status| status.get("kind"))
                .and_then(Value::as_str)
                == Some("idle")
        });
        let status = state
            .get("gameStatus")
            .and_then(|status| status.get("kind"));
        if status.and_then(Value::as_str) != Some("ongoing") {
            break state;
        }

        assert_eq!(
            state.get("sideToMove").and_then(Value::as_str),
            Some("white"),
            "the human is on move whenever the engine is idle and the game is live"
        );
        let Some(Value::Array(moves)) = state.get("legalMoves") else {
            panic!("expected legal moves");
        };
        let uci = moves[0].as_str().expect("a UCI move").to_owned();
        let at = state.get("revision").and_then(Value::as_u64).unwrap();
        let response = post(
            &server,
            "/api/move",
            &format!(r#"{{"uci":"{uci}","revision":{at}}}"#),
        );
        assert_eq!(
            response.status, 200,
            "move {uci} at revision {at} was refused"
        );

        plies += 1;
        // The fifty-move rule alone bounds this well inside the cap; it exists so a regression
        // that stops the game advancing fails the test rather than hanging it.
        assert!(plies < 400, "the game did not reach a terminal status");
    };

    let kind = terminal
        .get("gameStatus")
        .and_then(|status| status.get("kind"))
        .and_then(Value::as_str)
        .expect("a terminal status");
    assert!(
        kind == "checkmate" || kind == "draw",
        "expected a terminal status, got {kind:?}"
    );

    // A finished game refuses further moves with a code the browser turns into a sentence, rather
    // than silently accepting them.
    let at = terminal.get("revision").and_then(Value::as_u64).unwrap();
    let Some(Value::Array(moves)) = terminal.get("legalMoves") else {
        panic!("expected a legal move list");
    };
    if let Some(uci) = moves.first().and_then(Value::as_str) {
        let response = post(
            &server,
            "/api/move",
            &format!(r#"{{"uci":"{uci}","revision":{at}}}"#),
        );
        assert_eq!(response.status, 409);
        assert_eq!(response.error_code(), "game_over");
    }
}
