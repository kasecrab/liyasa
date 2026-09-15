//! Positions, composed rewritten to expanded to source (§7.5.1 item 2).
//!
//! comrak reports lines and columns; everything downstream of the parser wants
//! byte offsets, and everything a reader sees wants the offset in the file they
//! wrote. Three coordinate spaces, composed in one place so no caller has to
//! remember the order.

use liyasa_core::markdown::{RewriteMap, SpanMap};
use liyasa_core::{Origin, SourceId, Span};

/// Line starts of one text, for turning a 1-based line and column into a byte
/// offset.
#[derive(Debug, Clone, Default)]
pub struct Lines {
    starts: Vec<u32>,
    len: u32,
}

impl Lines {
    pub fn of(text: &str) -> Self {
        let mut starts = vec![0];
        let mut at = 0u32;
        for line in text.split_inclusive('\n') {
            at += line.len() as u32;
            starts.push(at);
        }
        Self {
            starts,
            len: text.len() as u32,
        }
    }

    /// A 1-based line and column as a byte offset, clamped to the text.
    pub fn offset(&self, line: usize, column: usize) -> u32 {
        let start = self
            .starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(self.len);
        let end = self.starts.get(line).copied().unwrap_or(self.len);
        (start + column.saturating_sub(1) as u32).min(end)
    }
}

/// Composes the two maps: a comrak position becomes an expanded span, and an
/// expanded span becomes the origin a reader is shown.
pub struct Positions<'a> {
    pub source: SourceId,
    lines: Lines,
    rewrite: &'a RewriteMap,
    expansion: &'a SpanMap,
}

impl<'a> Positions<'a> {
    pub fn new(
        source: SourceId,
        rewritten: &str,
        rewrite: &'a RewriteMap,
        expansion: &'a SpanMap,
    ) -> Self {
        Self {
            source,
            lines: Lines::of(rewritten),
            rewrite,
            expansion,
        }
    }

    /// A comrak source position as a span in expanded coordinates. comrak's end
    /// column is inclusive; spans are half-open.
    pub fn span(&self, sourcepos: comrak::nodes::Sourcepos) -> Span {
        let start = self.rewrite.to_expanded(
            self.lines
                .offset(sourcepos.start.line, sourcepos.start.column),
        );
        let end = self.rewrite.to_expanded(
            self.lines
                .offset(sourcepos.end.line, sourcepos.end.column + 1),
        );
        Span::new(self.source, start, start.max(end))
    }

    /// Where a reader wrote the bytes an expanded span covers.
    ///
    /// The expansion map's runs carry the source span of the whole run, so the
    /// offset within the run carries across; a span with no run behind it came
    /// from the page itself and is already in source coordinates.
    pub fn origin(&self, expanded: Span) -> Origin {
        let Some((run_start, run_end, origin)) = self
            .expansion
            .origins_in(expanded.start, expanded.end.max(expanded.start + 1))
            .next()
        else {
            return Origin::at(expanded);
        };
        let Some(span) = origin.span else {
            return origin.clone();
        };
        let shift = expanded.start.saturating_sub(*run_start);
        let end = expanded.end.min(*run_end);
        Origin {
            span: Some(Span::new(
                span.source,
                span.start + shift,
                (span.start + shift + end.saturating_sub(expanded.start)).min(span.end),
            )),
            frames: origin.frames.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
