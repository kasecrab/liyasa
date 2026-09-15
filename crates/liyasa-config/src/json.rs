//! Where each part of `liyasa.json` sits in its source text.
//!
//! `serde_json` gives a `Value` and nothing else, and `jsonschema` reports
//! failures by JSON Pointer, so a diagnostic can only point at a line if
//! something maps pointers back to byte offsets. [`SpanIndex`] is a structural
//! scan of the same bytes that does exactly that: it never interprets a value,
//! only finds where each one starts and ends.

use std::collections::BTreeMap;

use liyasa_core::span::{SourceId, Span};

/// The key and value spans of one JSON member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Located {
    /// The quoted key, for an object member. `None` at the root and inside an
    /// array.
    pub key: Option<Span>,
    pub value: Span,
}

/// Every JSON Pointer in a document, mapped to where it is written.
#[derive(Debug, Clone)]
pub struct SpanIndex {
    source: SourceId,
    located: BTreeMap<String, Located>,
}

impl SpanIndex {
    pub fn scan(source: SourceId, text: &str) -> Self {
        let mut index = Self {
            source,
            located: BTreeMap::new(),
        };
        let mut scanner = Scanner {
            bytes: text.as_bytes(),
            at: 0,
        };
        scanner.skip_space();
        index.value_at(&mut scanner, &mut String::new(), None);
        index
    }

    pub fn source(&self) -> SourceId {
        self.source
    }

    pub fn located(&self, pointer: &str) -> Option<Located> {
        self.located.get(pointer).copied()
    }

    pub fn value(&self, pointer: &str) -> Option<Span> {
        self.located.get(pointer).map(|l| l.value)
    }

    pub fn key(&self, pointer: &str) -> Option<Span> {
        self.located.get(pointer).and_then(|l| l.key)
    }

    /// The span of `pointer`, or of the closest ancestor that has one. A
    /// validator may report a path this scan never produced — a failure inside
    /// a `$ref`ed subschema, say — and a diagnostic without a span is worse
    /// than one that points at the enclosing object.
    pub fn nearest(&self, pointer: &str) -> Option<Span> {
        let mut rest = pointer;
        loop {
            if let Some(span) = self.value(rest) {
                return Some(span);
            }
            match rest.rfind('/') {
                Some(0) => rest = "",
                Some(at) => rest = &rest[..at],
                None => return self.value(""),
            }
        }
    }

    /// The key span of `pointer` when it has one, else its value span, else the
    /// nearest ancestor's. This is what a diagnostic should underline: an
    /// unknown or misspelled key reads better underlined than its value.
    pub fn nearest_key(&self, pointer: &str) -> Option<Span> {
        match self.located.get(pointer) {
            Some(located) => Some(located.key.unwrap_or(located.value)),
            None => self.nearest(pointer),
        }
    }

    /// Copies every entry at or under `pointer` from another file's index,
    /// after dropping what was there. Used when an overlay or a navigation
    /// file replaces a subtree: the grafted spans carry their own `SourceId`,
    /// so a diagnostic lands in the file that actually wrote the value.
    pub fn graft(&mut self, pointer: &str, from: &SpanIndex) {
        let under = format!("{pointer}/");
        self.located
            .retain(|at, _| at != pointer && !at.starts_with(&under));
        for (at, located) in &from.located {
            if at == pointer || at.starts_with(&under) {
                self.located.insert(at.clone(), *located);
            }
        }
    }

    /// Re-roots another file's index under `pointer` and grafts it there, for a
    /// file whose whole document becomes one subtree of the config.
    pub fn graft_root(&mut self, pointer: &str, from: &SpanIndex) {
        let under = format!("{pointer}/");
        self.located
            .retain(|at, _| at != pointer && !at.starts_with(&under));
        for (at, located) in &from.located {
            self.located.insert(format!("{pointer}{at}"), *located);
        }
    }

    fn record(&mut self, pointer: &str, key: Option<Span>, start: usize, end: usize) {
        self.located.insert(
            pointer.to_owned(),
            Located {
                key,
                value: Span::new(self.source, start as u32, end as u32),
            },
        );
    }

