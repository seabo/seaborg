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
//! strand the caller on a request certain to fail.
//!
//! Two things keep that from happening. Challenge creation, where a day-long
//! refusal is the expected case, uses the non-retrying entry point and acts on
//! the refusal itself. Every other call still retries, but only through waits
//! short enough to be worth sitting out; a longer one the server states is
//! surfaced rather than slept, so no limit can hold a thread for hours whichever
//! endpoint it arrives on.
//!
//! Either way the response body is parsed for the rate-limit detail Lichess
//! supplies — which limit fired, and how long it lasts.
//!
//! Every request is traced. A rate limit is a statement about request volume, so
//! diagnosing one needs the traffic that provoked it and not just the refusal
//! that ended it: each request carries an id that ties its start to its outcome,
//! and streams announce when they open and close because a stream that keeps
//! reconnecting is itself a source of requests. The tracing sits at `debug`, off
//! under the bot's default `Info` level and enabled with
//! `RUST_LOG=lichess::transport=debug`.
//!
//! One case is loud by default: a 429 whose body carries no rate-limit envelope.
//! Lichess describes the per-day caps a bot expects to meet in that envelope, so
//! a 429 without one came from a limit this crate cannot name, and the raw body
//! is the only evidence of which. It is rare by construction and worth a warning.
//!
//! Tests substitute a fake transport that replays recorded NDJSON and records the
//! requests the bot makes, so challenge and game handling run with no network
//! access.

use std::io::BufRead;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

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
/// Ceiling for the 429 backoff, and the longest server-stated wait this
/// transport will sit out on the caller's behalf. Beyond it, waiting inside the
/// request stops being a transient hiccup and becomes a stalled thread.
const RATE_LIMIT_MAX: Duration = Duration::from_secs(600);
/// How many times a single request is retried through 429s before giving up and
/// surfacing the rate-limit error to the caller.
const RATE_LIMIT_MAX_ATTEMPTS: u32 = 5;

/// Source of request ids. Process-wide rather than per-transport so ids stay
/// unique across every request in a log, which matters because the bot runs one
/// transport per process but reads it from several threads at once.
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// One request's identity and start time, tying its trace lines together.
///
/// A retried request gets a fresh trace per attempt: each attempt is a distinct
/// round trip with its own status and duration, and collapsing them under one id
/// would hide exactly the repetition a rate-limit investigation is looking for.
struct RequestTrace {
    id: u64,
    method: &'static str,
    /// The request path, owned because the trace outlives the borrow the caller
    /// built its URL from.
    path: String,
    started: Instant,
}

impl RequestTrace {
    /// Record the start of `method path` and log it.
    fn begin(method: &'static str, path: &str) -> RequestTrace {
        let trace = RequestTrace {
            id: NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed),
            method,
            path: path.to_string(),
            started: Instant::now(),
        };
        log::debug!("req#{} -> {} {}", trace.id, trace.method, trace.path);
        trace
    }

    /// Log the outcome of the request: its status and how long it took.
    fn finish(&self, status: u16) {
        log::debug!(
            "req#{} <- {} {} {} in {}ms",
            self.id,
            self.method,
            self.path,
            status,
            self.started.elapsed().as_millis()
        );
    }

    /// Log a request that produced no HTTP status at all — a connection, TLS, or
    /// timeout failure. Without this a failed request would show only an opening
    /// line, making a stalled connection indistinguishable from a lost log.
    fn fail(&self, error: &str) {
        log::debug!(
            "req#{} <- {} {} failed after {}ms: {}",
            self.id,
            self.method,
            self.path,
            self.started.elapsed().as_millis(),
            error
        );
    }
}

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
            RATE_LIMIT_MAX,
            RATE_LIMIT_MAX_ATTEMPTS,
            op,
        )
    }

    /// Send one form POST, shared by the retrying and non-retrying entry points so
    /// they can differ in retry policy alone.
    fn send_form(&self, path: &str, form: &[(&str, &str)]) -> Result<String> {
        let trace = RequestTrace::begin("POST", path);
        let response = self
            .agent
            .post(self.url(path))
            .header("Authorization", &self.bearer)
            .send_form(form.iter().copied());
        read_response(response, &trace)
    }
}

