//! The one line-based pass that runs before comrak
//! (`plan/rfcs/0003-parser-spike.md`, `plan/rfcs/0026-a-colon-in-an-info-string.md`).
//!
//! It does two things, both of them because comrak cannot.
//!
//! **Leaf directives** `::name{props}`. comrak does not recognize them, and by
//! the time it has parsed the paragraph that holds one the text is no longer
//! recoverable — `::image{src=a_b_c}` has had emphasis applied to `_b_`. So a
//! leaf keeps the marker rewrite of §7.5.1 item 2, on the one line it occupies:
//! a leaf is a single line, it never nests, and the substitution preserves line
//! numbers, so only the column within that one line has to be recovered, through
//! the directive's recorded span rather than by arithmetic on the marker. A
//! literal `<!--ly:` is escaped before anything is written and the nonce is 128
//! bits per build, so a marker cannot be forged.
//!
//! **The block tag form** `<Card …>` … `</Card>`. comrak reads those lines as
//! HTML, and an HTML block runs to the next blank line, so the body between
//! them is raw text rather than the Markdown CM-52 requires. Rewriting the two
//! lines to a directive fence — padded, as above — hands the nesting and the
//! body to comrak's own container algorithm, and everything downstream sees one
//! kind of component rather than two.
//!
//! **Container props.** comrak owns container segmentation, but it will not
//! accept a colon anywhere in an info string, so `:::card{href="https://x"}` is
//! not a directive to it at all — it is a paragraph. Rather than escape the
//! author's own text, the props are taken out of the line entirely: comrak is
//! handed `:::card` padded to the same byte length, and the props are held out
//! of band and matched back by line number. The line length is unchanged, so
//! every position comrak reports is still exact, and there is no marker text on
//! this path for anything to forge.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use liyasa_core::diagnostics::code;
use liyasa_core::document::Props;
use liyasa_core::markdown::{ComponentKind, DirectiveInfo, DirectiveTable, Expanded, RewriteMap};
use liyasa_core::{Diagnostic, Diagnostics, SourceId, Span};

use super::{info, mask, tag};

/// The literal prefix a forged marker would have to reproduce.
pub const MARKER_PREFIX: &str = "<!--ly:";
const ESCAPED_PREFIX: &str = "<!--&#108;y:";

pub struct Rewritten {
    pub text: String,
    pub map: RewriteMap,
    pub table: DirectiveTable,
    /// The table row for each container open, by its 1-based line.
    pub containers: BTreeMap<u32, usize>,
    /// `*[ABBR]: expansion`, by abbreviation.
    pub abbreviations: BTreeMap<String, String>,
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
    let mut containers: BTreeMap<u32, usize> = BTreeMap::new();
    // Tag-form opens still waiting for their close, so `</Card>` with nothing
    // above it stays the raw HTML comrak made of it.
    let mut tags: Vec<String> = Vec::new();
    let mut abbreviations: BTreeMap<String, String> = BTreeMap::new();
    let outermost = tag_fence_length(&expanded.text);
    let mut diagnostics = Diagnostics::new();
    let mut fence = mask::Fences::default();
    let mut line_number = 0u32;
    let mut at = 0usize;
    // Cumulative over the lines *before* an entry: expanded = rewritten - delta.
    let mut delta = 0i32;

