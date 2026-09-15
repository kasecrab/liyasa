//! Line geometry shared by the scanner and the formatter.
//!
//! Every decision the scanner makes about a line — is this a fence, is this
//! indented code, is this a directive — is made against the line's *content*,
//! which is what is left after the blockquote markers and the indentation that
//! the enclosing list item owns. Keeping that arithmetic in one place is what
//! stops the fence rule and the indented-code rule from disagreeing.

/// A tab advances to the next multiple of this, as CommonMark defines it.
pub const TAB_STOP: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line<'a> {
    /// Byte offset of the first byte of the line.
    pub start: usize,
    /// Byte offset just past the line's last byte, the newline excluded.
    pub end: usize,
    /// Byte offset just past the newline, or of the end of the text.
    pub next: usize,
    /// Byte offset where [`Line::content`] begins.
    pub content_at: usize,
    /// The line without its blockquote markers or leading whitespace.
    pub content: &'a str,
    /// Columns of indentation after the blockquote markers, tabs expanded.
    pub indent: usize,
    /// How many `>` markers the line carries.
    pub quotes: usize,
}

impl Line<'_> {
    pub fn is_blank(&self) -> bool {
        self.content.is_empty()
    }
}

/// Splits `text` into lines from `from`, keeping byte offsets into `text`.
pub fn split(text: &str, from: usize) -> Vec<Line<'_>> {
    let mut out = Vec::new();
    let mut at = from;
    for raw in text[from..].split_inclusive('\n') {
        let line = raw.trim_end_matches(['\r', '\n']);
        out.push(measure(line, at, at + raw.len()));
        at += raw.len();
    }
    out
}

fn measure(line: &str, start: usize, next: usize) -> Line<'_> {
    let mut quotes = 0usize;
    let mut column = 0usize;
    let mut at = 0usize;

    while let Some(ch) = line[at..].chars().next() {
        match ch {
            ' ' => column += 1,
            '\t' => column += TAB_STOP - column % TAB_STOP,
            // A blockquote marker resets the indentation the content sits in;
            // one optional space after it belongs to the marker.
            '>' => {
                quotes += 1;
                column = 0;
                at += 1;
                if line[at..].starts_with(' ') {
                    at += 1;
                }
                continue;
            }
            _ => break,
        }
        at += ch.len_utf8();
    }

    Line {
        start,
        end: start + line.len(),
        next,
        content_at: start + at,
        content: line[at..].trim_end(),
        indent: column,
        quotes,
    }
}

/// The content column of the innermost open list item, so that indentation the
/// list owns is not mistaken for an indented code block.
#[derive(Debug, Default)]
pub struct ListStack(Vec<usize>);

impl ListStack {
    /// The column content must reach before it is indented code.
    pub fn content_column(&self) -> usize {
        self.0.last().copied().unwrap_or_default()
    }

    /// Feeds one non-blank line, closing items the line has outdented past and
    /// opening one when the line starts a list item.
    pub fn feed(&mut self, line: &Line<'_>) {
        while self
            .0
            .last()
            .is_some_and(|column| line.indent < *column && !line.is_blank())
        {
            self.0.pop();
        }
        if let Some(column) = marker_width(line.content) {
            self.0.push(line.indent + column);
        }
    }
}

/// The width of a bullet or ordered list marker plus the spaces after it, or
/// `None` when the line does not start a list item.
fn marker_width(content: &str) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut at = match bytes.first()? {
        b'-' | b'+' | b'*' => 1,
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            if digits > 9 || !matches!(bytes.get(digits), Some(b'.' | b')')) {
                return None;
            }
            digits + 1
        }
        _ => return None,
    };
    let marker = at;
    let spaces = content[at..].len() - content[at..].trim_start_matches([' ', '\t']).len();
    if spaces == 0 && at < content.len() {
        // `-item` is not a list; `-` alone on a line is.
        return None;
    }
    // A marker followed by five or more spaces starts with one space of
    // content, exactly as CommonMark says.
    at += if spaces > TAB_STOP { 1 } else { spaces.max(1) };
    let _ = marker;
    Some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> Line<'_> {
        split(text, 0).remove(0)
    }

    #[test]
    fn measures_a_plain_line() {
        let found = line("hello\n");
        assert_eq!(found.content, "hello");
        assert_eq!((found.indent, found.quotes, found.content_at), (0, 0, 0));
        assert_eq!((found.start, found.end, found.next), (0, 5, 6));
    }

    #[test]
    fn a_tab_advances_to_the_next_stop() {
        assert_eq!(line(" \tcode\n").indent, 4);
        assert_eq!(line("\tcode\n").indent, 4);
        assert_eq!(line("  \t code\n").indent, 5);
    }

    #[test]
    fn a_blockquote_marker_resets_the_indent() {
        let found = line("> > text\n");
        assert_eq!(found.quotes, 2);
        assert_eq!(found.indent, 0);
        assert_eq!(found.content, "text");
    }

    #[test]
    fn a_blank_line_has_no_content() {
        assert!(line("   \n").is_blank());
        assert!(line("\n").is_blank());
        assert!(!line("x\n").is_blank());
    }

    #[test]
    fn the_last_line_may_have_no_newline() {
        let found = line("tail");
        assert_eq!((found.end, found.next), (4, 4));
    }

    #[test]
    fn a_list_item_owns_its_indentation() {
        let mut stack = ListStack::default();
        for found in split("- item\n  still the item\n", 0) {
            stack.feed(&found);
        }
        assert_eq!(stack.content_column(), 2);
    }

    #[test]
    fn an_ordered_marker_counts_its_digits() {
        let mut stack = ListStack::default();
        stack.feed(&line("10. item\n"));
        assert_eq!(stack.content_column(), 4);
    }

    #[test]
    fn outdenting_closes_the_item() {
        let mut stack = ListStack::default();
        for found in split("- item\n  - inner\nback\n", 0) {
            stack.feed(&found);
        }
        assert_eq!(stack.content_column(), 0);
    }

    #[test]
    fn a_dash_without_a_space_is_not_a_marker() {
        let mut stack = ListStack::default();
        stack.feed(&line("-item\n"));
        assert_eq!(stack.content_column(), 0);
    }

    #[test]
    fn lines_tile_the_text() {
        let text = "a\n\nb\r\nc";
        let found = split(text, 0);
        assert_eq!(found.len(), 4);
        let joined: String = found.iter().map(|l| &text[l.start..l.next]).collect();
        assert_eq!(joined, text);
    }
}