impl Transport for HttpTransport {
    fn get(&self, path: &str) -> Result<String> {
        let url = self.url(path);
        self.with_rate_limit_retry(|| {
            let trace = RequestTrace::begin("GET", path);
            let response = self
                .agent
                .get(url.as_str())
                .header("Authorization", &self.bearer)
                .call();
            read_response(response, &trace)
        })
    }

    fn post_empty(&self, path: &str) -> Result<String> {
        let url = self.url(path);
        self.with_rate_limit_retry(|| {
            let trace = RequestTrace::begin("POST", path);
            let response = self
                .agent
                .post(url.as_str())
                .header("Authorization", &self.bearer)
                .send_empty();
            read_response(response, &trace)
        })
    }

    fn post_form(&self, path: &str, form: &[(&str, &str)]) -> Result<String> {
        self.with_rate_limit_retry(|| self.send_form(path, form))
    }

    fn post_form_once(&self, path: &str, form: &[(&str, &str)]) -> Result<String> {
        self.send_form(path, form)
    }

    fn open_stream(&self, path: &str) -> Result<Box<dyn Iterator<Item = Result<String>>>> {
        let url = self.url(path);
        self.with_rate_limit_retry(|| {
            let trace = RequestTrace::begin("GET", path);
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
            let response = check_status(response, &trace)?;
            let reader = std::io::BufReader::new(response.into_body().into_reader());
            let lines = reader
                .lines()
                .map(|line| line.map_err(|e| Error::Http(e.to_string())));
            // A stream's cost is the time it stays open, not the request that
            // opened it: a stream that reconnects every few seconds issues far
            // more requests than its one opening line suggests. Pairing the open
            // with a close makes that visible as a lifetime.
            Ok(Box::new(TracedStream::new(trace, lines))
                as Box<dyn Iterator<Item = Result<String>>>)
        })
    }
}

/// A stream's line iterator, logging how long the stream stayed open once it
/// ends.
///
/// The close is reported from `Drop` rather than on exhaustion because a stream
/// is at least as often abandoned as it is drained — the reader stops on
/// shutdown, or on an error mid-body — and a close line that only appeared for
/// cleanly finished streams would systematically miss the failures worth seeing.
struct TracedStream<I> {
    trace: RequestTrace,
    lines: I,
    /// Lines yielded so far, to distinguish a stream that carried traffic before
    /// dropping from one that produced nothing at all.
    yielded: u64,
}

impl<I> TracedStream<I> {
    fn new(trace: RequestTrace, lines: I) -> TracedStream<I> {
        log::debug!(
            "req#{} stream open {} {}",
            trace.id,
            trace.method,
            trace.path
        );
        TracedStream {
            trace,
            lines,
            yielded: 0,
        }
    }
}

impl<I: Iterator<Item = Result<String>>> Iterator for TracedStream<I> {
    type Item = Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.lines.next();
        if item.is_some() {
            self.yielded += 1;
        }
        item
    }
}

impl<I> Drop for TracedStream<I> {
    fn drop(&mut self) {
        log::debug!(
            "req#{} stream closed {} {} after {}ms, {} lines",
            self.trace.id,
            self.trace.method,
            self.trace.path,
            self.trace.started.elapsed().as_millis(),
            self.yielded
        );
    }
}

