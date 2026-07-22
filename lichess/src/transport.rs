//! HTTP transport abstraction.
//!
//! [`Transport`] is the seam between the bot logic and the network. The real
//! implementation, [`HttpTransport`], talks to Lichess over a single shared
//! `ureq::Agent` (one connection pool for the whole bot).
//!
//! Most calls turn an HTTP 429 into a bounded, shutdown-aware backoff here, which
//! suits an endpoint whose limits clear in seconds. Some limits do not: Lichess
//! caps outgoing challenges and bot-versus-bot games per day, and a request over
//! one of those can be refused for hours. Waiting that out inside a call would
//! strand the caller on a request certain to fail, so those callers use the
//! non-retrying entry point and act on the refusal themselves. Either way the
//! response body is parsed for the rate-limit detail Lichess supplies — which
//! limit fired, and how long it lasts.
//!
//! Tests substitute a fake transport that replays recorded NDJSON and records the
//! requests the bot makes, so challenge and game handling run with no network
//! access.

use std::io::BufRead;
use std::time::Duration;

use serde::Deserialize;

use crate::backoff::Backoff;
use crate::error::{Error, Result};
use crate::shutdown::Shutdown;

/// Longest wait to open a connection (TCP plus TLS handshake) before giving up
/// and letting the caller's reconnect backoff take over.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Longest wait for a response after the request is sent, applied to ordinary
/// request/response calls so a hung server cannot block them indefinitely.
///
/// Despite ureq naming this "recv response", it is not a header-only bound: the
/// deadline it establishes stays active as a preceding phase while the body is
/// read, so it also caps body reception. For a long-lived NDJSON stream that
/// receives keepalives every few seconds but no full "response" for minutes,
/// this fires mid-stream and tears down a healthy connection. The streaming path
/// (`open_stream`) therefore clears this per request, matching `curl`, which
/// applies no receive timeout to a stream and lets TCP failure end a dead one.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);

/// First wait applied after an HTTP 429, before any doubling. Lichess asks
/// clients to wait about a minute on a 429, so honor that as the floor.
const RATE_LIMIT_BASE: Duration = Duration::from_secs(60);
/// Ceiling for the 429 backoff.
const RATE_LIMIT_MAX: Duration = Duration::from_secs(600);
/// How many times a single request is retried through 429s before giving up and
/// surfacing the rate-limit error to the caller.
const RATE_LIMIT_MAX_ATTEMPTS: u32 = 5;

/// The HTTP operations the bot needs from Lichess.
///
/// Paths are relative to the API origin (for example `/api/account`); the
/// implementation joins them to its base URL. Bodies are returned as raw
/// strings and decoded by the caller, keeping this trait free of Lichess types.
pub trait Transport {
    /// Perform a GET and return the full response body.
    fn get(&self, path: &str) -> Result<String>;

    /// Perform a POST with no request body and return the response body.
    fn post_empty(&self, path: &str) -> Result<String>;

    /// Perform a POST with a URL-encoded form body.
    fn post_form(&self, path: &str, form: &[(&str, &str)]) -> Result<String>;

    /// Perform a POST with a URL-encoded form body, surfacing a rate-limit
    /// response to the caller instead of waiting it out here.
    ///
    /// Retrying inside the transport is right only when the wait is short and the
    /// caller has nothing better to do meanwhile. Neither holds for a request the
    /// server may refuse for hours: the caller is the only party that knows how
    /// long the limit lasts (the response says so), can spend the interval on
    /// other work, and can avoid re-sending a request that is certain to fail.
    /// Such callers use this instead of [`post_form`](Transport::post_form).
    fn post_form_once(&self, path: &str, form: &[(&str, &str)]) -> Result<String>;

    /// Open a streaming endpoint and yield its response one line at a time.
    ///
    /// Lichess NDJSON streams emit one JSON object per line and blank lines as
    /// keepalives; both are yielded verbatim for the caller to interpret.
    fn open_stream(&self, path: &str) -> Result<Box<dyn Iterator<Item = Result<String>>>>;
}

