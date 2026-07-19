//! HTTP transport abstraction.
//!
//! [`Transport`] is the seam between the bot logic and the network. The real
//! implementation, [`HttpTransport`], talks to Lichess over `ureq`. Tests
//! substitute a fake transport that replays recorded NDJSON and records the
//! requests the bot makes, so challenge handling runs with no network access.

use std::io::BufRead;

use crate::error::{Error, Result};

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

/// A [`Transport`] backed by `ureq`, authenticated with a bearer token.
pub struct HttpTransport {
    base_url: String,
    bearer: String,
}

impl HttpTransport {
    /// Build a transport for `base_url` authenticating every request with
    /// `token`.
    pub fn new(base_url: impl Into<String>, token: impl AsRef<str>) -> HttpTransport {
        HttpTransport {
            base_url: base_url.into(),
            bearer: format!("Bearer {}", token.as_ref()),
        }
    }

    /// Join the API origin with a relative request path.
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

impl Transport for HttpTransport {
    fn get(&self, path: &str) -> Result<String> {
        let mut response = ureq::get(self.url(path))
            .header("Authorization", &self.bearer)
            .call()
            .map_err(map_ureq_error)?;
        read_body(&mut response)
    }

    fn post_empty(&self, path: &str) -> Result<String> {
        let mut response = ureq::post(self.url(path))
            .header("Authorization", &self.bearer)
            .send_empty()
            .map_err(map_ureq_error)?;
        read_body(&mut response)
    }

    fn post_form(&self, path: &str, form: &[(&str, &str)]) -> Result<String> {
        let mut response = ureq::post(self.url(path))
            .header("Authorization", &self.bearer)
            .send_form(form.iter().copied())
            .map_err(map_ureq_error)?;
        read_body(&mut response)
    }

    fn open_stream(&self, path: &str) -> Result<Box<dyn Iterator<Item = Result<String>>>> {
        let response = ureq::get(self.url(path))
            .header("Authorization", &self.bearer)
            .call()
            .map_err(map_ureq_error)?;
        let reader = std::io::BufReader::new(response.into_body().into_reader());
        let lines = reader
            .lines()
            .map(|line| line.map_err(|e| Error::Http(e.to_string())));
        Ok(Box::new(lines))
    }
}

/// Read a full response body into a string.
fn read_body(response: &mut ureq::http::Response<ureq::Body>) -> Result<String> {
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| Error::Http(e.to_string()))
}

/// Translate a `ureq` error into this crate's error type, singling out the
/// unauthorized status because it has a distinct, actionable cause.
fn map_ureq_error(error: ureq::Error) -> Error {
    match error {
        ureq::Error::StatusCode(401) => Error::Unauthorized,
        other => Error::Http(other.to_string()),
    }
}
