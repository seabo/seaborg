//! A minimal JSON reader and writer for the local UI protocol.
//!
//! The browser protocol exchanges small, fixed-shape documents, so the server owns this rather
//! than taking a serialization dependency. Only the subset JSON actually needs is implemented,
//! and parsing is deliberately strict: anything unexpected is a request error, not a best guess.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A parsed JSON document.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    /// JSON numbers are parsed as `f64`, matching the language's own number model.
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(value) => Some(value),
            _ => None,
        }
    }

    /// Read a field as an exact non-negative integer.
    ///
    /// Revisions are `u64` on the wire, so fractional or out-of-range values are rejected rather
    /// than truncated: a rounded revision would silently address the wrong game state.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Number(value) => {
                if value.is_finite() && *value >= 0.0 && value.fract() == 0.0 && *value <= MAX_EXACT
                {
                    Some(*value as u64)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// The largest integer an `f64` represents exactly.
const MAX_EXACT: f64 = 9_007_199_254_740_992.0;

/// Guards against deeply nested input exhausting the stack during recursive descent.
const MAX_DEPTH: usize = 16;

#[derive(Debug, PartialEq, Eq)]
pub struct ParseError;

/// Parse a complete JSON document, rejecting trailing content.
pub fn parse(input: &str) -> Result<Json, ParseError> {
    let mut parser = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    let value = parser.value(0)?;
    parser.skip_whitespace();
    if parser.pos != parser.bytes.len() {
        return Err(ParseError);
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ParseError)
        }
    }

    fn literal(&mut self, word: &str) -> Result<(), ParseError> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(())
        } else {
            Err(ParseError)
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, ParseError> {
        if depth > MAX_DEPTH {
            return Err(ParseError);
        }
        self.skip_whitespace();
        match self.peek().ok_or(ParseError)? {
            b'n' => self.literal("null").map(|()| Json::Null),
            b't' => self.literal("true").map(|()| Json::Bool(true)),
            b'f' => self.literal("false").map(|()| Json::Bool(false)),
            b'"' => self.string().map(Json::String),
            b'[' => self.array(depth),
            b'{' => self.object(depth),
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(ParseError),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, ParseError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.peek().ok_or(ParseError)? {
                b',' => self.pos += 1,
                b']' => {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(ParseError),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, ParseError> {
        self.expect(b'{')?;
        let mut fields = BTreeMap::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(fields));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            let value = self.value(depth + 1)?;
            fields.insert(key, value);
            self.skip_whitespace();
            match self.peek().ok_or(ParseError)? {
                b',' => self.pos += 1,
                b'}' => {
                    self.pos += 1;
                    return Ok(Json::Object(fields));
                }
                _ => return Err(ParseError),
            }
        }
    }

    fn number(&mut self) -> Result<Json, ParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(
            self.peek(),
            Some(b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
        ) {
            self.pos += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| ParseError)?
            .parse::<f64>()
            .map(Json::Number)
            .map_err(|_| ParseError)
    }

    fn string(&mut self) -> Result<String, ParseError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let byte = self.peek().ok_or(ParseError)?;
            self.pos += 1;
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let escape = self.peek().ok_or(ParseError)?;
                    self.pos += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode_escape()?),
                        _ => return Err(ParseError),
                    }
                }
                // Unescaped control characters are invalid JSON.
                0x00..=0x1f => return Err(ParseError),
                _ => {
                    // Multi-byte UTF-8 sequences are copied verbatim; the input is already `str`,
                    // so the remaining continuation bytes are known to be well formed.
                    let start = self.pos - 1;
                    while matches!(self.peek(), Some(byte) if byte & 0xc0 == 0x80) {
                        self.pos += 1;
                    }
                    out.push_str(
                        std::str::from_utf8(&self.bytes[start..self.pos])
                            .map_err(|_| ParseError)?,
                    );
                }
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, ParseError> {
        let high = self.hex4()?;
        // A leading surrogate is only meaningful when paired with its trailing half.
        if (0xd800..0xdc00).contains(&high) {
            self.expect(b'\\')?;
            self.expect(b'u')?;
            let low = self.hex4()?;
            if !(0xdc00..0xe000).contains(&low) {
                return Err(ParseError);
            }
            let combined = 0x1_0000 + ((high - 0xd800) << 10) + (low - 0xdc00);
            return char::from_u32(combined).ok_or(ParseError);
        }
        char::from_u32(high).ok_or(ParseError)
    }

    fn hex4(&mut self) -> Result<u32, ParseError> {
        let end = self.pos + 4;
        let digits = self.bytes.get(self.pos..end).ok_or(ParseError)?;
        let text = std::str::from_utf8(digits).map_err(|_| ParseError)?;
        let value = u32::from_str_radix(text, 16).map_err(|_| ParseError)?;
        self.pos = end;
        Ok(value)
    }
}

