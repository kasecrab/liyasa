//! The Markdown sink components serialize into (PRD RX-61).
//!
//! The agent serialization is plain CommonMark: a reader that knows nothing
//! about Liyasa directives must still get the whole page. The writer owns three
//! things a component should never have to think about — blank lines between
//! blocks, the line prefix that nesting inside a quote or a list item adds, and
//! how long a fence has to be to survive its own body.

use std::fmt::Write as _;

#[derive(Debug, Default)]
pub struct Markdown {
    buf: String,
    /// Written after every newline: `> ` inside a quote, spaces inside a list.
    prefix: String,
    /// Prefix used once, for the line a list marker starts.
    pending_marker: Option<String>,
    /// Set when a list item has just opened: its first block must not be
    /// preceded by a blank line, or every list serializes loose.
    suppress_block: bool,
    at_line_start: bool,
}

impl Markdown {
    pub fn new() -> Self {
        Self {
            at_line_start: true,
            ..Self::default()
        }
    }

    /// Ends the current block. The next write starts after one blank line.
    pub fn block(&mut self) -> &mut Self {
        if std::mem::take(&mut self.suppress_block) {
            return self.end_line();
        }
        if self.buf.is_empty() {
            return self;
        }
        self.end_line();
        if !self.buf.ends_with("\n\n") {
            self.raw_prefix();
            self.buf.push('\n');
            self.at_line_start = true;
        }
        self
    }

    /// Ends the current line without opening a new block.
    pub fn end_line(&mut self) -> &mut Self {
        if !self.at_line_start {
            self.buf.push('\n');
            self.at_line_start = true;
        }
        self
    }

    /// Writes inline text, re-prefixing every line break it contains.
    pub fn write(&mut self, text: &str) -> &mut Self {
        for (index, line) in text.split('\n').enumerate() {
            if index > 0 {
                self.buf.push('\n');
                self.at_line_start = true;
            }
            if !line.is_empty() {
                self.start_line();
                self.buf.push_str(line);
            }
        }
        self
    }

    pub fn line(&mut self, text: &str) -> &mut Self {
        self.write(text);
        self.end_line()
    }

    pub fn heading(&mut self, level: u8, text: &str) -> &mut Self {
        self.block();
        let hashes = "#".repeat(level.clamp(1, 6) as usize);
        self.line(&format!("{hashes} {}", text.trim()))
    }

    pub fn paragraph(&mut self, text: &str) -> &mut Self {
        self.block();
        self.line(text)
    }

    pub fn thematic_break(&mut self) -> &mut Self {
        self.block();
        self.line("---")
    }

    /// A fenced block. The fence is always longer than the longest backtick run
    /// in the body, so a body containing a fence cannot end it early.
    pub fn fence(&mut self, info: &str, body: &str) -> &mut Self {
        self.block();
        let fence = "`".repeat(fence_len(body));
        self.line(&format!("{fence}{info}"));
        if !body.is_empty() {
            self.write(body.strip_suffix('\n').unwrap_or(body));
            self.end_line();
        }
        self.line(&fence)
    }

    /// Runs `body` with `> ` added to every line.
    pub fn quote(&mut self, body: impl FnOnce(&mut Self)) -> &mut Self {
        self.block();
        // The quote's own separator is the block's; the first paragraph inside
        // must not add a second.
        self.suppress_block = true;
        self.nested("> ", None, body);
        self
    }

    /// Runs `body` as one list item: `marker` starts the first line, the rest is
    /// indented to match.
    pub fn item(&mut self, marker: &str, body: impl FnOnce(&mut Self)) -> &mut Self {
        self.end_line();
        let indent = " ".repeat(marker.chars().count());
        self.suppress_block = true;
        self.nested(&indent, Some(marker.to_owned()), body);
        self
    }

    /// The `n`th marker of an ordered list, `1.`-style.
    pub fn ordered_item(&mut self, number: u32, body: impl FnOnce(&mut Self)) -> &mut Self {
        self.item(&format!("{number}. "), body)
    }

    pub fn bullet_item(&mut self, body: impl FnOnce(&mut Self)) -> &mut Self {
        self.item("- ", body)
    }

