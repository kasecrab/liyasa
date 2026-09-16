//! YAML with the limits of CM-24.
//!
//! `liyasa-core::yaml` is the only place the YAML crate itself is called; this
//! module is the guard in front of it. The order matters: the caps on size and
//! on anchors are checked on the *text*, before parsing, because an alias bomb
//! expands during the parse and a cap on the result would arrive too late. The
//! depth and node caps are then checked on the parsed value, which by that
//! point cannot have been amplified.
//!
//! Anchors and aliases are rejected rather than expanded. Nothing in
//! documentation front matter needs them, and every YAML denial-of-service
//! starts with one.

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::span::Span;
use liyasa_core::yaml::YamlError;

/// 1 MB, counted in bytes of the document text.
pub const MAX_BYTES: usize = 1 << 20;
pub const MAX_DEPTH: usize = 32;
pub const MAX_NODES: usize = 10_000;

/// Parses a YAML document under the CM-24 limits.
pub fn parse_value(text: &str, at: Option<Span>) -> Result<serde_json::Value, YamlError> {
    guard(text, at)?;
    let value = liyasa_core::yaml::parse_value(text, at)?;
    check_shape(&value, at)?;
    Ok(value)
}

/// Parses a document and its typed view, as `liyasa-core` does, but under the
/// limits.
pub fn parse_typed<T: serde::de::DeserializeOwned>(
    text: &str,
    at: Option<Span>,
) -> Result<(serde_json::Value, T), YamlError> {
    let value = parse_value(text, at)?;
    let typed = serde_json::from_value(value.clone()).map_err(|e| reject(e.to_string(), at))?;
    Ok((value, typed))
}

/// The text-level caps: document size and the anchor and alias grammar.
pub fn guard(text: &str, at: Option<Span>) -> Result<(), YamlError> {
    if text.len() > MAX_BYTES {
        return Err(reject(
            format!(
                "YAML document is {} bytes; the cap is {MAX_BYTES} ({} MB)",
                text.len(),
                MAX_BYTES >> 20
            ),
            at,
        ));
    }
    if let Some(offset) = find_anchor_or_alias(text) {
        let span = at.map(|span| {
            let start = span.start + offset as u32;
            Span::new(span.source, start, (start + 1).min(span.end))
        });
        return Err(reject(
            "YAML anchors and aliases are disabled; write the value out instead".to_owned(),
            span,
        ));
    }
    Ok(())
}

fn check_shape(value: &serde_json::Value, at: Option<Span>) -> Result<(), YamlError> {
    let mut nodes = 0usize;
    walk(value, 1, &mut nodes).map_err(|message| reject(message, at))
}

