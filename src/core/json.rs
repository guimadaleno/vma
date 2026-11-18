//! JSON, as the MCP server reads and writes it: a value, read from one line
//! and written back as one line. A number keeps the text it came as, so a
//! request's id goes back exactly as it was sent. Nothing here knows MCP.

use std::fmt::Write;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    /// The number as written.
    Number(String),
    String(String),
    Array(Vec<Json>),
    /// Members in the order they came or were made.
    Object(Vec<(String, Json)>),
}

/// How deep arrays and objects may nest in what is read.
const DEPTH: usize = 64;

impl Json {
    /// An object of `members`.
    pub fn object(members: Vec<(&str, Json)>) -> Json {
        Json::Object(
            members
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    pub fn text(s: &str) -> Json {
        Json::String(s.to_string())
    }

    pub fn number(n: impl std::fmt::Display) -> Json {
        Json::Number(n.to_string())
    }

    /// A member of an object.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    /// A whole number, as written with no fraction or exponent.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Number(n) => n.parse().ok(),
            _ => None,
        }
    }

    /// Reads one JSON value, with nothing but white space around it.
    pub fn parse(text: &str) -> Result<Json, String> {
        let mut reader = Reader {
            bytes: text.as_bytes(),
            at: 0,
        };
        let value = reader.value(0)?;
        reader.space();
        if reader.at != reader.bytes.len() {
            return Err(format!("more after the value, at byte {}", reader.at));
        }
        Ok(value)
    }

    /// The value on one line.
    pub fn write(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }

    fn write_to(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Number(n) => out.push_str(n),
            Json::String(s) => quote(s, out),
            Json::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_to(out);
                }
                out.push(']');
            }
            Json::Object(members) => {
                out.push('{');
                for (i, (key, value)) in members.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    quote(key, out);
                    out.push(':');
                    value.write_to(out);
                }
                out.push('}');
            }
        }
    }
}

fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
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

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn space(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.bytes.get(self.at) {
            self.at += 1;
        }
    }

    fn fail<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what}, at byte {}", self.at))
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > DEPTH {
            return self.fail("nested too deep");
        }
        self.space();
        match self.bytes.get(self.at) {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::String),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => {
                for (word, value) in [
                    ("true", Json::Bool(true)),
                    ("false", Json::Bool(false)),
                    ("null", Json::Null),
                ] {
                    if self.bytes[self.at..].starts_with(word.as_bytes()) {
                        self.at += word.len();
                        return Ok(value);
                    }
                }
                self.fail("no value")
            }
            None => self.fail("the text ends before a value"),
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.at += 1;
        let mut members = Vec::new();
        self.space();
        if self.bytes.get(self.at) == Some(&b'}') {
            self.at += 1;
            return Ok(Json::Object(members));
        }
        loop {
            self.space();
            if self.bytes.get(self.at) != Some(&b'"') {
                return self.fail("no member's name");
            }
            let key = self.string()?;
            self.space();
            if self.bytes.get(self.at) != Some(&b':') {
                return self.fail("no : after a member's name");
            }
            self.at += 1;
            members.push((key, self.value(depth + 1)?));
            self.space();
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Json::Object(members));
                }
                _ => return self.fail("no , or } after a member"),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.at += 1;
        let mut items = Vec::new();
        self.space();
        if self.bytes.get(self.at) == Some(&b']') {
            self.at += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.space();
            match self.bytes.get(self.at) {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Json::Array(items));
                }
                _ => return self.fail("no , or ] after an item"),
            }
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.at;
        let digits = |r: &mut Self| {
            let from = r.at;
            while let Some(b'0'..=b'9') = r.bytes.get(r.at) {
                r.at += 1;
            }
            r.at > from
        };
        if self.bytes.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        if self.bytes.get(self.at) == Some(&b'0') {
            self.at += 1;
        } else if !digits(self) {
            return self.fail("a number with no digits");
        }
        if self.bytes.get(self.at) == Some(&b'.') {
            self.at += 1;
            if !digits(self) {
                return self.fail("no digits after a number's point");
            }
        }
        if let Some(b'e' | b'E') = self.bytes.get(self.at) {
            self.at += 1;
            if let Some(b'+' | b'-') = self.bytes.get(self.at) {
                self.at += 1;
            }
            if !digits(self) {
                return self.fail("no digits in a number's exponent");
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).expect("ASCII digits");
        Ok(Json::Number(text.to_string()))
    }

    fn string(&mut self) -> Result<String, String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let start = self.at;
            while let Some(&b) = self.bytes.get(self.at) {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.at += 1;
            }
            // The text came as a &str, and a run stops only at ASCII, so the
            // run is whole UTF-8.
            out.push_str(std::str::from_utf8(&self.bytes[start..self.at]).expect("UTF-8"));
            match self.bytes.get(self.at) {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escaped = match self.bytes.get(self.at) {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'/') => '/',
                        Some(b'b') => '\u{8}',
                        Some(b'f') => '\u{c}',
                        Some(b'n') => '\n',
                        Some(b'r') => '\r',
                        Some(b't') => '\t',
                        Some(b'u') => {
                            self.at += 1;
                            let c = self.unicode()?;
                            out.push(c);
                            continue;
                        }
                        _ => return self.fail("an unknown escape"),
                    };
                    self.at += 1;
                    out.push(escaped);
                }
                Some(_) => return self.fail("a control character in a string"),
                None => return self.fail("a string never closed"),
            }
        }
    }

    /// The character of a `\u` escape whose four digits start here, and of
    /// the low surrogate's escape after it where it is a high one.
    fn unicode(&mut self) -> Result<char, String> {
        let high = self.hex4()?;
        if !(0xD800..0xDC00).contains(&high) {
            return char::from_u32(high).map_or_else(|| self.fail("a lone surrogate"), Ok);
        }
        if !self.bytes[self.at..].starts_with(b"\\u") {
            return self.fail("a high surrogate with no low one");
        }
        self.at += 2;
        let low = self.hex4()?;
        if !(0xDC00..0xE000).contains(&low) {
            return self.fail("a high surrogate with no low one");
        }
        let c = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
        char::from_u32(c).map_or_else(|| self.fail("a bad surrogate pair"), Ok)
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self.bytes.get(self.at..self.at + 4);
        let value = digits
            .and_then(|d| std::str::from_utf8(d).ok())
            .filter(|d| d.bytes().all(|b| b.is_ascii_hexdigit()))
            .and_then(|d| u32::from_str_radix(d, 16).ok());
        match value {
            Some(v) => {
                self.at += 4;
                Ok(v)
            }
            None => self.fail("a \\u escape without four hex digits"),
        }
    }
}