/// Append a JSON string literal, escaping everything the grammar requires.
pub fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Append a `"key":` prefix, inserting a separating comma for all but the first field.
pub fn write_key(out: &mut String, first: &mut bool, key: &str) {
    if *first {
        *first = false;
    } else {
        out.push(',');
    }
    write_string(out, key);
    out.push(':');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_command_documents_the_protocol_uses() {
        let value = parse(r#"{"uci":"e2e4","revision":7}"#).unwrap();
        assert_eq!(value.get("uci").unwrap().as_str(), Some("e2e4"));
        assert_eq!(value.get("revision").unwrap().as_u64(), Some(7));
        assert!(value.get("missing").is_none());
    }

    #[test]
    fn parses_nested_and_empty_containers() {
        assert_eq!(parse("{}").unwrap(), Json::Object(BTreeMap::new()));
        assert_eq!(parse("[]").unwrap(), Json::Array(Vec::new()));
        let value = parse(r#"{"a":[1,{"b":null},true,false]}"#).unwrap();
        let array = match value.get("a").unwrap() {
            Json::Array(items) => items,
            other => panic!("expected array, got {other:?}"),
        };
        assert_eq!(array.len(), 4);
        assert_eq!(array[0], Json::Number(1.0));
        assert_eq!(array[1].get("b"), Some(&Json::Null));
        assert_eq!(array[2], Json::Bool(true));
        assert_eq!(array[3], Json::Bool(false));
    }

    #[test]
    fn rejects_malformed_and_trailing_input() {
        for input in [
            "",
            "{",
            "{\"a\":}",
            "{\"a\":1,}",
            "[1,]",
            "{\"a\":1} trailing",
            "nul",
            "\"unterminated",
            "{'a':1}",
            "01x",
        ] {
            assert_eq!(parse(input), Err(ParseError), "accepted {input:?}");
        }
    }

    #[test]
    fn rejects_revisions_that_are_not_exact_non_negative_integers() {
        for input in ["1.5", "-1", "1e400", "\"3\""] {
            let value = parse(input).unwrap_or(Json::Null);
            assert_eq!(value.as_u64(), None, "accepted {input:?}");
        }
        assert_eq!(parse("0").unwrap().as_u64(), Some(0));
        assert_eq!(parse("1e3").unwrap().as_u64(), Some(1000));
    }

    #[test]
    fn rejects_deeply_nested_documents() {
        let deep = format!("{}1{}", "[".repeat(64), "]".repeat(64));
        assert_eq!(parse(&deep), Err(ParseError));
    }

    #[test]
    fn round_trips_strings_needing_escapes() {
        let value = parse(r#""a\"b\\c\ndAé😀""#).unwrap();
        let text = value.as_str().unwrap();
        assert_eq!(text, "a\"b\\c\ndAé😀");

        let mut out = String::new();
        write_string(&mut out, text);
        assert_eq!(parse(&out).unwrap().as_str(), Some(text));
    }

    #[test]
    fn escapes_control_characters_when_writing() {
        let mut out = String::new();
        write_string(&mut out, "tab\there\u{1}");
        assert_eq!(out, r#""tab\there\u0001""#);
        assert_eq!(parse(&out).unwrap().as_str(), Some("tab\there\u{1}"));
    }

    #[test]
    fn rejects_unescaped_control_characters_and_lone_surrogates() {
        assert_eq!(parse("\"line\nbreak\""), Err(ParseError));
        assert_eq!(parse(r#""\ud83d""#), Err(ParseError));
        assert_eq!(parse(r#""\udc00\udc00""#), Err(ParseError));
    }

    #[test]
    fn writes_comma_separated_object_keys() {
        let mut out = String::from("{");
        let mut first = true;
        write_key(&mut out, &mut first, "a");
        out.push('1');
        write_key(&mut out, &mut first, "b");
        out.push('2');
        out.push('}');
        assert_eq!(out, r#"{"a":1,"b":2}"#);
    }
}
