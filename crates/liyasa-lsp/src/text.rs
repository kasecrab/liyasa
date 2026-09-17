//! Byte offsets on one side, editor positions on the other.
//!
//! Liyasa spans are byte offsets (`liyasa_core::Span`). LSP positions are a
//! zero-based line and a `character` counted in whatever encoding the client
//! negotiated, which by default is UTF-16 code units. Everything that crosses
//! between the two crosses here.

use crate::protocol::{Position, PositionEncoding, Range};

/// One document's text with its line starts precomputed.
#[derive(Debug, Clone)]
pub struct Text {
    text: String,
    /// Byte offset of the first character of each line. Always starts with 0,
    /// so `line_starts.len()` is the line count of a document that does not end
    /// in a newline and one more than it does otherwise.
    line_starts: Vec<u32>,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let line_starts = line_starts(&text);
        Self { text, line_starts }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> u32 {
        // A source larger than 4 GiB is not a documentation page, and `Span`
        // could not address it either.
        u32::try_from(self.text.len()).unwrap_or(u32::MAX)
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn line_count(&self) -> u32 {
        u32::try_from(self.line_starts.len()).unwrap_or(u32::MAX)
    }

    /// The byte offset a position names, clamped into the document. A client
    /// whose idea of the text is one keystroke ahead of ours must not be able
    /// to panic the server, so every out-of-range input lands on a boundary
    /// rather than failing.
    pub fn offset_of(&self, at: Position, encoding: PositionEncoding) -> u32 {
        let Some(&start) = self.line_starts.get(at.line as usize) else {
            return self.len();
        };
        let end = self.line_end(at.line);
        let line = &self.text[start as usize..end as usize];

        let mut counted = 0u32;
        for (offset, ch) in line.char_indices() {
            if counted >= at.character {
                return start + u32::try_from(offset).unwrap_or(0);
            }
            counted += units(ch, encoding);
        }
        end
    }

    /// The position of a byte offset. An offset inside a character rounds down
    /// to that character's start, which is what an editor means by "here".
    pub fn position_of(&self, offset: u32, encoding: PositionEncoding) -> Position {
        let offset = offset.min(self.len());
        let line = match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(after) => after.saturating_sub(1),
        };
        let start = self.line_starts.get(line).copied().unwrap_or(0);
        let mut character = 0u32;
        for (at, ch) in self.text[start as usize..].char_indices() {
            let at = start + u32::try_from(at).unwrap_or(0);
            if at + u32::try_from(ch.len_utf8()).unwrap_or(1) > offset {
                break;
            }
            character += units(ch, encoding);
        }
        Position {
            line: u32::try_from(line).unwrap_or(0),
            character,
        }
    }

    pub fn range_of(&self, start: u32, end: u32, encoding: PositionEncoding) -> Range {
        Range {
            start: self.position_of(start, encoding),
            end: self.position_of(end.max(start), encoding),
        }
    }

    /// The text of one line without its terminator.
    pub fn line(&self, line: u32) -> &str {
        let Some(&start) = self.line_starts.get(line as usize) else {
            return "";
        };
        let end = self.line_end(line);
        &self.text[start as usize..end as usize]
    }

    /// The line a byte offset falls on.
    pub fn line_at(&self, offset: u32) -> u32 {
        self.position_of(offset, PositionEncoding::Utf8).line
    }

    /// The byte offset of the end of a line's content, before `\r\n` or `\n`.
    fn line_end(&self, line: u32) -> u32 {
        let start = self.line_starts.get(line as usize).copied().unwrap_or(0);
        let next = self
            .line_starts
            .get(line as usize + 1)
            .copied()
            .unwrap_or(self.len());
        let mut end = next;
        let bytes = self.text.as_bytes();
        if end > start && bytes.get(end as usize - 1) == Some(&b'\n') {
            end -= 1;
        }
        if end > start && bytes.get(end as usize - 1) == Some(&b'\r') {
            end -= 1;
        }
        end
    }
}

fn line_starts(text: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    for (at, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(u32::try_from(at + 1).unwrap_or(u32::MAX));
        }
    }
    // A trailing newline opens a last, empty line, which is where an editor
    // puts the cursor; `starts` already has it. A document that does not end in
    // one has no extra entry, which is also right.
    starts
}

/// How many units of `encoding` one character occupies.
fn units(ch: char, encoding: PositionEncoding) -> u32 {
    match encoding {
        PositionEncoding::Utf8 => u32::try_from(ch.len_utf8()).unwrap_or(1),
        PositionEncoding::Utf16 => u32::try_from(ch.len_utf16()).unwrap_or(1),
        PositionEncoding::Utf32 => 1,
    }
}