/// `bytes` in base64, with padding, as MCP carries a picture.
pub fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_read_and_write_back() {
        let line = r#"{"jsonrpc":"2.0","id":7,"params":{"a":[1,-2.5e3,true,false,null],"b":"x"}}"#;
        let value = Json::parse(line).unwrap();
        assert_eq!(value.write(), line);
        assert_eq!(value.get("id"), Some(&Json::Number("7".into())));
        assert_eq!(value.get("id").and_then(Json::as_i64), Some(7));
        assert_eq!(
            value
                .get("params")
                .and_then(|p| p.get("b"))
                .and_then(Json::as_str),
            Some("x")
        );
        assert_eq!(Json::parse(" [ ] ").unwrap(), Json::Array(vec![]));
        assert_eq!(Json::parse("{ }").unwrap(), Json::Object(vec![]));
    }

    #[test]
    fn strings_keep_every_character() {
        let value = Json::parse(r#""a\"b\\c\/d\n\t\u00e9\ud83d\ude00é""#).unwrap();
        assert_eq!(value, Json::text("a\"b\\c/d\n\té😀é"));
        assert_eq!(Json::text("q\"\\\n\u{1}é").write(), r#""q\"\\\n\u0001é""#);
        let round = Json::text("tab\there \"quoted\" \\ back");
        assert_eq!(Json::parse(&round.write()).unwrap(), round);
    }

    #[test]
    fn wrong_json_is_named() {
        for bad in [
            "",
            "{",
            "[1,]",
            "{\"a\" 1}",
            "\"open",
            "tru",
            "01x",
            "1.",
            "-",
            "\"\\x\"",
            "\"\\ud800\"",
            "{\"a\":1} x",
            "\"a\u{1}b\"",
        ] {
            assert!(Json::parse(bad).is_err(), "{bad:?}");
        }
        let deep = "[".repeat(100) + &"]".repeat(100);
        assert!(Json::parse(&deep).unwrap_err().contains("nested too deep"));
    }

    #[test]
    fn base64_pads_as_written() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }
}
