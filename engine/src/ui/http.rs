//! A deliberately narrow HTTP/1.1 subset for the loopback UI server.
//!
//! This implements only what the browser protocol needs: fixed routes, `Content-Length` bodies,
//! and streamed Server-Sent Events. Everything is bounded before it is buffered so a misbehaving
//! or hostile local client cannot force unbounded allocation, and anything outside the subset is
//! answered with a status rather than interpreted generously.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// How long a client has to deliver one complete request.
///
/// This bounds the whole request rather than a single read: the socket timeout is reset to the
/// time still remaining before every read, so a client that dribbles bytes cannot hold a
/// connection thread indefinitely by keeping each individual read inside the timeout.
pub const REQUEST_DEADLINE: Duration = Duration::from_secs(15);

/// The longest acceptable request line, covering the method, target, and version.
pub const MAX_REQUEST_LINE: usize = 8 * 1024;

/// The largest acceptable single header line.
pub const MAX_HEADER_LINE: usize = 8 * 1024;

/// The largest acceptable number of header lines.
pub const MAX_HEADERS: usize = 64;

/// The largest acceptable request body. Commands are a few dozen bytes; this is generous.
pub const MAX_BODY: usize = 8 * 1024;

/// A parsed request, limited to the fields the router consults.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    pub method: String,
    /// The request target with any query string removed.
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// Look up a header case-insensitively, as HTTP field names are case-insensitive.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Why a request could not be turned into a `Request`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestError {
    /// The connection ended before a request line arrived. Not an error worth answering.
    Closed,
    /// The request was syntactically invalid.
    Malformed,
    /// The request exceeded one of the size limits above.
    TooLarge,
    /// The request was not delivered in full within [`REQUEST_DEADLINE`].
    Timeout,
    /// A transport-level read failure.
    Io,
}

impl RequestError {
    /// The status a client should receive, or `None` when the connection simply closed.
    pub fn status(self) -> Option<Status> {
        match self {
            RequestError::Closed | RequestError::Io => None,
            RequestError::Malformed => Some(Status::BadRequest),
            RequestError::TooLarge => Some(Status::PayloadTooLarge),
            RequestError::Timeout => Some(Status::RequestTimeout),
        }
    }
}

/// Read and parse one request from `reader`, giving the client until `deadline` to deliver it.
pub fn read_request(
    reader: &mut BufReader<TcpStream>,
    deadline: Instant,
) -> Result<Request, RequestError> {
    let line = read_line(reader, MAX_REQUEST_LINE, deadline)?;
    if line.is_empty() {
        return Err(RequestError::Closed);
    }

    let mut parts = line.split(' ');
    let method = parts.next().ok_or(RequestError::Malformed)?;
    let target = parts.next().ok_or(RequestError::Malformed)?;
    let version = parts.next().ok_or(RequestError::Malformed)?;
    if parts.next().is_some() || !version.starts_with("HTTP/1.") {
        return Err(RequestError::Malformed);
    }
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(RequestError::Malformed);
    }
    // Only origin-form targets are served; there is no proxying and no absolute-form support.
    if !target.starts_with('/') {
        return Err(RequestError::Malformed);
    }
    let path = target.split(['?', '#']).next().unwrap_or("").to_owned();

    let mut headers = Vec::new();
    loop {
        let line = read_line(reader, MAX_HEADER_LINE, deadline)?;
        if line.is_empty() {
            break;
        }
        if headers.len() >= MAX_HEADERS {
            return Err(RequestError::TooLarge);
        }
        let (name, value) = line.split_once(':').ok_or(RequestError::Malformed)?;
        if name.is_empty() || name.ends_with(' ') {
            return Err(RequestError::Malformed);
        }
        headers.push((name.to_owned(), value.trim().to_owned()));
    }

    let request = Request {
        method: method.to_owned(),
        path,
        headers,
        body: Vec::new(),
    };

    // Chunked bodies are outside the subset. Rejecting them explicitly avoids the classic
    // request-smuggling ambiguity where a body is framed two different ways.
    if request
        .header("transfer-encoding")
        .is_some_and(|value| !value.is_empty())
    {
        return Err(RequestError::Malformed);
    }

    let length = match request.header("content-length") {
        None => 0,
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| RequestError::Malformed)?,
    };
    if length > MAX_BODY {
        return Err(RequestError::TooLarge);
    }

    let mut body = vec![0_u8; length];
    apply_deadline(reader, deadline)?;
    reader
        .read_exact(&mut body)
        .map_err(|error| read_failure(&error, RequestError::Malformed))?;

    Ok(Request { body, ..request })
}

/// Classify a failed read, distinguishing an expired deadline from other transport failures.
///
/// A socket read timeout surfaces as `WouldBlock` or `TimedOut` depending on the platform.
fn read_failure(error: &io::Error, fallback: RequestError) -> RequestError {
    match error.kind() {
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => RequestError::Timeout,
        _ => fallback,
    }
}

