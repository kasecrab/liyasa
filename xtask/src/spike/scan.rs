//! The directive scanner and marker rewrite, as PRD §7.5.1 specifies them.
//!
//! This is the spike's implementation of candidate (a). It is deliberately the
//! whole mechanism — segmentation, the colon-count stack, the container-depth
//! check, out-of-band props, and the `RewriteMap` — because the question the
//! spike answers is whether that mechanism holds up, not whether comrak parses
//! Markdown.

use liyasa_core::diagnostics::code;
use liyasa_core::document::{PropValue, Props};
use liyasa_core::markdown::{ComponentKind, DirectiveInfo, DirectiveTable, RewriteMap};
use liyasa_core::{Diagnostic, Diagnostics, SourceId, Span};

/// The literal prefix a forged marker would have to reproduce.
pub const MARKER_PREFIX: &str = "<!--ly:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerRole {
    Open,
    Close,
    Leaf,
}

impl MarkerRole {
    const fn letter(self) -> char {
        match self {
            Self::Open => 'o',
            Self::Close => 'c',
            Self::Leaf => 'l',
        }
    }
}

pub struct Rewritten {
    pub text: String,
    pub map: RewriteMap,
    pub table: DirectiveTable,
    /// Per marker ID: its role and, for a container, the ID of its partner.
    pub markers: Vec<Marker>,
    pub diagnostics: Diagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {
    pub role: MarkerRole,
    pub partner: Option<usize>,
}

/// The indentation and blockquote prefix of a line, which is what the scanner
/// compares to decide whether an open and a close sit at the same CommonMark
/// container depth (§7.5.1 item 7).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Prefix {
    indent: usize,
    quotes: usize,
}

struct Open {
    id: usize,
    colons: usize,
    prefix: Prefix,
    span: Span,
}