    /// Scans one value, recording it at `pointer`, and leaves the scanner just
    /// past it. `pointer` is restored to its entry value before returning.
    fn value_at(&mut self, scanner: &mut Scanner<'_>, pointer: &mut String, key: Option<Span>) {
        let start = scanner.at;
        match scanner.peek() {
            Some(b'{') => {
                scanner.at += 1;
                self.members(scanner, pointer);
            }
            Some(b'[') => {
                scanner.at += 1;
                self.elements(scanner, pointer);
            }
            Some(b'"') => {
                scanner.skip_string();
            }
            Some(_) => scanner.skip_atom(),
            None => {}
        }
        self.record(pointer, key, start, scanner.at);
    }

    fn members(&mut self, scanner: &mut Scanner<'_>, pointer: &mut String) {
        let len = pointer.len();
        loop {
            scanner.skip_space();
            match scanner.peek() {
                Some(b'}') => {
                    scanner.at += 1;
                    return;
                }
                Some(b',') => {
                    scanner.at += 1;
                    continue;
                }
                Some(b'"') => {}
                Some(_) => {
                    scanner.at += 1;
                    continue;
                }
                None => return,
            }
            let key_start = scanner.at;
            let name = scanner.skip_string();
            let key = Span::new(self.source, key_start as u32, scanner.at as u32);
            scanner.skip_space();
            if scanner.peek() != Some(b':') {
                continue;
            }
            scanner.at += 1;
            scanner.skip_space();
            pointer.push('/');
            pointer.push_str(&escape(&name));
            self.value_at(scanner, pointer, Some(key));
            pointer.truncate(len);
        }
    }

    fn elements(&mut self, scanner: &mut Scanner<'_>, pointer: &mut String) {
        let len = pointer.len();
        let mut at = 0usize;
        loop {
            scanner.skip_space();
            match scanner.peek() {
                Some(b']') => {
                    scanner.at += 1;
                    return;
                }
                Some(b',') => {
                    scanner.at += 1;
                    continue;
                }
                None => return,
                Some(_) => {}
            }
            pointer.push('/');
            pointer.push_str(&at.to_string());
            self.value_at(scanner, pointer, None);
            pointer.truncate(len);
            at += 1;
        }
    }
}

/// `~` and `/` are the two characters a pointer token cannot hold (RFC 6901).
fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

struct Scanner<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Scanner<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    /// Consumes a quoted string and returns its unescaped-enough content: only
    /// `~` and `/` matter to a pointer, and neither is ever written as an
    /// escape that this misreads.
    fn skip_string(&mut self) -> String {
        let mut out = String::new();
        self.at += 1; // the opening quote
        while let Some(byte) = self.peek() {
            self.at += 1;
            match byte {
                b'"' => break,
                b'\\' => {
                    let escaped = self.peek();
                    self.at += 1;
                    match escaped {
                        Some(b'u') => {
                            let hex = self.bytes.get(self.at..self.at + 4);
                            self.at = (self.at + 4).min(self.bytes.len());
                            if let Some(c) = hex
                                .and_then(|h| std::str::from_utf8(h).ok())
                                .and_then(|h| u32::from_str_radix(h, 16).ok())
                                .and_then(char::from_u32)
                            {
                                out.push(c);
                            }
                        }
                        Some(b'n') => out.push('\n'),
                        Some(b't') => out.push('\t'),
                        Some(b'r') => out.push('\r'),
                        Some(b'b') => out.push('\u{8}'),
                        Some(b'f') => out.push('\u{c}'),
                        Some(other) => out.push(other as char),
                        None => break,
                    }
                }
                _ => {
                    let from = self.at - 1;
                    while self.peek().is_some_and(|b| b & 0xC0 == 0x80) {
                        self.at += 1;
                    }
                    out.push_str(&String::from_utf8_lossy(&self.bytes[from..self.at]));
                }
            }
        }
        out
    }

    /// A number, `true`, `false`, or `null`: everything up to the next
    /// structural character.
    fn skip_atom(&mut self) {
        while let Some(byte) = self.peek() {
            if matches!(byte, b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r') {
                break;
            }
            self.at += 1;
        }
    }
}
