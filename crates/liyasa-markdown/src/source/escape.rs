//! Interpolating values that are not `operator`-trusted (CM-20).
//!
//! A fact fetched from a URL, a value a command printed, a `reader.*` field,
//! anything an automation supplied: none of it may open a Markdown block, an
//! inline run, or a directive when it lands in prose. The rule is fail-loud
//! rather than best-effort.
//!
//! Multi-line values are **rejected**, not escaped. A single line can always be
//! neutralized by escaping its punctuation; a value with a line break in it can
//! open a construct on a line the escaping never sees, and no escape set fixes
//! that. Refusing it is `E0320`.
//!
//! `operator`-trust values — `file` and `manual` fact sources, config
//! variables, front matter — are interpolated verbatim and never come here.

use liyasa_core::diagnostics::{Diagnostic, code};

/// `Diagnostic` is 160 bytes and this runs per interpolation, so the error
/// travels boxed.
pub type EscapeError = Box<Diagnostic>;

/// Characters that can open an inline construct anywhere in a line.
const INLINE: &[char] = &['`', '~', '*', '_', '[', ']', '<', '>', '|', '\\', '!'];

/// Characters that open a block construct, but only at the start of a line.
const BLOCK_OPENERS: &[char] = &['#', '>', '-', '+', ':'];

/// Every line break Markdown or the surrounding HTML would honour.
const BREAKS: &[char] = &['\n', '\r', '\u{2028}', '\u{2029}'];