/// A [`Transport`] backed by a shared `ureq::Agent`, authenticated with a bearer
/// token.
pub struct HttpTransport {
    agent: ureq::Agent,
    base_url: String,
    bearer: String,
    shutdown: Shutdown,
}

impl HttpTransport {
    /// Build a transport for `base_url` authenticating every request with
    /// `token`, sharing one connection pool and honoring `shutdown` while waiting
    /// out a rate-limit backoff.
    pub fn new(
        base_url: impl Into<String>,
        token: impl AsRef<str>,
        shutdown: Shutdown,
    ) -> HttpTransport {
        Self::with_response_timeout(base_url, token, shutdown, RESPONSE_TIMEOUT)
    }

    /// Build a transport whose shared agent bounds response reception by
    /// `response_timeout`. Factored out of [`HttpTransport::new`] so tests can
    /// drive the timeout down to a few hundred milliseconds and exercise it
    /// against a local server without waiting on the production 15s bound.
    fn with_response_timeout(
        base_url: impl Into<String>,
        token: impl AsRef<str>,
        shutdown: Shutdown,
        response_timeout: Duration,
    ) -> HttpTransport {
        // `http_status_as_error(false)` is what lets this crate inspect the
        // status itself, so a 429 can be told apart from other 4xx and mapped to
        // a retryable error instead of an opaque failure.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_response(Some(response_timeout))
            .build()
            .into();
        HttpTransport {
            agent,
            base_url: base_url.into(),
            bearer: format!("Bearer {}", token.as_ref()),
            shutdown,
        }
    }

    /// Join the API origin with a relative request path.
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Run `op`, retrying through HTTP 429 responses with backoff until it
    /// succeeds, the attempt budget is spent, or shutdown is requested.
    fn with_rate_limit_retry<T>(&self, op: impl FnMut() -> Result<T>) -> Result<T> {
        with_rate_limit_retry(
            &self.shutdown,
            |wait| self.shutdown.sleep(wait),
            Backoff::new(RATE_LIMIT_BASE, RATE_LIMIT_MAX),
            RATE_LIMIT_MAX_ATTEMPTS,
            op,
        )
    }

    /// Send one form POST, shared by the retrying and non-retrying entry points so
    /// they can differ in retry policy alone.
    fn send_form(&self, url: &str, form: &[(&str, &str)]) -> Result<String> {
        let response = self
            .agent
            .post(url)
            .header("Authorization", &self.bearer)
            .send_form(form.iter().copied());
        read_response(response)
    }
}

impl Transport for HttpTransport {
    fn get(&self, path: &str) -> Result<String> {
        let url = self.url(path);
        self.with_rate_limit_retry(|| {
            let response = self
                .agent
                .get(url.as_str())
                .header("Authorization", &self.bearer)
                .call();
            read_response(response)
        })
    }

    fn post_empty(&self, path: &str) -> Result<String> {
        let url = self.url(path);
        self.with_rate_limit_retry(|| {
            let response = self
                .agent
                .post(url.as_str())
                .header("Authorization", &self.bearer)
                .send_empty();
            read_response(response)
        })
    }

    fn post_form(&self, path: &str, form: &[(&str, &str)]) -> Result<String> {
        let url = self.url(path);
        self.with_rate_limit_retry(|| self.send_form(&url, form))
    }

    fn post_form_once(&self, path: &str, form: &[(&str, &str)]) -> Result<String> {
        self.send_form(&self.url(path), form)
    }

    fn open_stream(&self, path: &str) -> Result<Box<dyn Iterator<Item = Result<String>>>> {
        let url = self.url(path);
        self.with_rate_limit_retry(|| {
            // Clear the shared agent's recv-response deadline for this request
            // only. That deadline stays active while the body is read, so on a
            // long-lived NDJSON stream it would fire mid-stream and kill a
            // healthy connection; a dropped stream is still ended by TCP-level
            // failure, which surfaces here as an error the caller reconnects on.
            let response = self
                .agent
                .get(url.as_str())
                .config()
                .timeout_recv_response(None)
                .build()
                .header("Authorization", &self.bearer)
                .call();
            let response = check_status(response)?;
            let reader = std::io::BufReader::new(response.into_body().into_reader());
            let lines = reader
                .lines()
                .map(|line| line.map_err(|e| Error::Http(e.to_string())));
            Ok(Box::new(lines) as Box<dyn Iterator<Item = Result<String>>>)
        })
    }
}

