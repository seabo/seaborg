//! HTTP transport abstraction.
//!
//! [`Transport`] is the seam between the bot logic and the network. The real
//! implementation, [`HttpTransport`], talks to Lichess over a single shared
//! `ureq::Agent` (one connection pool for the whole bot) and turns HTTP 429
//! responses into a bounded, shutdown-aware backoff. Tests substitute a fake
//! transport that replays recorded NDJSON and records the requests the bot makes,
//! so challenge and game handling run with no network access.

use std::io::BufRead;
use std::time::Duration;

use crate::backoff::Backoff;
use crate::error::{Error, Result};
use crate::shutdown::Shutdown;

/// Longest wait to open a connection (TCP plus TLS handshake) before giving up
/// and letting the caller's reconnect backoff take over.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Longest wait for response headers. This bounds the *header* phase only; it is
/// deliberately not a body timeout, because the game and event streams are
/// long-lived bodies that must be allowed to stay open indefinitely.
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
        // `http_status_as_error(false)` is what lets this crate inspect the
        // status itself, so a 429 can be told apart from other 4xx and mapped to
        // a retryable error instead of an opaque failure.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_response(Some(RESPONSE_TIMEOUT))
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
        self.with_rate_limit_retry(|| {
            let response = self
                .agent
                .post(url.as_str())
                .header("Authorization", &self.bearer)
                .send_form(form.iter().copied());
            read_response(response)
        })
    }

    fn open_stream(&self, path: &str) -> Result<Box<dyn Iterator<Item = Result<String>>>> {
        let url = self.url(path);
        self.with_rate_limit_retry(|| {
            let response = self
                .agent
                .get(url.as_str())
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
        let retry_after = match op() {
            Err(Error::RateLimited { retry_after }) => retry_after,
            other => return other,
        };
        if attempt >= max_attempts || shutdown.is_requested() {
            return Err(Error::RateLimited { retry_after });
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
        429 => Err(Error::RateLimited {
            retry_after: retry_after(&response),
        }),
        // Lichess explains a rejected request in the response body (typically
        // `{"error":"..."}`), which is the only thing that says *why* a 400
        // happened. Read it so the reason reaches the caller instead of a bare
        // status code.
        other => {
            let body = response.into_body().read_to_string().ok();
            Err(unexpected_status_error(other, body.as_deref()))
        }
    }
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

    use super::*;

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
                Err(Error::RateLimited { retry_after: None })
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
                Err(Error::RateLimited { retry_after: None })
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
                Err(Error::RateLimited { retry_after: None })
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
}