/// Escapes a single-line untrusted value for interpolation into Markdown.
///
/// `at_line_start` tells the function whether the value's first character will
/// be the first thing on its line, which is the only place a block opener can
/// open a block.
pub fn escape_untrusted_markdown(value: &str, at_line_start: bool) -> Result<String, EscapeError> {
    if let Some(at) = value.find(BREAKS) {
        return Err(Box::new(
            Diagnostic::new(
                code::E0320,
                "untrusted value contains a line break and was not interpolated",
            )
            .help(format!(
                "the break is at byte {at}; a value from a `url`, `command`, or `screenshot` \
                 source, from `reader.*`, or from an automation must be a single line"
            )),
        ));
    }

    let mut out = String::with_capacity(value.len() + 8);
    // Leading indentation becomes one space: four of them would be code.
    let trimmed = value.trim_start_matches([' ', '\t']);
    if trimmed.len() != value.len() {
        out.push(' ');
    }
    let leading_digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();

    for (at, ch) in trimmed.char_indices() {
        // Up to three spaces of indentation do not stop a block from opening,
        // so the first *content* character is the one that has to be escaped,
        // even when a space now precedes it.
        let opens_block = at_line_start && at == 0 && BLOCK_OPENERS.contains(&ch);
        // `1.` and `1)` open an ordered list, so the punctuation after a
        // leading digit run is escaped even though it is not at column zero.
        let ordered =
            at_line_start && leading_digits > 0 && at == leading_digits && matches!(ch, '.' | ')');
        if INLINE.contains(&ch) || opens_block || ordered {
            out.push('\\');
        }
        out.push(ch);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use liyasa_core::document::Segment;
    use liyasa_core::span::{SourceId, Span};

    use super::*;

    fn escaped(value: &str) -> String {
        escape_untrusted_markdown(value, true).expect("a single-line value")
    }

    fn inline(value: &str) -> String {
        escape_untrusted_markdown(value, false).expect("a single-line value")
    }

    /// The invariant the fuzz target asserts: no new block, inline, or
    /// directive node appears.
    ///
    /// An escaped value lands in *expanded* text, which is re-scanned for
    /// directives and parsed as Markdown but never templated again — minijinja
    /// has already run, and the value is its output. `{{` and `{%` in a value
    /// are therefore literal text, which is why CM-20's escape set does not
    /// name `{`, and why the template segments and their `E0202` are ignored
    /// here.
    fn assert_inert(value: &str) {
        let text = format!("{}\n", escaped(value));
        assert!(
            !opens_a_block(&text),
            "{value:?} escaped to {text:?}, which still opens a block"
        );
        let (document, diagnostics) = crate::source::scan(&text, SourceId(0));
        assert!(
            document.segments.iter().all(|segment| matches!(
                segment,
                Segment::Markdown { .. } | Segment::Template { .. }
            )),
            "{value:?} escaped to {text:?} and produced {:?}",
            document.segments
        );
        let structural: Vec<_> = diagnostics
            .iter()
            .filter(|d| !matches!(d.code.as_str(), "E0202" | "E0210"))
            .map(|d| d.code)
            .collect();
        assert!(
            structural.is_empty(),
            "{value:?} escaped to {text:?} and reported {structural:?}"
        );
    }

    /// Stated independently of the escaper: whether the line could still open
    /// a Markdown block. The scanner alone cannot answer this, because a
    /// heading and a paragraph are both one `Markdown` segment.
    fn opens_a_block(text: &str) -> bool {
        let line = text.trim_end_matches('\n');
        // Leading indentation in columns; an interior tab opens nothing.
        let indent = line
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .fold(
                0usize,
                |at, ch| if ch == '\t' { at + 4 - at % 4 } else { at + 1 },
            );
        if indent >= 4 {
            return true;
        }
        let rest = line.trim_start_matches([' ', '\t']);
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        matches!(
            rest.as_bytes().first(),
            Some(b'#' | b'>' | b'-' | b'+' | b'*' | b':' | b'~' | b'`' | b'=' | b'_')
        ) || (digits > 0 && matches!(rest.as_bytes().get(digits), Some(b'.' | b')')))
    }

    #[test]
    fn a_plain_value_is_unchanged() {
        assert_eq!(
            escaped("1000 requests per minute"),
            "1000 requests per minute"
        );
    }

    #[test]
    fn a_line_break_is_rejected() {
        for value in ["a\nb", "a\rb", "a\u{2028}b", "a\u{2029}b", "a\r\nb"] {
            let error = escape_untrusted_markdown(value, true).expect_err("a break");
            assert_eq!(error.code, code::E0320);
        }
    }

    #[test]
    fn inline_openers_are_escaped_anywhere() {
        assert_eq!(inline("a `b` c"), "a \\`b\\` c");
        assert_eq!(inline("**bold**"), "\\*\\*bold\\*\\*");
        assert_eq!(inline("[link](x)"), "\\[link\\](x)");
        assert_eq!(inline("<script>"), "\\<script\\>");
        assert_eq!(inline("a | b"), "a \\| b");
        assert_eq!(inline("a \\ b"), "a \\\\ b");
        assert_eq!(inline("!bang"), "\\!bang");
    }

    #[test]
    fn a_block_opener_is_escaped_only_at_a_line_start() {
        assert_eq!(escaped("# heading"), "\\# heading");
        assert_eq!(inline("# heading"), "# heading");
        assert_eq!(escaped("+ item"), "\\+ item");
        assert_eq!(inline("+ item"), "+ item");
    }

    #[test]
    fn an_ordered_list_marker_is_escaped() {
        assert_eq!(escaped("1. first"), "1\\. first");
        assert_eq!(escaped("12) first"), "12\\) first");
        assert_eq!(escaped("v1.2 released"), "v1.2 released");
        assert_eq!(inline("1. first"), "1. first");
    }

    #[test]
    fn leading_indentation_collapses_to_one_space() {
        assert_eq!(escaped("    code"), " code");
        assert_eq!(escaped("\t\tcode"), " code");
        assert_eq!(escaped("  # heading"), " \\# heading");
    }

    #[test]
    fn the_acceptance_values_open_nothing() {
        for value in [
            "> quote",
            "- item",
            "1. item",
            "~~~",
            ":::note",
            "# heading",
            "<!--ly:0000:o:0-->",
            "```bash",
            "|a|b|",
            "    indented",
            "*emphasis*",
            "[ref]: http://x",
            "<div>",
            "::button{label=\"x\"}",
            "{{ x }}",
            "{% for x in y %}",
            "---",
            "___",
            "+ item",
            "0) item",
        ] {
            assert_inert(value);
        }
    }

    #[test]
    fn a_marker_prefix_cannot_survive_escaping() {
        let out = escaped("<!--ly:deadbeef:o:0-->");
        assert!(!out.contains("<!--ly:"), "{out}");
    }

    /// Deterministic stand-in for the `untrusted-escape` fuzz target: every
    /// string over the dangerous alphabet escapes to inert text.
    #[test]
    fn escaping_is_inert_over_the_dangerous_alphabet() {
        const ALPHABET: &[char] = &[
            '`', '~', '*', '_', '[', ']', '<', '>', '|', '\\', '!', '#', '-', '+', ':', '.', ')',
            '(', '{', '}', '%', '1', ' ', '\t', 'a', '"', '\'', '&', ';', '/',
        ];
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..2_000 {
            let length = (next() % 12) as usize + 1;
            let value: String = (0..length)
                .map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize])
                .collect();
            assert_inert(&value);
        }
    }

    #[test]
    fn a_rejection_can_be_located() {
        let error = escape_untrusted_markdown("a\nb", true).expect_err("a break");
        let span = Span::new(SourceId(0), 4, 9);
        assert_eq!((*error).at(span).span, Some(span));
    }
}
