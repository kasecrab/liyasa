//! Interned sources and the span-to-position mapping (PRD §34.9).
//!
//! A `Diagnostic` is meaningless across crates without this: `SourceId` is only
//! an index into one build's map, which is why the JSON form of a diagnostic
//! carries the path instead.

use std::sync::Arc;

use crate::ids::Fingerprint;
use crate::span::{LineCol, SourceId, Span};
use crate::vfs::VfsPath;

pub struct SourceFile {
    pub path: VfsPath,
    pub text: Arc<str>,
    /// Byte offset of the first character of each line; always starts at 0.
    pub line_starts: Vec<u32>,
    pub fingerprint: Fingerprint,
}

#[derive(Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, path: VfsPath, text: Arc<str>) -> SourceId {
        let fingerprint = Fingerprint::of(text.as_bytes());
        let mut line_starts = vec![0u32];
        line_starts.extend(
            text.bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(at, _)| (at + 1) as u32),
        );
        let id = SourceId(self.files.len() as u32);
        self.files.push(SourceFile {
            path,
            text,
            line_starts,
            fingerprint,
        });
        id
    }

    pub fn get(&self, id: SourceId) -> &SourceFile {
        self.files
            .get(id.0 as usize)
            .unwrap_or_else(|| unreachable!("SourceId {} is not in this map", id.0))
    }

    pub fn try_get(&self, id: SourceId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }

    pub fn find(&self, path: &VfsPath) -> Option<SourceId> {
        self.files
            .iter()
            .position(|f| &f.path == path)
            .map(|at| SourceId(at as u32))
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn line_col(&self, span: Span) -> (LineCol, LineCol) {
        let file = self.get(span.source);
        (file.line_col(span.start), file.line_col(span.end))
    }

    /// The span's lines with `context` lines on either side, as they appear in
    /// the source. Used by the CLI renderer and by RFC and error documentation.
    pub fn snippet(&self, span: Span, context: u32) -> String {
        let file = self.get(span.source);
        let first = file.line_col(span.start).line.saturating_sub(context + 1) as usize;
        let last = (file.line_col(span.end).line + context) as usize;
        file.text
            .lines()
            .skip(first)
            .take(last.saturating_sub(first))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl SourceFile {
    /// The 1-based line and the 1-based column, counted in UTF-8 bytes.
    ///
    /// An offset past the end clamps to the end rather than panicking: a
    /// diagnostic about a truncated file must still be renderable.
    pub fn line_col(&self, offset: u32) -> LineCol {
        let offset = offset.min(self.text.len() as u32);
        let line = self
            .line_starts
            .partition_point(|start| *start <= offset)
            .max(1);
        let start = self.line_starts.get(line - 1).copied().unwrap_or_default();
        LineCol {
            line: line as u32,
            col: offset - start + 1,
        }
    }

    /// The byte offset of a 1-based line and column, or `None` when either is
    /// out of range.
    pub fn offset(&self, at: LineCol) -> Option<u32> {
        let start = self.line_starts.get(at.line.checked_sub(1)? as usize)?;
        let offset = start + at.col.checked_sub(1)?;
        (offset <= self.text.len() as u32).then_some(offset)
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }
}
