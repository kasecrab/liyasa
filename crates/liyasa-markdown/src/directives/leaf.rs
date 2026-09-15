//! Leaf directives `::name{props}` (`plan/rfcs/0003-parser-spike.md` item 2).
//!
//! comrak does not recognize them, and by the time it has parsed the paragraph
//! that holds one the text is no longer recoverable — `::image{src=a_b_c}` has
//! had emphasis applied to `_b_`. So a leaf keeps the marker rewrite of
//! §7.5.1 item 2, on the one line it occupies.
//!
//! The rewrite is far smaller than the container one it replaced: a leaf is a
//! single line, it never nests, and the substitution preserves line numbers, so
//! only the column within that one line has to be recovered — through the
//! directive's recorded span, never by arithmetic on the marker.
//!
//! A literal `<!--ly:` in the text is escaped before anything is written, and
//! the nonce is 128 bits generated per build, so a marker cannot be forged.

use std::fmt::Write as _;

use liyasa_core::diagnostics::code;
use liyasa_core::document::Props;
use liyasa_core::markdown::{ComponentKind, DirectiveInfo, DirectiveTable, Expanded, RewriteMap};
use liyasa_core::{Diagnostic, Diagnostics, SourceId, Span};

use super::{info, mask};

/// The literal prefix a forged marker would have to reproduce.
pub const MARKER_PREFIX: &str = "<!--ly:";
const ESCAPED_PREFIX: &str = "<!--&#108;y:";

pub struct Rewritten {
    pub text: String,
    pub map: RewriteMap,
    pub table: DirectiveTable,
    pub diagnostics: Diagnostics,
}

/// Replaces every leaf directive with an opaque marker comment and records what
/// it stood for. The returned text has the same number of lines as the input.
pub fn rewrite(expanded: &Expanded, nonce: [u8; 16]) -> Rewritten {
    let source = source_of(expanded);
    let nonce_hex = nonce.iter().fold(String::with_capacity(32), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    });

    let mut out = String::with_capacity(expanded.text.len());
    let mut map: Vec<(u32, i32)> = Vec::new();
    let mut table: Vec<DirectiveInfo> = Vec::new();
    let mut diagnostics = Diagnostics::new();
    let mut fence = mask::Fences::default();
    let mut at = 0usize;
    // Cumulative over the lines *before* an entry: expanded = rewritten - delta.
    let mut delta = 0i32;

    for raw in expanded.text.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\r', '\n']);
        let eol = &raw[line.len()..];
        let start = at;
        at += raw.len();

        let mask::Split {
            lead,
            content,
            indent,
        } = mask::split(line);
        let content_at = start + lead.len();

        if fence.step(content, indent) {
            emit(&mut out, &mut map, &mut delta, raw, raw);
            continue;
        }

        if content.contains(MARKER_PREFIX) {
            let escaped = escape(content, source, content_at as u32, &mut diagnostics);
            emit(
                &mut out,
                &mut map,
                &mut delta,
                raw,
                &format!("{lead}{escaped}{eol}"),
            );
            continue;
        }

        let Some(info) = leaf_of(content) else {
            emit(&mut out, &mut map, &mut delta, raw, raw);
            continue;
        };
        let name_at = content_at as u32 + 2;
        diagnostics.extend(info.diagnostics(source, name_at));

        let id = table.len();
        table.push(DirectiveInfo {
            prop_spans: prop_spans(&info, source, name_at),
            name: info.name,
            props: info.props,
            kind: ComponentKind::Leaf,
            span: Span::new(source, start as u32, (start + line.len()) as u32),
        });
        emit(
            &mut out,
            &mut map,
            &mut delta,
            raw,
            &format!("{lead}{MARKER_PREFIX}{nonce_hex}:l:{id}-->{eol}"),
        );
    }

    Rewritten {
        text: out,
        map: RewriteMap(map),
        table: DirectiveTable(table),
        diagnostics,
    }
}

/// The marker ID a rewritten HTML comment stands for, or `None` when the text
/// is not this build's marker.
pub fn marker_id(html: &str, nonce: [u8; 16]) -> Option<usize> {
    let nonce_hex = nonce.iter().fold(String::with_capacity(32), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    });
    let body = html
        .trim()
        .strip_prefix(MARKER_PREFIX)?
        .strip_suffix("-->")?;
    let rest = body.strip_prefix(&nonce_hex)?.strip_prefix(":l:")?;
    rest.parse().ok()
}

/// `::name{props}`, and never `:::name`, which is comrak's container.
fn leaf_of(content: &str) -> Option<info::Info> {
    let rest = content.strip_prefix("::")?;
    if rest.starts_with(':') {
        return None;
    }
    let info = info::parse(rest);
    if info.name.is_empty() {
        return None;
    }
    // The whole line is the directive, or it is prose that happens to start
    // with two colons.
    let tail = &rest[info.name_at.end as usize..];
    if !tail.trim_end().is_empty() && !tail.trim_start().starts_with('{') {
        return None;
    }
    Some(info)
}

fn prop_spans(info: &info::Info, source: SourceId, base: u32) -> Vec<(String, Span)> {
    info.props
        .0
        .keys()
        .filter_map(|name| {
            let at = info.span_of(name)?;
            Some((
                name.clone(),
                Span::new(source, base + at.start, base + at.end),
            ))
        })
        .collect()
}

fn escape(content: &str, source: SourceId, base: u32, diagnostics: &mut Diagnostics) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    let mut at = 0u32;
    while let Some(found) = rest.find(MARKER_PREFIX) {
        let seen = base + at + found as u32;
        diagnostics.push(
            Diagnostic::new(code::W0319, "literal directive marker prefix; escaped").at(Span::new(
                source,
                seen,
                seen + MARKER_PREFIX.len() as u32,
            )),
        );
        out.push_str(&rest[..found]);
        out.push_str(ESCAPED_PREFIX);
        let used = found + MARKER_PREFIX.len();
        at += used as u32;
        rest = &rest[used..];
    }
    out.push_str(rest);
    out
}

/// Appends one line and keeps the map in step.
///
/// An entry's delta is the one accumulated by the lines *before* it, so a
/// line's own change in length applies from the next entry on. An entry is
/// needed only where the cumulative delta changes, plus one at offset zero:
/// `RewriteMap::to_expanded` reads the last entry at or before an offset, and
/// an offset before the first entry has nothing to read.
fn emit(out: &mut String, map: &mut Vec<(u32, i32)>, delta: &mut i32, raw: &str, rewritten: &str) {
    if map.last().map(|(_, seen)| *seen) != Some(*delta) {
        map.push((out.len() as u32, *delta));
    }
    *delta += rewritten.len() as i32 - raw.len() as i32;
    out.push_str(rewritten);
}

/// Expanded offsets are not a source, but `Span` carries one; every run in the
/// span map agrees on it, so the first is as good as any.
fn source_of(expanded: &Expanded) -> SourceId {
    expanded
        .map
        .0
        .iter()
        .find_map(|(_, _, origin)| origin.span.map(|span| span.source))
        .unwrap_or(SourceId(0))
}

/// The props of a leaf, for callers that only want the table row.
pub fn props_of(content: &str) -> Option<(String, Props)> {
    leaf_of(content).map(|info| (info.name, info.props))
}

#[cfg(test)]
mod tests;