fn walk(value: &serde_json::Value, depth: usize, nodes: &mut usize) -> Result<(), String> {
    *nodes += 1;
    if *nodes > MAX_NODES {
        return Err(format!("YAML document has more than {MAX_NODES} nodes"));
    }
    if depth > MAX_DEPTH {
        return Err(format!(
            "YAML document nests deeper than {MAX_DEPTH} levels"
        ));
    }
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                walk(item, depth + 1, nodes)?;
            }
        }
        serde_json::Value::Object(fields) => {
            for field in fields.values() {
                walk(field, depth + 1, nodes)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn reject(message: String, at: Option<Span>) -> YamlError {
    let mut diagnostic = Diagnostic::new(code::E0102, message);
    diagnostic.span = at;
    Box::new(diagnostic)
}

/// The byte offset of the first anchor (`&name`) or alias (`*name`) token, if
/// the document has one.
///
/// Both are only anchors where YAML expects a node, so the scan skips
/// comments, quoted scalars, and block scalars, and then requires the `&` or
/// `*` to sit where a node may begin and to be followed by a name. A plain
/// scalar may not start with either character, so a valid document never
/// trips this by accident.
fn find_anchor_or_alias(text: &str) -> Option<usize> {
    let mut block: Option<usize> = None;
    let mut offset = 0usize;

    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\r', '\n']);
        let start = offset;
        offset += raw.len();

        let indent = line.len() - line.trim_start().len();
        if let Some(parent) = block {
            if line.trim().is_empty() || indent > parent {
                continue;
            }
            block = None;
        }

        if let Some(at) = scan_line(line, start, &mut block, indent) {
            return Some(at);
        }
    }
    None
}

fn scan_line(line: &str, start: usize, block: &mut Option<usize>, indent: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut at = 0usize;
    // The last non-space byte seen, which is what decides whether the next
    // `&` or `*` stands where a node may begin.
    let mut previous = b'\n';
    let mut after_space = true;

    while at < bytes.len() {
        match bytes[at] {
            b'#' if after_space => return None,
            b'\'' => {
                at += 1;
                while at < bytes.len() {
                    if bytes[at] == b'\'' {
                        if bytes.get(at + 1) == Some(&b'\'') {
                            at += 2;
                            continue;
                        }
                        break;
                    }
                    at += 1;
                }
                previous = b'\'';
            }
            b'"' => {
                at += 1;
                while at < bytes.len() {
                    match bytes[at] {
                        b'\\' => at += 2,
                        b'"' => break,
                        _ => at += 1,
                    }
                }
                previous = b'"';
            }
            b'|' | b'>' if node_position(previous) && header_of(&line[at..]) => {
                *block = Some(indent);
                return None;
            }
            b'&' | b'*' if node_position(previous) && is_name(bytes.get(at + 1)) => {
                return Some(start + at);
            }
            byte => {
                if !byte.is_ascii_whitespace() {
                    previous = byte;
                }
            }
        }
        // An unterminated quoted scalar leaves `at` at the end of the line,
        // and a line ending in a backslash inside one leaves it one past the
        // end, because the escape arm steps over two bytes. Both are valid
        // YAML — an apostrophe in a plain scalar, a quoted scalar continued on
        // the next line — so neither may index.
        after_space = bytes.get(at).is_some_and(u8::is_ascii_whitespace) || at == 0;
        at += 1;
    }
    None
}

/// The characters after which YAML expects a node to start.
fn node_position(previous: u8) -> bool {
    matches!(previous, b'\n' | b'-' | b':' | b',' | b'[' | b'{' | b'?')
}

fn is_name(byte: Option<&u8>) -> bool {
    byte.is_some_and(|b| !b.is_ascii_whitespace() && !matches!(b, b',' | b']' | b'}'))
}

/// `|`, `>`, and their chomping and indentation modifiers, optionally followed
/// by a comment: the tail of a line that opens a block scalar.
fn header_of(tail: &str) -> bool {
    let rest =
        tail[1..].trim_start_matches(['+', '-', '1', '2', '3', '4', '5', '6', '7', '8', '9']);
    let rest = rest.trim_start();
    rest.is_empty() || rest.starts_with('#')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(text: &str) -> String {
        match parse_value(text, None) {
            Ok(value) => panic!("expected a rejection, got {value}"),
            Err(diagnostic) => {
                assert_eq!(diagnostic.code, code::E0102);
                diagnostic.message.clone()
            }
        }
    }

    #[test]
    fn plain_documents_parse() {
        let value = parse_value("title: Install\ntags: [a, b]\n", None).expect("valid YAML");
        assert_eq!(value["title"], "Install");
        assert_eq!(value["tags"][1], "b");
    }

    /// The anchor scan runs over every front-matter block of every build, so
    /// an unterminated quoted scalar in it is a panic on ordinary prose.
    #[test]
    fn an_unterminated_quote_does_not_index_past_the_line() {
        let value = parse_value(
            "description: Every key a page's YAML front matter accepts\n",
            None,
        )
        .expect("valid YAML");
        assert_eq!(
            value["description"],
            "Every key a page's YAML front matter accepts"
        );
    }

    #[test]
    fn a_backslash_at_the_end_of_a_quoted_line_does_not_index_past_it() {
        // The escape arm steps over two bytes, so the line ends with `at` one
        // past the end rather than at it. The trailing backslash is YAML's
        // line continuation, which also swallows the fold's space.
        let value =
            parse_value("note: \"he said \\\"hi\\\"\\\n  and left\"\n", None).expect("valid YAML");
        assert_eq!(value["note"], "he said \"hi\"and left");
    }

    #[test]
    fn a_lone_apostrophe_is_still_scanned_to_the_end_of_the_line() {
        let value =
            parse_value("title: what a page's front matter accepts'\n", None).expect("valid YAML");
        assert_eq!(value["title"], "what a page's front matter accepts'");
    }

    /// Every byte of a plain scalar, so no arm of the scan can step off the
    /// end of one.
    #[test]
    fn no_single_line_document_panics_the_anchor_scan() {
        const ALPHABET: &[char] = &[
            '\'', '"', '\\', '#', '&', '*', '|', '>', ':', '-', ',', '[', ']', '{', '}', '?', ' ',
            'a', '1', '\t', 'é',
        ];
        let mut state = 0x853c_49e6_748f_ea9bu64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..4_000 {
            let length = (next() % 16) as usize + 1;
            let body: String = (0..length)
                .map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize])
                .collect();
            let _ = parse_value(&format!("key: {body}\n"), None);
        }
    }

    #[test]
    fn an_alias_bomb_is_rejected_before_it_expands() {
        let bomb = "a: &a [x, x]\nb: &b [*a, *a]\nc: &c [*b, *b]\nd: [*c, *c]\n";
        assert!(err(bomb).contains("anchors and aliases are disabled"));
    }

    #[test]
    fn an_anchor_alone_is_rejected() {
        assert!(err("base: &anchor\n  x: 1\n").contains("anchors and aliases"));
    }

    #[test]
    fn an_alias_in_a_flow_sequence_is_rejected() {
        assert!(err("a: &a 1\nb: [*a]\n").contains("anchors and aliases"));
    }

    #[test]
    fn a_star_inside_a_quoted_scalar_is_content() {
        let value = parse_value("title: \"a * b\"\nglob: '*.md'\n", None).expect("valid YAML");
        assert_eq!(value["glob"], "*.md");
    }

    #[test]
    fn a_star_inside_a_plain_scalar_is_content() {
        let value = parse_value("title: a * b\nnote: 5 & 6\n", None).expect("valid YAML");
        assert_eq!(value["title"], "a * b");
    }

    #[test]
    fn a_star_in_a_comment_is_content() {
        let value = parse_value("title: A  # see *note and &c\n", None).expect("valid YAML");
        assert_eq!(value["title"], "A");
    }

    #[test]
    fn a_star_inside_a_block_scalar_is_content() {
        let text = "body: |\n  *emphasis* and &amp;\n  more\ntitle: A\n";
        let value = parse_value(text, None).expect("valid YAML");
        assert_eq!(value["title"], "A");
        assert!(value["body"].as_str().is_some_and(|b| b.contains('*')));
    }

    #[test]
    fn a_block_scalar_does_not_hide_a_later_alias() {
        let text = "body: |\n  text\nother: &a 1\n";
        assert!(err(text).contains("anchors and aliases"));
    }

    #[test]
    fn an_oversized_document_is_rejected() {
        let text = format!("title: {}\n", "x".repeat(MAX_BYTES));
        assert!(err(&text).contains("the cap is"));
    }

    #[test]
    fn deep_nesting_is_rejected() {
        let mut text = String::new();
        for level in 0..MAX_DEPTH + 8 {
            text.push_str(&"  ".repeat(level));
            text.push_str("k:\n");
        }
        assert!(err(&text).contains("nests deeper"));
    }

    #[test]
    fn a_wide_document_is_rejected_by_the_node_cap() {
        let mut text = String::from("items:\n");
        for n in 0..MAX_NODES + 10 {
            text.push_str(&format!("  - {n}\n"));
        }
        assert!(err(&text).contains("more than"));
    }

    #[test]
    fn a_document_at_the_node_cap_is_accepted() {
        let mut text = String::from("items:\n");
        for n in 0..MAX_NODES - 2 {
            text.push_str(&format!("  - {n}\n"));
        }
        assert!(parse_value(&text, None).is_ok());
    }

    /// CM-24 puts a wall-clock bound on the rejection: the dev server parses
    /// front matter on every keystroke, so a bomb must be refused, not merely
    /// survived.
    #[test]
    fn every_rejection_is_immediate() {
        let bomb = {
            let mut text = String::from("a0: &a0 [x, x]\n");
            for level in 1..24 {
                text.push_str(&format!(
                    "a{level}: &a{level} [*a{}, *a{}]\n",
                    level - 1,
                    level - 1
                ));
            }
            text
        };
        let wide = format!("title: {}\n", "x".repeat(2 << 20));
        let deep = (0..40).fold(String::new(), |mut text, level| {
            text.push_str(&"  ".repeat(level));
            text.push_str("k:\n");
            text
        });
        for text in [&bomb, &wide, &deep] {
            let started = std::time::Instant::now();
            assert!(parse_value(text, None).is_err());
            assert!(
                started.elapsed() < std::time::Duration::from_millis(50),
                "took {:?}",
                started.elapsed()
            );
        }
    }

    #[test]
    fn invalid_yaml_keeps_the_core_code() {
        let error = parse_value("title: [unclosed\n", None).expect_err("invalid YAML");
        assert_eq!(error.code, code::E0101);
    }
}