/// Bound the next read by the time still left before `deadline`.
///
/// Without this the socket timeout restarts on every read, so a client sending one byte just
/// inside the timeout could occupy a connection thread for hours.
fn apply_deadline(reader: &BufReader<TcpStream>, deadline: Instant) -> Result<(), RequestError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(RequestError::Timeout);
    }
    reader
        .get_ref()
        .set_read_timeout(Some(remaining))
        .map_err(|_| RequestError::Io)
}

/// Read one CRLF-terminated line, returning it without the terminator.
fn read_line(
    reader: &mut BufReader<TcpStream>,
    limit: usize,
    deadline: Instant,
) -> Result<String, RequestError> {
    let mut raw = Vec::new();
    apply_deadline(reader, deadline)?;
    // `take` bounds the read so an endless line without a newline cannot exhaust memory.
    let read = reader
        .take((limit + 1) as u64)
        .read_until(b'\n', &mut raw)
        .map_err(|error| read_failure(&error, RequestError::Io))?;
    if read == 0 {
        return Ok(String::new());
    }
    if !raw.ends_with(b"\n") {
        // Reading the whole allowance without a terminator means the line is over-long; stopping
        // short of it means the peer stopped sending mid-line, which is a malformed request.
        return Err(if read > limit {
            RequestError::TooLarge
        } else {
            RequestError::Malformed
        });
    }
    raw.pop();
    if raw.ends_with(b"\r") {
        raw.pop();
    }
    String::from_utf8(raw).map_err(|_| RequestError::Malformed)
}

/// The status codes this server emits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Ok,
    BadRequest,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    RequestTimeout,
    Conflict,
    PayloadTooLarge,
    UnsupportedMediaType,
    UnprocessableContent,
}

impl Status {
    pub fn code(self) -> u16 {
        match self {
            Status::Ok => 200,
            Status::BadRequest => 400,
            Status::Forbidden => 403,
            Status::NotFound => 404,
            Status::MethodNotAllowed => 405,
            Status::RequestTimeout => 408,
            Status::Conflict => 409,
            Status::PayloadTooLarge => 413,
            Status::UnsupportedMediaType => 415,
            Status::UnprocessableContent => 422,
        }
    }

    pub fn reason(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::BadRequest => "Bad Request",
            Status::Forbidden => "Forbidden",
            Status::NotFound => "Not Found",
            Status::MethodNotAllowed => "Method Not Allowed",
            Status::RequestTimeout => "Request Timeout",
            Status::Conflict => "Conflict",
            Status::PayloadTooLarge => "Payload Too Large",
            Status::UnsupportedMediaType => "Unsupported Media Type",
            Status::UnprocessableContent => "Unprocessable Content",
        }
    }
}

/// A restrictive policy for a UI that loads only its own embedded assets.
///
/// No remote origin is reachable, inline script and style are disallowed, and framing is denied,
/// so a scripted page cannot exfiltrate state or wrap the UI for clickjacking.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; \
script-src 'self'; \
style-src 'self'; \
img-src 'self' data:; \
connect-src 'self'; \
base-uri 'none'; \
form-action 'none'; \
frame-ancestors 'none'";

/// Headers applied to every response regardless of route.
fn write_common_headers(out: &mut Vec<u8>) {
    let _ = write!(
        out,
        "Content-Security-Policy: {CONTENT_SECURITY_POLICY}\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\n"
    );
}

/// Write a complete response with a buffered body and close the connection.
pub fn write_response(
    stream: &mut TcpStream,
    status: Status,
    content_type: &str,
    cache_control: &str,
    body: &[u8],
) -> io::Result<()> {
    let mut out = Vec::with_capacity(body.len() + 256);
    let _ = write!(
        out,
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: {}\r\n\
         Connection: close\r\n",
        status.code(),
        status.reason(),
        content_type,
        body.len(),
        cache_control,
    );
    write_common_headers(&mut out);
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(body);
    stream.write_all(&out)?;
    stream.flush()
}

/// Write a JSON response. Application state is never cached, so it cannot go stale in the browser.
pub fn write_json(stream: &mut TcpStream, status: Status, body: &str) -> io::Result<()> {
    write_response(
        stream,
        status,
        "application/json; charset=utf-8",
        "no-store",
        body.as_bytes(),
    )
}

/// Write a JSON error body carrying a stable machine-readable code.
pub fn write_error(stream: &mut TcpStream, status: Status, code: &str) -> io::Result<()> {
    let mut body = String::from("{\"error\":");
    super::json::write_string(&mut body, code);
    body.push('}');
    write_json(stream, status, &body)
}