    for raw in expanded.text.split_inclusive('\n') {
        line_number += 1;
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

        if let Some((abbr, expansion)) = abbreviation_of(content) {
            abbreviations.insert(abbr, expansion);
            emit(
                &mut out,
                &mut map,
                &mut delta,
                raw,
                &format!("{}{eol}", " ".repeat(line.len())),
            );
            continue;
        }

        if let Some(tag) = tag_line(content) {
            // The outermost fence is the longest: comrak closes a container on
            // the first fence it sees that is at least as long, so a nested one
            // has to be shorter or it closes its parent too.
            let open_fence = format!(
                "{}{TAG_NAME}",
                ":".repeat(outermost.saturating_sub(tags.len()).max(3))
            );
            let closing_at = tags.iter().rposition(|open| *open == tag.name);
            let close_fence = ":".repeat(outermost.saturating_sub(closing_at.unwrap_or(0)).max(3));
            match tag.form {
                tag::Form::Open if open_fence.len() <= content.len() => {
                    let id = table.len();
                    containers.insert(line_number, id);
                    tags.push(tag.name.clone());
                    table.push(DirectiveInfo {
                        name: tag::directive_name(&tag.name),
                        props: tag.props,
                        kind: ComponentKind::Container,
                        span: Span::new(source, start as u32, (start + line.len()) as u32),
                        prop_spans: Vec::new(),
                    });
                    let padding = " ".repeat(content.len() - open_fence.len());
                    emit(
                        &mut out,
                        &mut map,
                        &mut delta,
                        raw,
                        &format!("{lead}{open_fence}{padding}{eol}"),
                    );
                    continue;
                }
                tag::Form::Close if closing_at.is_some() && close_fence.len() <= content.len() => {
                    tags.truncate(closing_at.unwrap_or_default());
                    let padding = " ".repeat(content.len() - close_fence.len());
                    emit(
                        &mut out,
                        &mut map,
                        &mut delta,
                        raw,
                        &format!("{lead}{close_fence}{padding}{eol}"),
                    );
                    continue;
                }
                _ => {}
            }
        }

        if let Some((colons, info)) = container_of(content) {
            let name_at = content_at as u32 + colons as u32;
            diagnostics.extend(info.diagnostics(source, name_at));
            let id = table.len();
            containers.insert(line_number, id);
            let bare = format!("{}{}", ":".repeat(colons), info.name);
            table.push(DirectiveInfo {
                prop_spans: prop_spans(&info, source, name_at),
                name: info.name,
                props: info.props,
                kind: ComponentKind::Container,
                span: Span::new(source, start as u32, (start + line.len()) as u32),
            });
            // Padded to the same byte length, so every column comrak reports
            // on this line is still the column the author wrote.
            let padding = " ".repeat(content.len().saturating_sub(bare.len()));
            emit(
                &mut out,
                &mut map,
                &mut delta,
                raw,
                &format!("{lead}{bare}{padding}{eol}"),
            );
            continue;
        }

        let Some(info) = leaf_of(content) else {
            // Only a line that is not a directive can carry a marker into
            // comrak's input: a directive's props are held out of band, so
            // there is nothing on those lines left to forge.
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
        containers,
        abbreviations,
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

/// The placeholder a rewritten tag open carries. The real name is held out of
/// band with the props, so one character is enough, and it keeps the
/// replacement short enough to fit inside `<Ab>` — the shortest tag anyone
/// writes. A tag too short for its fence is left as the HTML comrak made of it.
const TAG_NAME: &str = "x";

/// `*[ABBR]: expansion` (CM-40).
///
/// The abbreviation may be anything but `]`; the expansion is the rest of the
/// line, and an empty one is not a definition.
fn abbreviation_of(content: &str) -> Option<(String, String)> {
    let rest = content.strip_prefix("*[")?;
    let end = rest.find(']')?;
    let abbr = rest[..end].trim();
    let expansion = rest[end + 1..].strip_prefix(':')?.trim();
    if abbr.is_empty() || expansion.is_empty() {
        return None;
    }
    Some((abbr.to_owned(), expansion.to_owned()))
}

/// The fence length the outermost block tag gets.
///
/// Long enough that the innermost tag still out-fences every directive the
/// author wrote, and one shorter per level on the way in.
fn tag_fence_length(text: &str) -> usize {
    let mut fences = mask::Fences::default();
    let mut stack: Vec<String> = Vec::new();
    let mut depth = 0usize;
    let mut written = 0usize;
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\r', '\n']);
        let split = mask::split(line);
        if fences.step(split.content, split.indent) {
            continue;
        }
        if let Some(tag) = tag_line(split.content) {
            match tag.form {
                tag::Form::Open => {
                    stack.push(tag.name);
                    depth = depth.max(stack.len());
                }
                tag::Form::Close => {
                    if let Some(at) = stack.iter().rposition(|open| *open == tag.name) {
                        stack.truncate(at);
                    }
                }
                tag::Form::SelfClosing => {}
            }
            continue;
        }
        let colons = split.content.chars().take_while(|c| *c == ':').count();
        if colons >= 3 {
            written = written.max(colons);
        }
    }
    (written + 1).max(3) + depth.saturating_sub(1)
}

/// A line that is nothing but one component tag.
fn tag_line(content: &str) -> Option<tag::Tag> {
    if !content.starts_with('<') || !content.trim_end().ends_with('>') {
        return None;
    }
    tag::parse(content.trim_end())
}

/// `:::name{props}`, with props worth taking out of the line.
///
/// A directive with no props is left exactly as it was: comrak reads the name
/// from the info string either way, and a line nobody rewrote is one fewer
/// place for a position to drift.
fn container_of(content: &str) -> Option<(usize, info::Info)> {
    let colons = content.chars().take_while(|c| *c == ':').count();
    if colons < 3 {
        return None;
    }
    let rest = &content[colons..];
    let info = info::parse(rest);
    if info.name.is_empty() {
        return None;
    }
    let tail = rest[info.name_at.end as usize..].trim();
    if tail.is_empty() {
        return None;
    }
    // Anything that is not a prop list is not a directive; comrak agrees, and
    // rewriting it would change a paragraph into one.
    if !tail.starts_with('{') || !tail.ends_with('}') {
        return None;
    }
    Some((colons, info))
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
