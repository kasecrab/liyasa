//! CommonMark-correct inline code span matching (CM-23).
//!
//! The templating pass runs before the Markdown parser, so it has to decide on
//! its own which backticks delimit code. A line-local heuristic gets that
//! wrong for the case documentation hits constantly: a span that wraps across
//! a soft line break while showing `{{ x }}` or `{% for %}` to the reader.
//!
//! The rule is the spec's backtick-string rule. A run of *n* backticks that is
//! neither preceded nor followed by a backtick opens a span, and the span ends
//! at the next run of **exactly** *n*. A run that never finds its partner is
//! literal text. Callers pass one block's text at a time, which is what keeps a
//! span from crossing a blank line, a list-item boundary, or any other block
//! boundary.

use std::ops::Range;

/// The inline code spans of one block's text, delimiters included, in order
/// and non-overlapping.
pub fn code_spans(text: &str) -> Vec<Range<usize>> {
    let runs = backtick_runs(text);
    let mut out = Vec::new();
    let mut open = 0;
    while open < runs.len() {
        let Some(close) = (open + 1..runs.len()).find(|at| runs[*at].len == runs[open].len) else {
            // No partner: this run is literal, and a longer run later may still
            // open a span of its own.
            open += 1;
            continue;
        };
        out.push(runs[open].start..runs[close].start + runs[close].len);
        open = close + 1;
    }
    out
}

/// Whether `offset` falls inside one of `spans`, which must be sorted.
pub fn is_masked(spans: &[Range<usize>], offset: usize) -> bool {
    let at = spans.partition_point(|span| span.start <= offset);
    at.checked_sub(1).is_some_and(|at| offset < spans[at].end)
}

struct Run {
    start: usize,
    len: usize,
}

/// Backslash-escaped backticks are literal and do not join a run, so
/// `` \`x` `` has one run, not two, and opens nothing.
fn backtick_runs(text: &str) -> Vec<Run> {
    // Backticks and backslashes are ASCII, and no UTF-8 continuation byte is
    // ever ASCII, so scanning bytes cannot land inside a character.
    let bytes = text.as_bytes();
    let mut runs = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'`' => {
                let start = at;
                while at < bytes.len() && bytes[at] == b'`' {
                    at += 1;
                }
                runs.push(Run {
                    start,
                    len: at - start,
                });
            }
            _ => at += 1,
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(text: &str) -> Vec<&str> {
        code_spans(text)
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }

    #[test]
    fn matches_a_single_span() {
        assert_eq!(spans("a `x` b"), ["`x`"]);
    }

    #[test]
    fn a_double_run_wraps_a_single_backtick() {
        assert_eq!(spans("a ``x ` y`` b"), ["``x ` y``"]);
    }

    #[test]
    fn a_span_crosses_a_soft_line_break() {
        assert_eq!(
            spans("show `{{ x }}\nand {% for %}` here"),
            ["`{{ x }}\nand {% for %}`"]
        );
    }

    #[test]
    fn template_syntax_inside_a_span_is_covered() {
        let text = "use `{% raw %}` or `{{ x }}`";
        assert_eq!(spans(text), ["`{% raw %}`", "`{{ x }}`"]);
    }

    #[test]
    fn an_unmatched_run_is_literal() {
        assert_eq!(spans("a ` b"), Vec::<&str>::new());
        assert_eq!(spans("a ``` b ` c"), Vec::<&str>::new());
    }

    #[test]
    fn a_run_of_a_different_length_stays_inside_the_span() {
        assert_eq!(spans("``a` b``"), ["``a` b``"]);
    }

    #[test]
    fn an_escaped_backtick_opens_nothing() {
        assert_eq!(spans("\\`x` y"), Vec::<&str>::new());
    }

    #[test]
    fn a_literal_run_does_not_consume_a_later_pair() {
        assert_eq!(spans("`` a ` b ` c"), ["` b `"]);
    }

    #[test]
    fn spans_do_not_overlap() {
        let found = code_spans("`a` `b` `c`");
        assert_eq!(found, [0..3, 4..7, 8..11]);
    }

    #[test]
    fn masking_lookup_matches_the_ranges() {
        let text = "a `x` b";
        let found = code_spans(text);
        assert!(!is_masked(&found, 0));
        assert!(is_masked(&found, 2));
        assert!(is_masked(&found, 4));
        assert!(!is_masked(&found, 5));
    }

    #[test]
    fn a_trailing_backslash_does_not_run_off_the_end() {
        assert_eq!(spans("a `x` \\"), ["`x`"]);
    }
}