/// Write the response head for a Server-Sent Events stream, leaving the body open.
pub fn write_event_stream_head(stream: &mut TcpStream) -> io::Result<()> {
    let mut out = Vec::with_capacity(256);
    let _ = write!(
        out,
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/event-stream; charset=utf-8\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         X-Accel-Buffering: no\r\n"
    );
    write_common_headers(&mut out);
    out.extend_from_slice(b"\r\n");
    stream.write_all(&out)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, TcpListener};

    /// Feed `raw` through a real loopback socket and parse it as the server would.
    fn parse(raw: &[u8]) -> Result<Request, RequestError> {
        parse_within(raw, Duration::from_secs(20))
    }

    /// Parse `raw` with an explicit deadline, so timeout handling is testable.
    fn parse_within(raw: &[u8], allowance: Duration) -> Result<Request, RequestError> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let raw = raw.to_vec();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).unwrap();
            let _ = stream.write_all(&raw);
            let _ = stream.flush();
            // Signal end-of-request so a short body is a read failure rather than a hang.
            let _ = stream.shutdown(std::net::Shutdown::Write);
            stream
        });
        let (server, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(server);
        let parsed = read_request(&mut reader, Instant::now() + allowance);
        let _ = client.join();
        parsed
    }

    #[test]
    fn parses_a_get_request_and_strips_the_query() {
        let request =
            parse(b"GET /api/state?since=3 HTTP/1.1\r\nHost: 127.0.0.1:9\r\n\r\n").unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/api/state");
        assert_eq!(request.header("host"), Some("127.0.0.1:9"));
        assert!(request.body.is_empty());
    }

    #[test]
    fn parses_a_post_body_and_matches_headers_case_insensitively() {
        let request = parse(
            b"POST /api/move HTTP/1.1\r\nHOST: 127.0.0.1:9\r\nContent-Length: 9\r\n\r\n{\"a\": 1}\n",
        )
        .unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.header("Host"), Some("127.0.0.1:9"));
        assert_eq!(request.body, b"{\"a\": 1}\n");
    }

    #[test]
    fn rejects_bodies_over_the_limit_without_buffering_them() {
        let raw = format!(
            "POST /api/move HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        assert_eq!(parse(raw.as_bytes()), Err(RequestError::TooLarge));
    }

    #[test]
    fn rejects_an_over_long_request_line() {
        let raw = format!("GET /{} HTTP/1.1\r\n\r\n", "a".repeat(MAX_REQUEST_LINE));
        assert_eq!(parse(raw.as_bytes()), Err(RequestError::TooLarge));
    }

    #[test]
    fn rejects_too_many_headers() {
        let mut raw = String::from("GET / HTTP/1.1\r\n");
        for index in 0..=MAX_HEADERS {
            raw.push_str(&format!("X-Pad-{index}: v\r\n"));
        }
        raw.push_str("\r\n");
        assert_eq!(parse(raw.as_bytes()), Err(RequestError::TooLarge));
    }

    #[test]
    fn rejects_malformed_requests() {
        for raw in [
            &b"GET\r\n\r\n"[..],
            b"GET / HTTP/1.1 extra\r\n\r\n",
            b"GET / FTP/1.1\r\n\r\n",
            b"GET http://evil/ HTTP/1.1\r\n\r\n",
            b"G3T / HTTP/1.1\r\n\r\n",
            b"GET / HTTP/1.1\r\nBadHeader\r\n\r\n",
            b"POST / HTTP/1.1\r\nContent-Length: abc\r\n\r\n",
        ] {
            assert_eq!(parse(raw), Err(RequestError::Malformed), "accepted {raw:?}");
        }
    }

    #[test]
    fn rejects_transfer_encoding_to_avoid_ambiguous_framing() {
        let raw =
            b"POST /api/move HTTP/1.1\r\nTransfer-Encoding: chunked\r\nContent-Length: 0\r\n\r\n";
        assert_eq!(parse(raw), Err(RequestError::Malformed));
    }

    #[test]
    fn reports_a_closed_connection_distinctly() {
        assert_eq!(parse(b""), Err(RequestError::Closed));
        assert_eq!(RequestError::Closed.status(), None);
        assert_eq!(
            RequestError::TooLarge.status(),
            Some(Status::PayloadTooLarge)
        );
    }

    #[test]
    fn a_client_that_never_finishes_its_request_hits_the_deadline() {
        // The peer opens a connection and sends nothing. Without a whole-request deadline this
        // would block for the full socket timeout on every read instead of ending once.
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let silent = std::thread::spawn(move || {
            let stream = TcpStream::connect(addr).unwrap();
            std::thread::sleep(Duration::from_millis(500));
            drop(stream);
        });

        let (server, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(server);
        let started = Instant::now();
        let outcome = read_request(&mut reader, Instant::now() + Duration::from_millis(150));

        assert_eq!(outcome, Err(RequestError::Timeout));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the read did not stop at the deadline"
        );
        assert_eq!(RequestError::Timeout.status(), Some(Status::RequestTimeout));
        let _ = silent.join();
    }

    #[test]
    fn a_line_truncated_by_the_peer_is_malformed_rather_than_too_large() {
        assert_eq!(
            parse(b"GET / HTTP/1.1\r\nHost: 127.0.0.1:9"),
            Err(RequestError::Malformed)
        );
    }

    #[test]
    fn rejects_a_body_shorter_than_its_declared_length() {
        let raw = b"POST /api/move HTTP/1.1\r\nContent-Length: 32\r\n\r\nshort";
        assert_eq!(parse(raw), Err(RequestError::Malformed));
    }
}