/// Retry `op` through HTTP 429s with `backoff`, waiting via `sleep`.
///
/// Any error other than [`Error::RateLimited`] propagates at once. After each
/// 429 the wait is the one the server stated — from the response body, or from
/// `Retry-After` when the body said nothing — else the next backoff step.
///
/// A stated wait longer than `max_wait` ends the loop instead of being slept.
/// Retrying inside the request only makes sense for a limit that clears while
/// the caller waits, and the limits Lichess reports in seconds-to-clear form can
/// stand for hours: sleeping one out would block the calling thread far past the
/// point of usefulness, on a retry the server has already told us will be
/// refused. The caller gets the full stated duration and can act on it.
///
/// The loop otherwise stops once `max_attempts` requests have been made or
/// shutdown is requested, returning the last rate-limit error either way so the
/// caller can decide what to do.
fn with_rate_limit_retry<T>(
    shutdown: &Shutdown,
    mut sleep: impl FnMut(Duration),
    mut backoff: Backoff,
    max_wait: Duration,
    max_attempts: u32,
    mut op: impl FnMut() -> Result<T>,
) -> Result<T> {
    let mut attempt = 1u32;
    loop {
        let (key, retry_after) = match op() {
            Err(Error::RateLimited { key, retry_after }) => (key, retry_after),
            other => return other,
        };
        if attempt >= max_attempts
            || shutdown.is_requested()
            || retry_after.is_some_and(|wait| wait > max_wait)
        {
            return Err(Error::RateLimited { key, retry_after });
        }
        sleep(retry_after.unwrap_or_else(|| backoff.next_delay()));
        attempt += 1;
    }
}

/// Map a completed request to a body string or a typed error by its status.
fn read_response(
    result: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    trace: &RequestTrace,
) -> Result<String> {
    let mut response = check_status(result, trace)?;
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
    trace: &RequestTrace,
) -> Result<ureq::http::Response<ureq::Body>> {
    let response = match result {
        Ok(response) => response,
        Err(error) => {
            let error = error.to_string();
            trace.fail(&error);
            return Err(Error::Http(error));
        }
    };
    let status = response.status().as_u16();
    trace.finish(status);
    match status {
        200..=299 => Ok(response),
        // 404 is a distinct, expected outcome on the challenge-accept path (the
        // challenge was canceled or expired before the accept landed), and 401
        // means the token was refused; both get their own variant the caller can
        // single out instead of an opaque body. The body is still read and
        // traced: a restriction scoped to the token can arrive as a 401, and its
        // body is then the only statement of which restriction that was.
        401 | 404 => {
            let body = response.into_body().read_to_string().ok();
            log_body(trace, body.as_deref());
            if status == 401 {
                Err(Error::Unauthorized)
            } else {
                Err(Error::NotFound)
            }
        }
        429 => {
            // Which limit fired is the whole question on a 429, and this crate
            // reads only two of the several places Lichess might answer it. Dump
            // the rest so a limit that names itself in an unexpected header is
            // visible rather than silently dropped.
            log_headers(trace, &response);
            // The header is the fallback, not the primary source: for the limits a
            // bot actually meets, Lichess states the wait in the body and sends no
            // `Retry-After` at all. Read the header before consuming the response,
            // then let the body override it.
            let header_wait = retry_after(&response);
            let body = response.into_body().read_to_string().ok();
            let limit = body.as_deref().and_then(parse_rate_limit);
            if limit.is_none() {
                // Lichess describes the per-day caps a bot expects to meet in a
                // `ratelimit` envelope. A 429 without one came from some other
                // limit, and since nothing else here names it, the raw body is
                // the only remaining evidence of which — and of whether the limit
                // is scoped to this endpoint or to the token as a whole.
                log::warn!(
                    "req#{} {} {} was rate limited with no rate-limit envelope; body: {}",
                    trace.id,
                    trace.method,
                    trace.path,
                    body_snippet(body.as_deref())
                );
            }
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
                    log::debug!(
                        "req#{} {} {} refused for the addressed account's limit [{}]",
                        trace.id,
                        trace.method,
                        trace.path,
                        limit.key
                    );
                    return Err(Error::OpponentRateLimited {
                        key: limit.key,
                        retry_after: limit.retry_after,
                    });
                }
            }
            // Unlike the statuses above, these have no variant of their own, so
            // the body is folded into the error as well as traced.
            log_body(trace, body.as_deref());
            Err(unexpected_status_error(other, body.as_deref()))
        }
    }
}