pub fn rewrite(source: &str, id: SourceId, nonce: [u8; 16]) -> Rewritten {
    let nonce_hex = nonce.iter().fold(String::with_capacity(32), |mut out, b| {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
        out
    });

    let mut out = String::with_capacity(source.len());
    let mut map = Vec::new();
    let mut table: Vec<DirectiveInfo> = Vec::new();
    let mut markers: Vec<Marker> = Vec::new();
    let mut diagnostics = Diagnostics::new();
    let mut stack: Vec<Open> = Vec::new();

    let mut fence: Option<(char, usize)> = None;
    let mut offset = 0usize;
    let mut delta = 0i32; // cumulative: expanded = rewritten - delta

    let emit = |out: &mut String,
                map: &mut Vec<(u32, i32)>,
                delta: &mut i32,
                raw: &str,
                rewritten: &str| {
        *delta += rewritten.len() as i32 - raw.len() as i32;
        map.push((out.len() as u32, *delta));
        out.push_str(rewritten);
    };

    for raw in source.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\r', '\n']);
        let eol = &raw[line.len()..];
        let start = offset;
        offset += raw.len();

        let prefix = prefix_of(line);
        let content = strip_prefix(line, &prefix);
        let content_at = start + (line.len() - content.len());

        if let Some((ch, len)) = fence {
            if is_fence_close(content, ch, len) {
                fence = None;
            }
            out.push_str(raw);
            continue;
        }
        if let Some((ch, len)) = fence_open(content) {
            fence = Some((ch, len));
            out.push_str(raw);
            continue;
        }

        // §7.5.1 item 2: a literal marker prefix is escaped before the rewrite,
        // so nothing in content or in an expanded value can forge a marker.
        if let Some(at) = content.find(MARKER_PREFIX) {
            let escaped = content.replacen(MARKER_PREFIX, "<!--&#108;y:", 1);
            let at = (content_at + at) as u32;
            diagnostics.push(
                Diagnostic::new(code::W0319, "literal directive marker prefix; escaped")
                    .at(Span::new(id, at, at + MARKER_PREFIX.len() as u32)),
            );
            let rewritten = format!("{}{escaped}{eol}", &line[..line.len() - content.len()]);
            emit(&mut out, &mut map, &mut delta, raw, &rewritten);
            continue;
        }

        let Some(directive) = classify(content) else {
            out.push_str(raw);
            continue;
        };

        let span = Span::new(id, start as u32, (start + line.len()) as u32);
        let marker_id = table.len();
        let role = match directive {
            Directive::Open {
                colons,
                name,
                props,
            } => {
                table.push(DirectiveInfo {
                    name,
                    prop_spans: props.spans(id, content_at),
                    props: props.value,
                    kind: ComponentKind::Container,
                    span,
                });
                markers.push(Marker {
                    role: MarkerRole::Open,
                    partner: None,
                });
                stack.push(Open {
                    id: marker_id,
                    colons,
                    prefix,
                    span,
                });
                MarkerRole::Open
            }
            Directive::Close { colons } => {
                let matching = stack
                    .iter()
                    .rposition(|open| open.colons == colons && open.prefix == prefix);
                let Some(at) = matching else {
                    let message = if stack.iter().any(|open| open.colons == colons) {
                        "directive close is at a different container depth than its open"
                    } else {
                        "directive close without a matching open"
                    };
                    diagnostics.push(Diagnostic::new(code::E0311, message).at(span));
                    out.push_str(raw);
                    continue;
                };
                for unclosed in stack.drain(at + 1..) {
                    diagnostics.push(
                        Diagnostic::new(code::E0310, "container directive not closed")
                            .at(unclosed.span)
                            .label(span, "a shallower directive closed here"),
                    );
                    markers[unclosed.id].partner = None;
                }
                let Some(open) = stack.pop() else {
                    unreachable!("rposition found it")
                };
                table.push(DirectiveInfo {
                    name: String::new(),
                    props: Props::default(),
                    kind: ComponentKind::Container,
                    span,
                    prop_spans: Vec::new(),
                });
                markers.push(Marker {
                    role: MarkerRole::Close,
                    partner: Some(open.id),
                });
                markers[open.id].partner = Some(marker_id);
                MarkerRole::Close
            }
            Directive::Leaf { name, props } => {
                table.push(DirectiveInfo {
                    name,
                    prop_spans: props.spans(id, content_at),
                    props: props.value,
                    kind: ComponentKind::Leaf,
                    span,
                });
                markers.push(Marker {
                    role: MarkerRole::Leaf,
                    partner: None,
                });
                MarkerRole::Leaf
            }
        };
        let lead = &line[..line.len() - content.len()];
        let rewritten = format!(
            "{lead}{MARKER_PREFIX}{nonce_hex}:{}:{marker_id}-->{eol}",
            role.letter()
        );
        emit(&mut out, &mut map, &mut delta, raw, &rewritten);
    }

    for unclosed in stack {
        diagnostics
            .push(Diagnostic::new(code::E0310, "container directive not closed").at(unclosed.span));
    }

    Rewritten {
        text: out,
        map: RewriteMap(map),
        table: DirectiveTable(table),
        markers,
        diagnostics,
    }
}

enum Directive {
    Open {
        colons: usize,
        name: String,
        props: ParsedProps,
    },
    Close {
        colons: usize,
    },
    Leaf {
        name: String,
        props: ParsedProps,
    },
}

#[derive(Default)]
pub struct ParsedProps {
    pub value: Props,
    /// Byte offsets relative to the start of the content, per prop name.
    offsets: Vec<(String, usize, usize)>,
}

impl ParsedProps {
    fn spans(&self, source: SourceId, base: usize) -> Vec<(String, Span)> {
        self.offsets
            .iter()
            .map(|(name, start, end)| {
                (
                    name.clone(),
                    Span::new(source, (base + start) as u32, (base + end) as u32),
                )
            })
            .collect()
    }
}

fn classify(content: &str) -> Option<Directive> {
    let colons = content.chars().take_while(|c| *c == ':').count();
    if colons < 2 {
        return None;
    }
    let rest = &content[colons..];
    if colons >= 3 && rest.trim().is_empty() {
        return Some(Directive::Close { colons });
    }
    let name_len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(rest.len());
    if name_len == 0 {
        return None;
    }
    let name = rest[..name_len].to_owned();
    let tail = rest[name_len..].trim_end();
    let props = parse_props(tail, colons + name_len)?;
    if colons >= 3 {
        Some(Directive::Open {
            colons,
            name,
            props,
        })
    } else {
        Some(Directive::Leaf { name, props })
    }
}

