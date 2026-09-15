//! What a line-based pass must not touch (PRD §7.5.1 item 3, CM-11).
//!
//! The leaf rewrite runs before comrak, so it has to recognize code on its own.
//! It recognizes fenced code and indented code; a leaf directive at a list
//! item's content column plus four spaces is indented code that this pass
//! rewrites anyway, which is the one place the leaf path still needs comrak's
//! container algorithm and does not have it.
// TODO(rfc-0003): `rewrite::tests::a_leaf_deep_inside_a_list_is_missed` pins
// that gap, and fails the day it closes.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Split<'a> {
    /// The blockquote markers and indentation before the content.
    pub lead: &'a str,
    pub content: &'a str,
    /// Columns of indentation after the last blockquote marker, tabs as four.
    pub indent: usize,
}

/// Separates a line's blockquote and indentation prefix from its content.
pub fn split(line: &str) -> Split<'_> {
    let mut at = 0;
    let mut indent = 0;
    for (offset, ch) in line.char_indices() {
        match ch {
            ' ' => indent += 1,
            '\t' => indent += 4 - indent % 4,
            '>' => indent = 0,
            _ => {
                at = offset;
                break;
            }
        }
        at = offset + ch.len_utf8();
    }
    Split {
        lead: &line[..at],
        content: &line[at..],
        indent,
    }
}

/// A line's content with its blockquote and indentation prefix removed.
pub fn content_of(line: &str) -> &str {
    split(line).content
}

/// Fenced and indented code, tracked across lines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Fences {
    open: Option<(char, usize)>,
}

impl Fences {
    /// Whether this line is code and must be left alone. The line that closes
    /// a fence is part of it, so it answers `true` too.
    pub fn step(&mut self, content: &str, indent: usize) -> bool {
        if let Some((ch, len)) = self.open {
            if closes(content, ch, len) {
                self.open = None;
            }
            return true;
        }
        if let Some(fence) = opens(content) {
            self.open = Some(fence);
            return true;
        }
        indent >= 4
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
}

fn opens(content: &str) -> Option<(char, usize)> {
    ['`', '~'].into_iter().find_map(|ch| {
        let len = content.chars().take_while(|c| *c == ch).count();
        // An info string may not contain a backtick, which is what keeps a
        // code span from being read as a fence.
        (len >= 3 && !(ch == '`' && content[len..].contains('`'))).then_some((ch, len))
    })
}

fn closes(content: &str, ch: char, len: usize) -> bool {
    let run = content.chars().take_while(|c| *c == ch).count();
    run >= len && content[run..].trim().is_empty()
}

#[cfg(test)]
mod tests;