/// Log every response header, for a status whose cause the bot could not name
/// from the fields it knows to read.
fn log_headers(trace: &RequestTrace, response: &ureq::http::Response<ureq::Body>) {
    // Skip the formatting entirely when the level is off: this walks every header
    // on a response that, during a rate-limit episode, arrives repeatedly.
    if !log::log_enabled!(log::Level::Debug) {
        return;
    }
    for (name, value) in response.headers() {
        log::debug!(
            "req#{} header {}: {}",
            trace.id,
            name,
            value.to_str().unwrap_or("<non-ascii>")
        );
    }
}

/// Trace the body of a failed response under its request's id.
///
/// Every non-2xx body is logged, including those whose status already has a
/// typed error and so never carries the text to the caller. A discarded body is
/// what made a live rate-limit lockout undiagnosable: whichever limit or
/// restriction fired, the server names it in the body and nowhere else.
fn log_body(trace: &RequestTrace, body: Option<&str>) {
    log::debug!(
        "req#{} {} {} body: {}",
        trace.id,
        trace.method,
        trace.path,
        body_snippet(body)
    );
}

/// Render a response body for a log line, capped and with missing or unreadable
/// bodies stated explicitly rather than logged as an empty string — "the server
/// sent nothing" and "the body could not be read" are different findings.
fn body_snippet(body: Option<&str>) -> String {
    match body.map(str::trim) {
        None => "<unreadable>".to_string(),
        Some("") => "<empty>".to_string(),
        Some(body) => body.chars().take(MAX_ERROR_BODY_CHARS).collect(),
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
    use std::sync::{Arc, Mutex, Once};
    use std::thread;
    use std::time::Instant;

    use super::*;

    /// One log record the capture logger kept, flattened to the parts the tracing
    /// tests assert on.
    #[derive(Clone)]
    struct Captured {
        level: log::Level,
        target: String,
        message: String,
    }

    /// Every record emitted since the process started. A single global buffer is
    /// forced by `log`, which permits one logger per process; tests therefore
    /// share it and select their own records rather than getting a private one.
    static CAPTURED: Mutex<Vec<Captured>> = Mutex::new(Vec::new());
    static INSTALL: Once = Once::new();

    struct CaptureLogger;

    impl log::Log for CaptureLogger {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }

        fn log(&self, record: &log::Record) {
            CAPTURED.lock().unwrap().push(Captured {
                level: record.level(),
                target: record.target().to_string(),
                message: record.args().to_string(),
            });
        }

        fn flush(&self) {}
    }

    /// Install the capture logger, once per test process.
    ///
    /// The max level is raised to `Trace` so nothing is filtered before it
    /// reaches the buffer; the tests assert on each record's own level instead,
    /// which is what actually decides visibility under an operator's `RUST_LOG`.
    fn install_capture_logger() {
        // Registered by reference rather than boxed: `log`'s boxed installer is
        // behind its `std` feature, which this crate does not enable.
        static LOGGER: CaptureLogger = CaptureLogger;
        INSTALL.call_once(|| {
            log::set_logger(&LOGGER).expect("no logger installed yet");
            log::set_max_level(log::LevelFilter::Trace);
        });
    }

    /// This module's captured records whose message mentions `needle`.
    ///
    /// Selecting by substring rather than draining the buffer is what makes these
    /// tests safe under the parallel test harness: every tracing test uses a
    /// request path unique to it, so concurrent tests cannot claim each other's
    /// records however their execution interleaves.
    ///
    /// Records from other targets are excluded because `ureq` logs the full URL
    /// too, so a path-only filter would return this crate's tracing mixed with
    /// the HTTP client's and let assertions about what the transport emits pass
    /// or fail on a dependency's logging.
    fn records_mentioning(needle: &str) -> Vec<Captured> {
        CAPTURED
            .lock()
            .unwrap()
            .iter()
            .filter(|record| record.target == TRANSPORT_TARGET && record.message.contains(needle))
            .cloned()
            .collect()
    }

    /// The log target this module's records carry, which is also the selector an
    /// operator writes in `RUST_LOG` to turn request tracing on.
    const TRANSPORT_TARGET: &str = "lichess::transport";

    /// Assert that some captured record mentioning `needle` also contains
    /// `fragment`, and return it.
    fn expect_record(needle: &str, fragment: &str) -> Captured {
        let records = records_mentioning(needle);
        records
            .iter()
            .find(|record| record.message.contains(fragment))
            .unwrap_or_else(|| {
                let seen: Vec<&str> = records.iter().map(|r| r.message.as_str()).collect();
                panic!("no record containing {fragment:?}; captured for {needle:?}: {seen:#?}")
            })
            .clone()
    }

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
            Duration::from_secs(30),
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
    fn a_wait_longer_than_the_bound_is_surfaced_rather_than_slept() {
        // Some Lichess limits are stated in hours, not seconds: the per-day caps
        // on challenges and on games against other bots both report the time
        // remaining. Sleeping one of those inside the request would park the
        // calling thread for the rest of the day on a retry the server has
        // already refused. The limit is real, so the error is the right outcome —
        // it just belongs to the caller, who can pause the one activity affected
        // and keep serving everything else.
        let waits = RefCell::new(Vec::new());
        let calls = RefCell::new(0u32);
        let result = with_rate_limit_retry::<()>(
            &Shutdown::new(),
            |wait| waits.borrow_mut().push(wait),
            Backoff::new(Duration::from_secs(1), Duration::from_secs(30)),
            Duration::from_secs(30),
            5,
            || {
                *calls.borrow_mut() += 1;
                Err(Error::RateLimited {
                    key: Some("bot.vsBot.day".to_string()),
                    retry_after: Some(Duration::from_secs(7200)),
                })
            },
        );
        match result {
            // The full duration reaches the caller: a clamped one would have it
            // resume far too early, straight into the same refusal.
            Err(Error::RateLimited { key, retry_after }) => {
                assert_eq!(key.as_deref(), Some("bot.vsBot.day"));
                assert_eq!(retry_after, Some(Duration::from_secs(7200)));
            }
            other => panic!("expected a rate-limit error, got {other:?}"),
        }
        assert_eq!(
            calls.into_inner(),
            1,
            "the refused request must not be re-sent"
        );
        assert!(
            waits.into_inner().is_empty(),
            "the calling thread must not sleep out a limit this long"
        );
    }

    #[test]
    fn a_wait_at_the_bound_is_still_slept() {
        // The boundary belongs to the retrying side: a limit that clears within
        // the bound is exactly the transient case in-transport retrying exists
        // for, so it must still be waited out transparently.
        let waits = RefCell::new(Vec::new());
        let calls = RefCell::new(0u32);
        let result = with_rate_limit_retry(
            &Shutdown::new(),
            |wait| waits.borrow_mut().push(wait),
            Backoff::new(Duration::from_secs(1), Duration::from_secs(30)),
            Duration::from_secs(30),
            5,
            || {
                let mut calls = calls.borrow_mut();
                *calls += 1;
                if *calls == 1 {
                    Err(Error::RateLimited {
                        key: None,
                        retry_after: Some(Duration::from_secs(30)),
                    })
                } else {
                    Ok("ok".to_string())
                }
            },
        );
        assert_eq!(result.unwrap(), "ok");
        assert_eq!(waits.into_inner(), vec![Duration::from_secs(30)]);
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
            Duration::from_secs(30),
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
            Duration::from_secs(30),
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
            Duration::from_secs(30),
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
            Duration::from_secs(30),
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
    fn a_retrying_post_also_refuses_to_sleep_out_a_day_long_limit() {
        // The endpoints that still retry in-transport — accepting and declining
        // challenges, moves, cancels — share the response parsing with the
        // challenge path, so they see these hours-long durations too. Accepting a
        // bot's challenge starts a game, which is what the per-day bot-versus-bot
        // cap is charged against, so this is a response the accept endpoint can
        // genuinely return. Retrying it would block the thread that runs the
        // event loop for the rest of the day.
        let (port, served) = serve_repeating("429 Too Many Requests", "", BOT_VS_BOT_BODY);
        let started = Instant::now();
        let result = transport_for(port).post_empty("/api/challenge/abcd1234/accept");
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
        // Belt and braces: the assertion above already rules out a retry, but a
        // wall-clock bound states the property the test actually protects, which
        // is that the caller is not held while the limit runs.
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the call must return promptly rather than waiting out the limit"
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

    #[test]
    fn a_request_and_its_response_share_one_traced_id() {
        // A rate limit is a claim about request volume, so the trace has to
        // support counting and attributing requests, not merely noting that some
        // happened. That needs each response tied to the request it answers.
        install_capture_logger();
        const PATH: &str = "/traced-round-trip";
        let (port, _) = serve_repeating("200 OK", "", "{}");
        transport_for(port).get(PATH).expect("the request succeeds");

        let sent = expect_record(PATH, "->");
        let received = expect_record(PATH, "<-");
        assert!(
            sent.message.contains("GET") && received.message.contains("GET"),
            "the method appears on both lines: {} / {}",
            sent.message,
            received.message
        );
        assert!(
            received.message.contains("200"),
            "the response line carries the status: {}",
            received.message
        );
        let id = sent
            .message
            .split_whitespace()
            .next()
            .expect("the request line opens with its id");
        assert!(
            received.message.starts_with(id),
            "response {} must carry the request's id {id}",
            received.message
        );
    }

    #[test]
    fn request_tracing_stays_below_the_operator_default_level() {
        // The bot runs at `Info` so its lifecycle is legible without `RUST_LOG`.
        // Per-request tracing at that level would bury it: a single game is
        // hundreds of requests. It belongs behind
        // `RUST_LOG=lichess::transport=debug` instead.
        install_capture_logger();
        const PATH: &str = "/traced-below-info";
        let (port, _) = serve_repeating("200 OK", "", "{}");
        transport_for(port).get(PATH).expect("the request succeeds");

        let records = records_mentioning(PATH);
        assert!(!records.is_empty(), "the request was traced at all");
        for record in records {
            assert_eq!(
                record.level,
                log::Level::Debug,
                "a successful request must add nothing at Info: {}",
                record.message
            );
            assert_eq!(
                record.target, TRANSPORT_TARGET,
                "the module target is what RUST_LOG selects on"
            );
        }
    }

    #[test]
    fn an_unexplained_429_preserves_its_body_at_warn() {
        // The body of a 429 that names no limit is the only evidence of which
        // limit fired. It used to be read and dropped, which is why a live
        // lockout could not be diagnosed at all; it must now survive, and at a
        // level the operator sees without having opted into tracing.
        install_capture_logger();
        const PATH: &str = "/unexplained-429";
        const BODY: &str = "Too many requests. Please wait a minute.";
        let (port, _) = serve_repeating("429 Too Many Requests", "", BODY);
        let result = transport_for(port).post_form_once(PATH, &[]);
        assert!(matches!(result, Err(Error::RateLimited { key: None, .. })));

        let record = expect_record(PATH, BODY);
        assert_eq!(
            record.level,
            log::Level::Warn,
            "an unexplained 429 must not need RUST_LOG to be seen: {}",
            record.message
        );
    }

    #[test]
    fn a_401_traces_its_body_even_though_the_error_drops_it() {
        // `Error::Unauthorized` carries no text, so the body is the only record
        // of what the server actually objected to — and a restriction scoped to
        // the token can arrive this way rather than as a 429. Dropping it would
        // leave the same blind spot this change exists to close.
        install_capture_logger();
        const PATH: &str = "/traced-401";
        const BODY: &str = r#"{"error":"No such token"}"#;
        let (port, _) = serve_repeating("401 Unauthorized", "", BODY);
        let result = transport_for(port).post_form_once(PATH, &[]);
        assert!(matches!(result, Err(Error::Unauthorized)));

        let record = expect_record(PATH, BODY);
        assert_eq!(
            record.level,
            log::Level::Debug,
            "a routine 401 belongs with the request tracing, not above it: {}",
            record.message
        );
    }

    #[test]
    fn a_429_that_names_its_limit_does_not_warn_about_the_body() {
        // When the envelope is present the limit is already identified, and the
        // caller logs it. Warning again would make the routine per-day caps look
        // like the anomaly the bare-body warning is reserved for.
        install_capture_logger();
        const PATH: &str = "/explained-429";
        let (port, _) = serve_repeating("429 Too Many Requests", "", BOT_VS_BOT_BODY);
        let result = transport_for(port).post_form_once(PATH, &[]);
        assert!(matches!(
            result,
            Err(Error::RateLimited { key: Some(_), .. })
        ));

        let shouted: Vec<String> = records_mentioning(PATH)
            .into_iter()
            .filter(|record| matches!(record.level, log::Level::Warn | log::Level::Error))
            .map(|record| record.message)
            .collect();
        assert!(
            shouted.is_empty(),
            "an identified limit is not an anomaly, but got {shouted:#?}"
        );
    }

    #[test]
    fn a_429_logs_every_response_header() {
        // This crate reads only `Retry-After` and the body envelope. A limit that
        // announces itself anywhere else — a scope, a window, a remaining count —
        // would otherwise be invisible precisely when it is the answer.
        install_capture_logger();
        const PATH: &str = "/header-dump-429";
        let (port, _) = serve_repeating(
            "429 Too Many Requests",
            "X-RateLimit-Scope: token\r\n",
            "nope",
        );
        let _ = transport_for(port).post_form_once(PATH, &[]);

        // Headers are logged under the request id rather than the path, so find
        // the id from the request line and select the headers that follow it.
        let sent = expect_record(PATH, "->");
        let id = sent
            .message
            .split_whitespace()
            .next()
            .expect("the request line opens with its id")
            .to_string();
        let header = expect_record(&id, "x-ratelimit-scope");
        assert!(
            header.message.contains("token"),
            "the header value is logged, not just its name: {}",
            header.message
        );
        assert_eq!(header.level, log::Level::Debug);
    }

    #[test]
    fn a_stream_traces_its_open_and_its_close() {
        // A stream costs requests over its lifetime, not at its open: one that
        // reconnects continually can dominate a token's request budget while
        // showing up as a single innocuous line. Pairing open with close, and
        // reporting the lines carried, makes that churn measurable.
        install_capture_logger();
        const PATH: &str = "/traced-stream";
        let port = serve_once(|mut stream| {
            read_request_headers(&mut stream);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nfirst\nsecond\n")
                .unwrap();
        });
        let transport =
            HttpTransport::new(format!("http://127.0.0.1:{port}"), "token", Shutdown::new());
        let lines: Vec<String> = transport
            .open_stream(PATH)
            .expect("stream opens")
            .collect::<Result<Vec<_>>>()
            .expect("both lines arrive");
        assert_eq!(lines, vec!["first".to_string(), "second".to_string()]);

        assert_eq!(expect_record(PATH, "stream open").level, log::Level::Debug);
        let closed = expect_record(PATH, "stream closed");
        assert!(
            closed.message.contains("2 lines"),
            "the close reports what the stream carried: {}",
            closed.message
        );
    }

    #[test]
    fn a_request_that_never_gets_a_status_is_still_traced() {
        // A connection or TLS failure produces no status, so the status-keyed
        // response line never fires. Without its own line the request would show
        // an open and no outcome, which reads as a lost log rather than a failure.
        install_capture_logger();
        const PATH: &str = "/unreachable";
        // Bind and immediately release a port, so connecting to it is refused
        // rather than merely slow.
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            listener.local_addr().expect("local addr").port()
        };
        let result = transport_for(port).get(PATH);
        assert!(matches!(result, Err(Error::Http(_))));

        let failed = expect_record(PATH, "failed after");
        assert_eq!(failed.level, log::Level::Debug);
    }
}