/// The props of an info string, without their spans. Shared with the engine
/// that lets comrak own segmentation and only parses the info string.
pub fn props_of(tail: &str) -> Option<Props> {
    parse_props(tail, 0).map(|p| p.value)
}

/// `{key="value" key=1 key=true .class #id}`; anything else is not a directive.
fn parse_props(tail: &str, base: usize) -> Option<ParsedProps> {
    let tail = tail.trim_start();
    if tail.is_empty() {
        return Some(ParsedProps::default());
    }
    let body = tail.strip_prefix('{')?.strip_suffix('}')?;
    let body_at = base + (tail.len() - body.len() - 1) + 1;
    let mut out = ParsedProps::default();
    let mut rest = body;
    let mut at = 0usize;
    while !rest.trim().is_empty() {
        let skipped = rest.len() - rest.trim_start().len();
        at += skipped;
        rest = rest.trim_start();
        let (name, value, used) = if let Some(after) = rest.strip_prefix('.') {
            let len = token_len(after);
            (
                "class".to_owned(),
                PropValue::Str(after[..len].to_owned()),
                len + 1,
            )
        } else if let Some(after) = rest.strip_prefix('#') {
            let len = token_len(after);
            (
                "id".to_owned(),
                PropValue::Str(after[..len].to_owned()),
                len + 1,
            )
        } else {
            let key_len = rest.find('=')?;
            let key = rest[..key_len].trim().to_owned();
            let after = &rest[key_len + 1..];
            let (value, len) = parse_value(after)?;
            (key, value, key_len + 1 + len)
        };
        out.offsets
            .push((name.clone(), body_at + at, body_at + at + used));
        out.value.0.insert(name, value);
        at += used;
        rest = &rest[used..];
    }
    Some(out)
}

fn parse_value(text: &str) -> Option<(PropValue, usize)> {
    if let Some(after) = text.strip_prefix('"') {
        let end = after.find('"')?;
        return Some((PropValue::Str(after[..end].to_owned()), end + 2));
    }
    if let Some(after) = text.strip_prefix("{{") {
        let end = after.find("}}")?;
        return Some((PropValue::Expr(after[..end].trim().to_owned()), end + 4));
    }
    if let Some(after) = text.strip_prefix('[') {
        let end = after.find(']')?;
        let items = after[..end]
            .split(',')
            .map(|item| PropValue::Str(item.trim().trim_matches('"').to_owned()))
            .collect();
        return Some((PropValue::List(items), end + 2));
    }
    let len = text
        .find(|c: char| c.is_whitespace() || c == '}')
        .unwrap_or(text.len());
    let token = &text[..len];
    let value = match token {
        "true" => PropValue::Bool(true),
        "false" => PropValue::Bool(false),
        _ => match token.parse::<f64>() {
            Ok(n) => PropValue::Num(n),
            Err(_) => PropValue::Str(token.to_owned()),
        },
    };
    Some((value, len))
}

fn token_len(text: &str) -> usize {
    text.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(text.len())
}

fn prefix_of(line: &str) -> Prefix {
    let mut indent = 0;
    let mut quotes = 0;
    for ch in line.chars() {
        match ch {
            ' ' => indent += 1,
            '\t' => indent += 4,
            '>' => {
                quotes += 1;
                indent = 0;
            }
            _ => break,
        }
    }
    Prefix { indent, quotes }
}

fn strip_prefix<'a>(line: &'a str, prefix: &Prefix) -> &'a str {
    let mut rest = line;
    for _ in 0..prefix.quotes {
        rest = rest.trim_start_matches([' ', '\t']);
        rest = rest.strip_prefix('>').unwrap_or(rest);
    }
    rest.trim_start_matches([' ', '\t'])
}

fn fence_open(content: &str) -> Option<(char, usize)> {
    for ch in ['`', '~'] {
        let len = content.chars().take_while(|c| *c == ch).count();
        if len >= 3 {
            return Some((ch, len));
        }
    }
    None
}

fn is_fence_close(content: &str, ch: char, len: usize) -> bool {
    let run = content.chars().take_while(|c| *c == ch).count();
    run >= len && content[run..].trim().is_empty()
}