    /// A table whose columns are as wide as their widest cell.
    pub fn table(&mut self, header: &[String], rows: &[Vec<String>]) -> &mut Self {
        if header.is_empty() {
            return self;
        }
        self.block();
        let mut widths: Vec<usize> = header.iter().map(|c| c.chars().count().max(3)).collect();
        for row in rows {
            for (at, cell) in row.iter().enumerate() {
                if let Some(width) = widths.get_mut(at) {
                    *width = (*width).max(cell.chars().count());
                }
            }
        }
        let render = |cells: &[String], widths: &[usize]| {
            let mut line = String::from("|");
            for (at, width) in widths.iter().enumerate() {
                let cell = cells.get(at).map_or("", String::as_str);
                let pad = width.saturating_sub(cell.chars().count());
                let _ = write!(line, " {cell}{} |", " ".repeat(pad));
            }
            line
        };
        let header_line = render(header, &widths);
        self.line(&header_line);
        let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
        let rule_line = render(&rule, &widths);
        self.line(&rule_line);
        for row in rows {
            let row_line = render(row, &widths);
            self.line(&row_line);
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// The document so far, with no trailing blank lines.
    pub fn finish(mut self) -> String {
        self.end_line();
        while self.buf.ends_with("\n\n") {
            self.buf.pop();
        }
        self.buf
    }

    pub fn as_str(&self) -> &str {
        &self.buf
    }

    fn nested(&mut self, prefix: &str, marker: Option<String>, body: impl FnOnce(&mut Self)) {
        let outer_prefix = self.prefix.clone();
        let outer_marker = self.pending_marker.take();
        self.prefix.push_str(prefix);
        self.pending_marker = marker.map(|m| format!("{outer_prefix}{m}"));
        body(self);
        self.end_line();
        self.prefix = outer_prefix;
        self.pending_marker = outer_marker;
    }

    fn start_line(&mut self) {
        if !self.at_line_start {
            return;
        }
        match self.pending_marker.take() {
            Some(marker) => self.buf.push_str(&marker),
            None => self.buf.push_str(&self.prefix),
        }
        self.at_line_start = false;
    }

    /// The prefix on an otherwise blank line, trailing spaces trimmed.
    fn raw_prefix(&mut self) {
        if self.at_line_start && !self.prefix.is_empty() {
            self.buf.push_str(self.prefix.trim_end());
            self.at_line_start = self.prefix.trim_end().is_empty();
        }
    }
}

/// How many backticks a fence needs to hold this body.
pub fn fence_len(body: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for ch in body.chars() {
        if ch == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    (longest + 1).max(3)
}

/// Escapes the characters that would start a Markdown construct in inline text.
pub fn escape_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// `[text](href)`, with both halves escaped.
pub fn link(text: &str, href: &str) -> String {
    format!("[{}]({})", escape_inline(text), escape_url(href))
}

pub fn escape_url(href: &str) -> String {
    href.replace(' ', "%20").replace(')', "%29")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_are_separated_by_one_blank_line() {
        let mut md = Markdown::new();
        md.paragraph("one");
        md.paragraph("two");
        assert_eq!(md.finish(), "one\n\ntwo\n");
    }

    #[test]
    fn a_leading_block_adds_no_blank_line() {
        let mut md = Markdown::new();
        md.heading(3, "Title");
        assert_eq!(md.finish(), "### Title\n");
    }

    #[test]
    fn a_fence_outgrows_its_body() {
        let mut md = Markdown::new();
        md.fence("rust", "let x = ```;\n");
        assert_eq!(md.finish(), "````rust\nlet x = ```;\n````\n");
    }

    #[test]
    fn a_short_body_keeps_three_backticks() {
        assert_eq!(fence_len("plain"), 3);
        assert_eq!(fence_len("a ` b"), 3);
        assert_eq!(fence_len("a ```` b"), 5);
    }

    #[test]
    fn a_quote_prefixes_every_line() {
        let mut md = Markdown::new();
        md.quote(|md| {
            md.paragraph("**Note**");
            md.paragraph("body");
        });
        assert_eq!(md.finish(), "> **Note**\n>\n> body\n");
    }

    #[test]
    fn a_list_item_indents_its_continuation() {
        let mut md = Markdown::new();
        md.bullet_item(|md| {
            md.paragraph("first");
            md.paragraph("second");
        });
        assert_eq!(md.finish(), "- first\n\n  second\n");
    }

    #[test]
    fn ordered_items_carry_their_number() {
        let mut md = Markdown::new();
        md.ordered_item(1, |md| {
            md.paragraph("one");
        });
        md.ordered_item(2, |md| {
            md.paragraph("two");
        });
        assert_eq!(md.finish(), "1. one\n2. two\n");
    }

    #[test]
    fn a_fence_inside_a_list_item_stays_indented() {
        let mut md = Markdown::new();
        md.bullet_item(|md| {
            md.paragraph("run");
            md.fence("sh", "ls\n");
        });
        assert_eq!(md.finish(), "- run\n\n  ```sh\n  ls\n  ```\n");
    }

    #[test]
    fn a_table_pads_to_its_widest_cell() {
        let mut md = Markdown::new();
        md.table(
            &["Name".into(), "Type".into()],
            &[vec!["limit".into(), "integer".into()]],
        );
        assert_eq!(
            md.finish(),
            "| Name  | Type    |\n| ----- | ------- |\n| limit | integer |\n"
        );
    }

    #[test]
    fn inline_escaping_covers_the_starters() {
        assert_eq!(escape_inline("a_b *c* [d]"), r"a\_b \*c\* \[d\]");
    }

    #[test]
    fn a_link_escapes_both_halves() {
        assert_eq!(link("a [b]", "/x y"), "[a \\[b\\]](/x%20y)");
    }

    #[test]
    fn finish_leaves_exactly_one_trailing_newline() {
        let mut md = Markdown::new();
        md.paragraph("x");
        md.block();
        md.block();
        assert_eq!(md.finish(), "x\n");
    }
}
