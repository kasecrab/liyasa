//! Byte-offset positions (PRD §34.9).

use serde::{Deserialize, Serialize};

/// A path interned into a [`SourceMap`](crate::source_map::SourceMap) for the
/// life of one build.
///
/// Diagnostics are serialized with the path rather than this index so they
/// survive crate and process boundaries (§34.9).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct SourceId(pub u32);

/// A half-open byte range within one source.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
pub struct Span {
    pub source: SourceId,
    pub start: u32,
    pub end: u32,
}

/// A 1-based line and a 1-based column counted in UTF-8 bytes, not characters.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

impl Span {
    pub const fn new(source: SourceId, start: u32, end: u32) -> Self {
        Self { source, start, end }
    }

    pub const fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    pub const fn contains(&self, offset: u32) -> bool {
        self.start <= offset && offset < self.end
    }

    /// The smallest span covering both. Panics on a cross-source join, which is
    /// always a bug in the caller rather than user input.
    pub fn join(self, other: Self) -> Self {
        assert_eq!(
            self.source, other.source,
            "cannot join spans across sources"
        );
        Self {
            source: self.source,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}