/// Retry `op` through HTTP 429s with `backoff`, waiting via `sleep`.
///
/// Any error other than [`Error::RateLimited`] propagates at once. After each
/// 429 the wait is the server's `Retry-After` when present, else the next
/// backoff step. The loop stops once `max_attempts` requests have been made or
/// shutdown is requested, returning the last rate-limit error so the caller can
/// decide what to do.
fn with_rate_limit_retry<T>(
    shutdown: &Shutdown,
    mut sleep: impl FnMut(Duration),
    mut backoff: Backoff,
    max_attempts: u32,
    mut op: impl FnMut() -> Result<T>,
) -> Result<T> {
    let mut attempt = 1u32;
    loop {
        let (key, retry_after) = match op() {
            Err(Error::RateLimited { key, retry_after }) => (key, retry_after),
            other => return other,
        };
        if attempt >= max_attempts || shutdown.is_requested() {
            return Err(Error::RateLimited { key, retry_after });
        }
        sleep(retry_after.unwrap_or_else(|| backoff.next_delay()));
        attempt += 1;
    }
}

/// Map a completed request to a body string or a typed error by its status.
fn read_response(
    result: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<String> {
    let mut response = check_status(result)?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| Error::Http(e.to_string()))
}

/// Turn a completed request into its response, or a typed error for the statuses
/// the bot handles specially. A transport-level failure (connection, TLS) and any
/// unhandled non-success status both become [`Error::Http`].
fn check_status(
    result: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<ureq::http::Response<ureq::Body>> {
    let response = result.map_err(|e| Error::Http(e.to_string()))?;
    match response.status().as_u16() {
        200..=299 => Ok(response),
        401 => Err(Error::Unauthorized),
        // 404 is a distinct, expected outcome on the challenge-accept path (the
        // challenge was canceled or expired before the accept landed), so it gets
        // its own variant the caller can single out instead of an opaque body.
        404 => Err(Error::NotFound),
        429 => {
            // The header is the fallback, not the primary source: for the limits a
            // bot actually meets, Lichess states the wait in the body and sends no
            // `Retry-After` at all. Read the header before consuming the response,
            // then let the body override it.
            let header_wait = retry_after(&response);
            let limit = response
                .into_body()
                .read_to_string()
                .ok()
                .and_then(|body| parse_rate_limit(&body));
            Err(Error::RateLimited {
                key: limit.as_ref().map(|l| l.key.clone()),
                retry_after: limit.and_then(|l| l.retry_after).or(header_wait),
            })
        }
        // Lichess explains a rejected request in the response body (typically
        // `{"error":"..."}`), which is the only thing that says *why* a 400
        // happened. Read it so the reason reaches the caller instead of a bare
        // status code.
        other => {
            let body = response.into_body().read_to_string().ok();
            // A 400 carrying a rate-limit envelope is not a rejected request at
            // all: it reports that the account this request names has exhausted a
            // Lichess allowance. Only that account is affected, so it gets its own
            // variant rather than being folded into an opaque HTTP failure.
            if other == 400 {
                if let Some(limit) = body.as_deref().and_then(parse_rate_limit) {
                    return Err(Error::OpponentRateLimited {
                        key: limit.key,
                        retry_after: limit.retry_after,
                    });
                }
            }
            Err(unexpected_status_error(other, body.as_deref()))
        }
    }
}

/// The rate-limit detail Lichess attaches to a limited response.
struct RateLimitBody {
    /// Lichess's identifier for the limit, such as `bot.vsBot.day`.
    key: String,
    /// How long until the limit clears.
    retry_after: Option<Duration>,
}

/// The shape Lichess uses to describe which limit was hit and for how long. It
/// appears on both the 429 that reports this account's own limit and the 400 that
/// reports the addressed account's, so one parser serves both.
#[derive(Deserialize)]
struct RateLimitEnvelope {
    ratelimit: RateLimitFields,
}

#[derive(Deserialize)]
struct RateLimitFields {
    key: String,
    /// Seconds until the limit clears. Modeled as optional so a response that
    /// omits it still yields the limit's identity rather than parsing to nothing.
    #[serde(default)]
    seconds: Option<u64>,
}

/// Extract the rate-limit detail from a response body, or `None` when the body is
/// not JSON or carries no rate-limit envelope.
fn parse_rate_limit(body: &str) -> Option<RateLimitBody> {
    let envelope: RateLimitEnvelope = serde_json::from_str(body).ok()?;
    Some(RateLimitBody {
        key: envelope.ratelimit.key,
        retry_after: envelope.ratelimit.seconds.map(Duration::from_secs),
    })
}

/// Longest body prefix folded into an [`Error::Http`]. Error bodies from Lichess
/// are small JSON objects; the cap keeps a misbehaving or unexpected endpoint
/// from flooding the log with a large body.
const MAX_ERROR_BODY_CHARS: usize = 500;

/// Build the error for an unhandled non-success status, folding in the response
/// body when the server sent a non-empty one. Kept separate from [`check_status`]
/// so the status-to-message mapping can be unit-tested without a live socket.
fn unexpected_status_error(status: u16, body: Option<&str>) -> Error {
    match body.map(str::trim).filter(|b| !b.is_empty()) {
        Some(body) => {
            let snippet: String = body.chars().take(MAX_ERROR_BODY_CHARS).collect();
            Error::Http(format!("unexpected status {status}: {snippet}"))
        }
        None => Error::Http(format!("unexpected status {status}")),
    }
}

/// Read a `Retry-After` header as a whole-second duration, if present and valid.
fn retry_after(response: &ureq::http::Response<ureq::Body>) -> Option<Duration> {
    let value = response.headers().get("retry-after")?.to_str().ok()?;
    let seconds = value.trim().parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    use super::*;

    /// Bind a throwaway HTTP/1.1 server on loopback that handles exactly one
    /// connection with `handle`, returning the port it listens on. The spawned
    /// thread owns the socket for the lifetime of the test; `handle` is expected
    /// to read the request and write the response.
    fn serve_once<F>(handle: F) -> u16
    where
        F: FnOnce(TcpStream) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("local addr").port();
        thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept one connection");
            handle(stream);
        });
        port
    }

    /// Drain the request header block so the response can be written without the
    /// client's send stalling. Reads until the CRLF-CRLF that ends the headers.
    fn read_request_headers(stream: &mut TcpStream) {
        let mut buf = [0u8; 1024];
        let mut seen = Vec::new();
        loop {
            let n = stream.read(&mut buf).expect("read request");
            if n == 0 {
                break;
            }
            seen.extend_from_slice(&buf[..n]);
            if seen.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
    }

    #[test]
    fn streaming_ignores_the_response_timeout_across_a_body_gap() {
        // A healthy Lichess stream sends a line, goes quiet for longer than the
        // agent's recv-response bound (as it does between keepalives), then sends
        // more. The streaming path clears that bound, so every line arrives; were
        // the bound still in force, the read would die during the silent gap.
        let response_timeout = Duration::from_millis(200);
        let gap = Duration::from_millis(600);
        let port = serve_once(move |mut stream| {
            read_request_headers(&mut stream);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\n\
                      Content-Type: application/x-ndjson\r\n\
                      Connection: close\r\n\r\n",
                )
                .unwrap();
            stream.write_all(b"line1\n").unwrap();
            stream.flush().unwrap();
            thread::sleep(gap);
            stream.write_all(b"line2\n").unwrap();
            stream.flush().unwrap();
        });
        let transport = HttpTransport::with_response_timeout(
            format!("http://127.0.0.1:{port}"),
            "token",
            Shutdown::new(),
            response_timeout,
        );
        let lines: Vec<String> = transport
            .open_stream("/stream")
            .expect("stream opens")
            .collect::<Result<Vec<_>>>()
            .expect("every line arrives despite the gap exceeding the response timeout");
        assert_eq!(lines, vec!["line1".to_string(), "line2".to_string()]);
    }

    #[test]
    fn a_non_streaming_get_still_times_out_when_the_body_stalls() {
        // Response headers arrive at once, but the body stalls past the agent's
        // recv-response bound. Ordinary calls keep that bound, so a wedged server
        // surfaces as an error instead of hanging the caller indefinitely.
        let response_timeout = Duration::from_millis(200);
        let stall = Duration::from_millis(800);
        let port = serve_once(move |mut stream| {
            read_request_headers(&mut stream);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                .unwrap();
            // Headers are in; withhold the body long enough to trip the bound.
            thread::sleep(stall);
            let _ = stream.write_all(b"late\n");
        });
        let transport = HttpTransport::with_response_timeout(
            format!("http://127.0.0.1:{port}"),
            "token",
            Shutdown::new(),
            response_timeout,
        );
        let result = transport.get("/slow");
        assert!(
            matches!(result, Err(Error::Http(_))),
            "a stalled body must surface as an error, got {result:?}"
        );
    }

    #[test]
    fn retries_through_a_429_then_succeeds() {
        // The first call is rate limited, the second succeeds. The recorded
        // waits show one backoff happened, honoring the server's Retry-After.
        let waits = RefCell::new(Vec::new());
        let calls = RefCell::new(0u32);
        let result = with_rate_limit_retry(
            &Shutdown::new(),
            |wait| waits.borrow_mut().push(wait),
            Backoff::new(Duration::from_secs(1), Duration::from_secs(30)),
            5,
            || {
                let mut calls = calls.borrow_mut();
                *calls += 1;
                if *calls == 1 {
                    Err(Error::RateLimited {
                        key: None,
                        retry_after: Some(Duration::from_secs(7)),
                    })
                } else {
                    Ok("ok".to_string())
                }
            },
        );
        assert_eq!(result.unwrap(), "ok");
        assert_eq!(waits.into_inner(), vec![Duration::from_secs(7)]);
    }

    #[test]
    fn gives_up_after_the_attempt_budget() {
        // Always rate limited: the op runs exactly `max_attempts` times and then
        // the rate-limit error surfaces.
        let calls = RefCell::new(0u32);
        let result = with_rate_limit_retry::<()>(
            &Shutdown::new(),
            |_| {},
            Backoff::new(Duration::from_secs(1), Duration::from_secs(30)),
            3,
            || {
                *calls.borrow_mut() += 1;
                Err(Error::RateLimited {
                    key: None,
                    retry_after: None,
                })
            },
        );
        assert!(matches!(result, Err(Error::RateLimited { .. })));
        assert_eq!(calls.into_inner(), 3, "one call per attempt, no more");
    }

    #[test]
    fn falls_back_to_backoff_when_no_retry_after() {
        // No Retry-After header: the wait comes from the doubling backoff.
        let waits = RefCell::new(Vec::new());
        let calls = RefCell::new(0u32);
        let _ = with_rate_limit_retry::<()>(
            &Shutdown::new(),
            |wait| waits.borrow_mut().push(wait),
            Backoff::new(Duration::from_secs(1), Duration::from_secs(30)),
            4,
            || {
                *calls.borrow_mut() += 1;
                Err(Error::RateLimited {
                    key: None,
                    retry_after: None,
                })
            },
        );
        assert_eq!(
            waits.into_inner(),
            vec![
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4)
            ]
        );
    }

    #[test]
    fn shutdown_stops_retrying_without_sleeping() {
        // Shutdown already requested: the op is tried once and the error is
        // returned without any wait.
        let shutdown = Shutdown::new();
        shutdown.request();
        let waits = RefCell::new(Vec::new());
        let calls = RefCell::new(0u32);
        let _ = with_rate_limit_retry::<()>(
            &shutdown,
            |wait| waits.borrow_mut().push(wait),
            Backoff::new(Duration::from_secs(1), Duration::from_secs(30)),
            5,
            || {
                *calls.borrow_mut() += 1;
                Err(Error::RateLimited {
                    key: None,
                    retry_after: None,
                })
            },
        );
        assert_eq!(calls.into_inner(), 1);
        assert!(waits.into_inner().is_empty());
    }

    #[test]
    fn unexpected_status_error_includes_the_response_body() {
        // The reason Lichess sends on a 400 must survive into the error message,
        // so a failed challenge logs why rather than just the status code.
        let error = unexpected_status_error(400, Some(r#"{"error":"Rated games require..."}"#));
        let Error::Http(message) = error else {
            panic!("expected Error::Http");
        };
        assert!(message.contains("400"), "status is reported: {message}");
        assert!(
            message.contains(r#"{"error":"Rated games require..."}"#),
            "body reaches the error: {message}"
        );
    }

    #[test]
    fn unexpected_status_error_omits_an_empty_body() {
        // A missing or blank body leaves a clean status-only message with no
        // dangling separator.
        assert!(matches!(
            unexpected_status_error(500, None),
            Error::Http(m) if m == "unexpected status 500"
        ));
        assert!(matches!(
            unexpected_status_error(500, Some("   \n")),
            Error::Http(m) if m == "unexpected status 500"
        ));
    }

    #[test]
    fn unexpected_status_error_caps_a_huge_body() {
        // An oversized body is truncated so it cannot flood the log.
        let huge = "x".repeat(MAX_ERROR_BODY_CHARS * 2);
        let Error::Http(message) = unexpected_status_error(400, Some(&huge)) else {
            panic!("expected Error::Http");
        };
        let body_len = message.len() - "unexpected status 400: ".len();
        assert_eq!(body_len, MAX_ERROR_BODY_CHARS);
    }

    #[test]
    fn a_non_rate_limit_error_propagates_immediately() {
        let calls = RefCell::new(0u32);
        let result = with_rate_limit_retry::<()>(
            &Shutdown::new(),
            |_| {},
            Backoff::new(Duration::from_secs(1), Duration::from_secs(30)),
            5,
            || {
                *calls.borrow_mut() += 1;
                Err(Error::Unauthorized)
            },
        );
        assert!(matches!(result, Err(Error::Unauthorized)));
        assert_eq!(calls.into_inner(), 1, "no retry on a non-429 error");
    }

    /// The body Lichess returns when an account is over the cap on games against
    /// other bots. The wait lives in the body; no `Retry-After` header accompanies
    /// it, which is why the body has to be read.
    const BOT_VS_BOT_BODY: &str = r#"{"error":"You played 100 games against other bots today, please wait before challenging another bot.","ratelimit":{"key":"bot.vsBot.day","seconds":7200}}"#;

    /// Bind a loopback server that answers every request with `status`, `body`, and
    /// any `extra_headers` (each already CRLF-terminated), closing each connection
    /// so one request means one accept. Returns the port and the count of requests
    /// served so far.
    fn serve_repeating(
        status: &'static str,
        extra_headers: &'static str,
        body: &'static str,
    ) -> (u16, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("local addr").port();
        let served = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&served);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                read_request_headers(&mut stream);
                counter.fetch_add(1, Ordering::SeqCst);
                let response = format!(
                    "HTTP/1.1 {status}\r\n\
                     Content-Type: application/json\r\n\
                     Content-Length: {}\r\n\
                     {extra_headers}\
                     Connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        (port, served)
    }

    fn transport_for(port: u16) -> HttpTransport {
        HttpTransport::new(format!("http://127.0.0.1:{port}"), "token", Shutdown::new())
    }

    #[test]
    fn a_non_retrying_post_surfaces_a_rate_limit_after_one_request() {
        // Sleeping out a limit that can stand for hours would strand the caller for
        // no benefit, and each re-send is a request the server has already refused.
        // The caller gets the refusal immediately and decides what to do with it.
        let (port, served) = serve_repeating("429 Too Many Requests", "", BOT_VS_BOT_BODY);
        let result = transport_for(port).post_form_once("/api/challenge/somebot", &[]);
        match result {
            Err(Error::RateLimited { key, retry_after }) => {
                assert_eq!(key.as_deref(), Some("bot.vsBot.day"));
                assert_eq!(retry_after, Some(Duration::from_secs(7200)));
            }
            other => panic!("expected a rate-limit error, got {other:?}"),
        }
        assert_eq!(
            served.load(Ordering::SeqCst),
            1,
            "the refused request must not be re-sent"
        );
    }

    #[test]
    fn a_rate_limit_body_outranks_a_retry_after_header() {
        // Both can be present. The body describes the specific limit that fired,
        // so it is the one to trust.
        let (port, _) = serve_repeating(
            "429 Too Many Requests",
            "Retry-After: 60\r\n",
            BOT_VS_BOT_BODY,
        );
        match transport_for(port).post_form_once("/api/challenge/somebot", &[]) {
            Err(Error::RateLimited { retry_after, .. }) => {
                assert_eq!(retry_after, Some(Duration::from_secs(7200)));
            }
            other => panic!("expected a rate-limit error, got {other:?}"),
        }
    }

    #[test]
    fn a_retry_after_header_still_applies_when_the_body_says_nothing() {
        let (port, _) = serve_repeating("429 Too Many Requests", "Retry-After: 45\r\n", "{}");
        match transport_for(port).post_form_once("/api/challenge/somebot", &[]) {
            Err(Error::RateLimited { key, retry_after }) => {
                assert_eq!(key, None);
                assert_eq!(retry_after, Some(Duration::from_secs(45)));
            }
            other => panic!("expected a rate-limit error, got {other:?}"),
        }
    }

    #[test]
    fn an_opponents_limit_arrives_as_a_400_carrying_the_same_envelope() {
        // Lichess reports the challenged account's exhausted allowance with a 400,
        // not a 429. Read as a plain rejection it would look like this bot's fault;
        // it is the opponent that is unavailable, and only for a stated while.
        const BODY: &str = r#"{"error":"someone played 100 games against other bots today, please wait until 2026-07-23 to challenge them.","ratelimit":{"key":"bot.vsBot.day","seconds":3600}}"#;
        let (port, _) = serve_repeating("400 Bad Request", "", BODY);
        match transport_for(port).post_form_once("/api/challenge/somebot", &[]) {
            Err(Error::OpponentRateLimited { key, retry_after }) => {
                assert_eq!(key, "bot.vsBot.day");
                assert_eq!(retry_after, Some(Duration::from_secs(3600)));
            }
            other => panic!("expected an opponent rate-limit error, got {other:?}"),
        }
    }

    #[test]
    fn a_400_without_a_rate_limit_envelope_stays_a_plain_http_error() {
        let (port, _) = serve_repeating("400 Bad Request", "", r#"{"error":"No such user"}"#);
        match transport_for(port).post_form_once("/api/challenge/nobody", &[]) {
            Err(Error::Http(detail)) => assert!(
                detail.contains("No such user"),
                "the rejection reason must survive, got {detail}"
            ),
            other => panic!("expected an HTTP error, got {other:?}"),
        }
    }

    #[test]
    fn a_rate_limit_envelope_without_seconds_still_names_the_limit() {
        let parsed = parse_rate_limit(r#"{"error":"nope","ratelimit":{"key":"some.limit"}}"#)
            .expect("an envelope with no duration still parses");
        assert_eq!(parsed.key, "some.limit");
        assert_eq!(parsed.retry_after, None);
        assert!(parse_rate_limit(r#"{"error":"nope"}"#).is_none());
        assert!(parse_rate_limit("not json").is_none());
    }
}
