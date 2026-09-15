//! The Source Document scanner (PRD §7.16; CM-01, CM-11, CM-22, CM-23).
//!
//! Segmentation is lossless by construction rather than by care: the passes
//! below produce only the spans of the segments that are *not* Markdown, and
//! the gaps between them are filled with `Markdown` segments at the end. A
//! segment can therefore never be dropped or double-counted, which is what the
//! tiling property test asserts.
//!
//! Three passes, in this order:
//!
//! 1. front matter, which decides where the body starts;
//! 2. blocks — fenced and indented code, and directive lines — because a
//!    template tag inside a code block is not a template tag (CM-11);
//! 3. template tags in what is left, skipping inline code spans (CM-23).
//!
//! `plan/rfcs/0020-source-document-tiling.md` records what `segments` tiles
//! and what counts as front matter.

use std::ops::Range;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{FenceInfo, PropValue, Props, Segment, SourceDocument, TemplateKind};
use liyasa_core::frontmatter::{Frontmatter, FrontmatterFields};
use liyasa_core::span::{SourceId, Span};

use super::lines::{self, Line, ListStack};
use super::mask;
use super::yaml;

/// The delimiter of a front matter block.
const FENCE_YAML: &str = "---";

/// How many private-use characters are reported before the rest are assumed to
/// be the same mistake repeated.
const MAX_PRIVATE_USE_REPORTS: usize = 16;

/// Segments a page into its lossless Source Document form.
///
/// `source` is expected to be normalized already (see
/// [`normalize`](super::normalize)); spans are byte offsets into it.
pub fn scan(source: &str, id: SourceId) -> (SourceDocument, Diagnostics) {
    let mut diagnostics = Diagnostics::new();
    reject_private_use(source, id, &mut diagnostics);

    let (frontmatter, body_at) = scan_frontmatter(source, id, &mut diagnostics);
    let mut cuts = Vec::new();
    scan_blocks(source, body_at, id, &mut cuts, &mut diagnostics);
    cuts.sort_by_key(|cut| cut.start);
    scan_templates(source, body_at, id, &mut cuts, &mut diagnostics);
    cuts.sort_by_key(|cut| cut.start);

    let mut segments = Vec::with_capacity(cuts.len() * 2 + 1);
    let mut at = body_at;
    for cut in cuts {
        if cut.start > at {
            segments.push(Segment::Markdown {
                span: Span::new(id, at as u32, cut.start as u32),
            });
        }
        at = cut.end;
        segments.push(cut.segment);
    }
    if at < source.len() {
        segments.push(Segment::Markdown {
            span: Span::new(id, at as u32, source.len() as u32),
        });
    }

    match_statements(source, &mut segments, &mut diagnostics);
    match_directives(source, &mut segments, &mut diagnostics);

    let document = SourceDocument {
        source: id,
        frontmatter,
        segments,
    };
    super::wellformed::check(source, &document, &mut diagnostics);
    (document, diagnostics)
}

/// The byte offset where the body begins: past the front matter, or 0.
pub fn body_start(document: &SourceDocument) -> u32 {
    document
        .frontmatter
        .as_ref()
        .map_or(0, |front| front.span.end)
}

struct Cut {
    start: usize,
    end: usize,
    segment: Segment,
}

// ---- private-use characters (CM-22) ----

/// The expansion sentinels live in the private use area, so content may not.
/// Rejecting them here is what makes a sentinel unforgeable: there is no
/// legitimate private-use character in documentation.
fn reject_private_use(source: &str, id: SourceId, diagnostics: &mut Diagnostics) {
    let mut reported = 0usize;
    for (at, ch) in source.char_indices() {
        if !is_private_use(ch) {
            continue;
        }
        reported += 1;
        if reported > MAX_PRIVATE_USE_REPORTS {
            break;
        }
        let start = at as u32;
        diagnostics.push(
            Diagnostic::new(
                code::E0212,
                format!("private-use character U+{:04X} in content", ch as u32),
            )
            .at(Span::new(id, start, start + ch.len_utf8() as u32))
            .help("expansion reserves the private use area for its sentinels"),
        );
    }
}

pub fn is_private_use(ch: char) -> bool {
    matches!(ch as u32, 0xE000..=0xF8FF | 0xF_0000..=0xF_FFFD | 0x10_0000..=0x10_FFFD)
}

// ---- front matter (CM-01, CM-24) ----

fn scan_frontmatter(
    source: &str,
    id: SourceId,
    diagnostics: &mut Diagnostics,
) -> (Option<Frontmatter>, usize) {
    let Some(rest) = source.strip_prefix(FENCE_YAML) else {
        return (None, 0);
    };
    let Some(rest) = rest.strip_prefix('\n') else {
        return (None, 0);
    };
    let open_len = FENCE_YAML.len() + 1;

    // The close may not be the line straight after the open: `---\n---` is two
    // thematic breaks, which is what the parser does with it too (RFC 0020).
    let mut body_len = 0usize;
    let mut close_len = None;
    for line in rest.split_inclusive('\n') {
        if body_len > 0 && line.trim_end() == FENCE_YAML {
            close_len = Some(line.len());
            break;
        }
        body_len += line.len();
    }
    let Some(close_len) = close_len else {
        return (None, 0);
    };

    let text = &rest[..body_len];
    let block_end = open_len + body_len + close_len;
    let span = Span::new(id, 0, block_end as u32);
    let text_span = Span::new(id, open_len as u32, (open_len + body_len) as u32);

    let parsed = if text.trim().is_empty() {
        Ok((serde_json::Value::Null, FrontmatterFields::default()))
    } else {
        yaml::parse_typed::<FrontmatterFields>(text, Some(text_span))
    };
    match parsed {
        Ok((value, typed)) => (Some(Frontmatter { span, value, typed }), block_end),
        Err(diagnostic) => {
            diagnostics.push(*diagnostic);
            (None, block_end)
        }
    }
}

// ---- blocks: code fences, indented code, directives (CM-11) ----

struct Fence {
    ch: u8,
    len: usize,
    start: usize,
    body_at: usize,
    info: FenceInfo,
}

fn scan_blocks(
    source: &str,
    body_at: usize,
    id: SourceId,
    cuts: &mut Vec<Cut>,
    diagnostics: &mut Diagnostics,
) {
    let mut fence: Option<Fence> = None;
    let mut indented: Option<(usize, usize)> = None;
    let mut list = ListStack::default();
    let mut raw = 0usize;
    let mut blank_before = true;

    let code = |cuts: &mut Vec<Cut>, start: usize, end: usize, body: Range<usize>, info| {
        cuts.push(Cut {
            start,
            end,
            segment: Segment::Code {
                span: Span::new(id, start as u32, end as u32),
                info,
                body: Span::new(id, body.start as u32, body.end as u32),
            },
        });
    };

    for line in lines::split(source, body_at) {
        if let Some(open) = &fence {
            if closes_fence(&line, open, list.content_column()) {
                let body = open.body_at..line.start;
                code(cuts, open.start, line.next, body, open.info.clone());
                fence = None;
            }
            continue;
        }

        if let Some((start, end)) = indented {
            if line.is_blank() {
                indented = Some((start, end));
                continue;
            }
            if line.indent >= list.content_column() + lines::TAB_STOP {
                indented = Some((start, line.next));
                continue;
            }
            code(cuts, start, end, start..end, FenceInfo::default());
            indented = None;
        }

        if line.is_blank() {
            blank_before = true;
            continue;
        }
        list.feed(&line);
        raw = raw_depth_after(&line, raw);

        if let Some(open) = opens_fence(&line, list.content_column()) {
            fence = Some(open);
            blank_before = false;
            continue;
        }
        if blank_before && line.indent >= list.content_column() + lines::TAB_STOP {
            indented = Some((line.start, line.next));
            blank_before = false;
            continue;
        }
        blank_before = false;

        if raw == 0
            && let Some(cut) = directive_cut(&line, id, diagnostics)
        {
            cuts.push(cut);
        }
    }

    if let Some(open) = fence {
        diagnostics.push(
            Diagnostic::new(code::E0301, "code fence is never closed").at(Span::new(
                id,
                open.start as u32,
                (open.start + open.len) as u32,
            )),
        );
        code(
            cuts,
            open.start,
            source.len(),
            open.body_at..source.len(),
            open.info,
        );
    } else if let Some((start, end)) = indented {
        code(cuts, start, end, start..end, FenceInfo::default());
    }
}

fn opens_fence(line: &Line<'_>, list_column: usize) -> Option<Fence> {
    if line.indent > list_column + 3 {
        return None;
    }
    let bytes = line.content.as_bytes();
    let ch = *bytes.first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let len = bytes.iter().take_while(|b| **b == ch).count();
    if len < 3 {
        return None;
    }
    let rest = &line.content[len..];
    // A backtick fence's info string may not contain a backtick: that is how
    // CommonMark keeps ``` `a` ``` from opening one.
    if ch == b'`' && rest.contains('`') {
        return None;
    }
    Some(Fence {
        ch,
        len,
        start: line.start,
        body_at: line.next,
        info: fence_info(rest),
    })
}

fn closes_fence(line: &Line<'_>, open: &Fence, list_column: usize) -> bool {
    if line.indent > list_column + 3 {
        return false;
    }
    let run = line
        .content
        .as_bytes()
        .iter()
        .take_while(|b| **b == open.ch)
        .count();
    run >= open.len && line.content[run..].trim().is_empty()
}

/// `bash template title="x" {1,3-5}`.
fn fence_info(rest: &str) -> FenceInfo {
    let mut info = FenceInfo::default();
    for token in tokens(rest) {
        if let Some(ranges) = token.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
            info.attrs.highlight.extend(highlight(ranges));
        } else if let Some((key, value)) = token.split_once('=') {
            info.attrs
                .kv
                .insert(key.to_owned(), value.trim_matches('"').to_owned());
        } else if info.lang.is_none() && token != "template" {
            info.lang = Some(token.to_owned());
        } else {
            info.attrs.flags.insert(token.to_owned());
        }
    }
    info
}

/// Whitespace-separated, except inside `"…"` or `{…}`.
fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut braces = 0usize;
    for ch in text.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                current.push(ch);
            }
            '{' => {
                braces += 1;
                current.push(ch);
            }
            '}' => {
                braces = braces.saturating_sub(1);
                current.push(ch);
            }
            _ if ch.is_whitespace() && !quoted && braces == 0 => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn highlight(ranges: &str) -> Vec<(u32, u32)> {
    ranges
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            match part.split_once('-') {
                Some((first, last)) => {
                    Some((first.trim().parse().ok()?, last.trim().parse().ok()?))
                }
                None => {
                    let only = part.parse().ok()?;
                    Some((only, only))
                }
            }
        })
        .collect()
}

// ---- directives ----

fn directive_cut(line: &Line<'_>, id: SourceId, diagnostics: &mut Diagnostics) -> Option<Cut> {
    let content = line.content;
    let colons = content.bytes().take_while(|b| *b == b':').count();
    if colons < 2 {
        return None;
    }
    let span = Span::new(id, line.start as u32, line.next as u32);
    let rest = &content[colons..];
    if colons >= 3 && rest.trim().is_empty() {
        return Some(Cut {
            start: line.start,
            end: line.next,
            segment: Segment::DirectiveClose { span },
        });
    }

    let name_len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(rest.len());
    if name_len == 0 {
        return None;
    }
    let name = rest[..name_len].to_owned();
    let tail = rest[name_len..].trim();
    let Some(props) = parse_props(tail) else {
        diagnostics.push(
            Diagnostic::new(code::E0312, "directive properties are not `{key=value …}`").at(span),
        );
        return None;
    };

    let segment = if colons >= 3 {
        Segment::DirectiveOpen {
            span,
            name,
            props,
            colons: colons.min(u8::MAX as usize) as u8,
            matching: None,
        }
    } else {
        Segment::DirectiveLeaf { span, name, props }
    };
    Some(Cut {
        start: line.start,
        end: line.next,
        segment,
    })
}

/// `{key="value" key=1 key=true .class #id}`; anything else is not a directive.
fn parse_props(tail: &str) -> Option<Props> {
    if tail.is_empty() {
        return Some(Props::default());
    }
    let body = tail.strip_prefix('{')?.strip_suffix('}')?;
    let mut out = Props::default();
    let mut rest = body;
    while !rest.trim().is_empty() {
        rest = rest.trim_start();
        let (name, value, used) = if let Some(after) = rest.strip_prefix('.') {
            let len = token_len(after);
            (
                "class".to_owned(),
                PropValue::Str(after[..len].to_owned()),
                len + 1,
            )
        } else if let Some(after) = rest.strip_prefix('#') {
            let len = token_len(after);
            (
                "id".to_owned(),
                PropValue::Str(after[..len].to_owned()),
                len + 1,
            )
        } else {
            let key_len = rest.find('=')?;
            let key = rest[..key_len].trim().to_owned();
            let (value, len) = parse_value(&rest[key_len + 1..])?;
            (key, value, key_len + 1 + len)
        };
        out.0.insert(name, value);
        rest = rest.get(used..)?;
    }
    Some(out)
}

fn parse_value(text: &str) -> Option<(PropValue, usize)> {
    if let Some(after) = text.strip_prefix('"') {
        let end = after.find('"')?;
        return Some((PropValue::Str(after[..end].to_owned()), end + 2));
    }
    if let Some(after) = text.strip_prefix("{{") {
        let end = after.find("}}")?;
        return Some((PropValue::Expr(after[..end].trim().to_owned()), end + 4));
    }
    if let Some(after) = text.strip_prefix('[') {
        let end = after.find(']')?;
        let items = after[..end]
            .split(',')
            .map(|item| PropValue::Str(item.trim().trim_matches('"').to_owned()))
            .collect();
        return Some((PropValue::List(items), end + 2));
    }
    let len = text
        .find(|c: char| c.is_whitespace() || c == '}')
        .unwrap_or(text.len());
    let token = &text[..len];
    let value = match token {
        "true" => PropValue::Bool(true),
        "false" => PropValue::Bool(false),
        _ => match token.parse::<f64>() {
            Ok(number) => PropValue::Num(number),
            Err(_) => PropValue::Str(token.to_owned()),
        },
    };
    Some((value, len))
}

fn token_len(text: &str) -> usize {
    text.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(text.len())
}

// ---- template tags (CM-10, CM-11, CM-23) ----

fn scan_templates(
    source: &str,
    body_at: usize,
    id: SourceId,
    cuts: &mut Vec<Cut>,
    diagnostics: &mut Diagnostics,
) {
    let mut raw = false;
    let mut found = Vec::new();
    let mut at = body_at;
    let taken: Vec<Range<usize>> = cuts.iter().map(|cut| cut.start..cut.end).collect();
    for taken in &taken {
        if taken.start > at {
            scan_region(
                source,
                at..taken.start,
                id,
                &mut raw,
                &mut found,
                diagnostics,
            );
        }
        at = at.max(taken.end);
    }
    if at < source.len() {
        scan_region(
            source,
            at..source.len(),
            id,
            &mut raw,
            &mut found,
            diagnostics,
        );
    }
    cuts.extend(found);
}

fn scan_region(
    source: &str,
    region: Range<usize>,
    id: SourceId,
    raw: &mut bool,
    cuts: &mut Vec<Cut>,
    diagnostics: &mut Diagnostics,
) {
    let masked = masked_ranges(source, region.clone());
    let mut at = region.start;
    while let Some(found) = source[at..region.end].find('{') {
        let open = at + found;
        at = open + 1;
        if mask::is_masked(&masked, open) {
            continue;
        }
        let Some(kind) = opener(&source[open..region.end]) else {
            continue;
        };
        let Some(tag) = close_of(&source[open..region.end], kind) else {
            diagnostics.push(
                Diagnostic::new(code::E0202, format!("`{}` is never closed", kind.open()))
                    .at(Span::new(id, open as u32, region.end as u32)),
            );
            break;
        };
        let end = open + tag;
        let name = statement_name(&source[open..end]);
        if *raw {
            if kind != Kind::Statement || name.as_deref() != Some("endraw") {
                continue;
            }
            *raw = false;
        } else if kind == Kind::Statement && name.as_deref() == Some("raw") {
            *raw = true;
        }
        cuts.push(Cut {
            start: open,
            end,
            segment: Segment::Template {
                span: Span::new(id, open as u32, end as u32),
                kind: match kind {
                    Kind::Output => TemplateKind::Output,
                    Kind::Comment => TemplateKind::Comment,
                    Kind::Statement => TemplateKind::Statement {
                        name: name.unwrap_or_default(),
                        matching: None,
                    },
                },
            },
        });
        at = end;
    }
}

/// Inline code spans of every block in a region, as offsets into `source`.
///
/// A span may wrap across a soft line break but not out of its block, so the
/// region is cut into blocks first: at blank lines, at list-item markers, and
/// at ATX headings (CM-23).
pub(crate) fn masked_ranges(source: &str, region: Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut block: Option<Range<usize>> = None;
    let flush = |block: &mut Option<Range<usize>>, out: &mut Vec<Range<usize>>| {
        if let Some(range) = block.take() {
            out.extend(
                mask::code_spans(&source[range.clone()])
                    .into_iter()
                    .map(|span| range.start + span.start..range.start + span.end),
            );
        }
    };

    for line in lines::split(source, region.start) {
        if line.start >= region.end {
            break;
        }
        let end = line.next.min(region.end);
        if line.is_blank() || starts_a_block(line.content) {
            flush(&mut block, &mut out);
        }
        if line.is_blank() {
            continue;
        }
        block = Some(match block.take() {
            Some(range) => range.start..end,
            None => line.content_at..end,
        });
        if line.content.starts_with('#') {
            flush(&mut block, &mut out);
        }
    }
    flush(&mut block, &mut out);
    out.sort_by_key(|range| range.start);
    out
}

/// Whether a line begins a new Markdown block, so an inline code span may not
/// continue across it.
fn starts_a_block(content: &str) -> bool {
    let bytes = content.as_bytes();
    match bytes.first() {
        Some(b'#' | b'>') => true,
        Some(b'-' | b'+' | b'*') => bytes.get(1).is_none_or(u8::is_ascii_whitespace),
        Some(b'0'..=b'9') => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            matches!(bytes.get(digits), Some(b'.' | b')'))
        }
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Output,
    Statement,
    Comment,
}

impl Kind {
    const fn open(self) -> &'static str {
        match self {
            Self::Output => "{{",
            Self::Statement => "{%",
            Self::Comment => "{#",
        }
    }

    const fn close(self) -> &'static str {
        match self {
            Self::Output => "}}",
            Self::Statement => "%}",
            Self::Comment => "#}",
        }
    }
}

fn opener(text: &str) -> Option<Kind> {
    match text.as_bytes().get(1)? {
        b'{' => Some(Kind::Output),
        b'%' => Some(Kind::Statement),
        b'#' => Some(Kind::Comment),
        _ => None,
    }
}

/// The offset just past the tag's closing delimiter, skipping over quoted
/// strings so that `{{ "100%}" }}` is one tag and not two.
fn close_of(text: &str, kind: Kind) -> Option<usize> {
    let close = kind.close();
    let bytes = text.as_bytes();
    let mut at = 2usize;
    while at < bytes.len() {
        match bytes[at] {
            b'"' | b'\'' if kind != Kind::Comment => {
                let quote = bytes[at];
                at += 1;
                while at < bytes.len() && bytes[at] != quote {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
            }
            _ if text[at..].starts_with(close) => return Some(at + close.len()),
            _ => {}
        }
        at += 1;
    }
    None
}

fn statement_name(tag: &str) -> Option<String> {
    let rest = tag.get(2..)?.trim_start_matches('-').trim_start();
    let len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (len > 0).then(|| rest[..len].to_owned())
}

/// Tracks `{% raw %}` across the block pass, which runs before tags are cut:
/// a `:::note` inside a raw block is text, not a directive.
fn raw_depth_after(line: &Line<'_>, depth: usize) -> usize {
    let mut depth = depth;
    let mut at = 0usize;
    while let Some(found) = line.content[at..].find("{%") {
        let open = at + found;
        at = open + 2;
        match statement_name(&line.content[open..]).as_deref() {
            Some("raw") => depth += 1,
            Some("endraw") => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

// ---- block statement pairing (part of CM-21) ----

/// Statements that open a block, with the tag that closes each.
fn opens_block(name: &str, tag: &str) -> bool {
    match name {
        "for" | "if" | "macro" | "call" | "filter" | "with" | "raw" | "autoescape" | "block" => {
            true
        }
        // `{% set x = 1 %}` is a statement; `{% set x %}…{% endset %}` is a block.
        "set" => !tag.contains('='),
        _ => false,
    }
}

fn match_statements(source: &str, segments: &mut [Segment], diagnostics: &mut Diagnostics) {
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut pairs: Vec<(usize, usize)> = Vec::new();

    for at in 0..segments.len() {
        let Segment::Template { span, kind } = &segments[at] else {
            continue;
        };
        let TemplateKind::Statement { name, .. } = kind else {
            continue;
        };
        let tag = &source[span.start as usize..span.end as usize];
        if let Some(opened) = name.strip_prefix("end") {
            match stack.pop() {
                Some((open_at, open_name)) if open_name == opened => pairs.push((open_at, at)),
                Some((open_at, open_name)) => {
                    let open_span = segments[open_at].span();
                    diagnostics.push(
                        Diagnostic::new(
                            code::E0202,
                            format!("`{{% {name} %}}` closes `{{% {open_name} %}}`"),
                        )
                        .at(*span)
                        .label(open_span, format!("`{open_name}` opened here")),
                    );
                }
                None => diagnostics.push(
                    Diagnostic::new(code::E0202, format!("`{{% {name} %}}` closes nothing"))
                        .at(*span),
                ),
            }
            continue;
        }
        // CM-13: template inheritance belongs to theme templates, not pages.
        if matches!(name.as_str(), "extends" | "block") {
            diagnostics.push(
                Diagnostic::new(
                    code::E0202,
                    format!("`{{% {name} %}}` is not available inside a page"),
                )
                .at(*span)
                .help("template inheritance is reserved for theme templates"),
            );
        }
        if opens_block(name, tag) {
            stack.push((at, name.clone()));
        }
    }

    for (at, name) in stack {
        diagnostics.push(
            Diagnostic::new(code::E0202, format!("`{{% {name} %}}` is never closed"))
                .at(segments[at].span()),
        );
    }
    for (open_at, close_at) in pairs {
        if let Segment::Template {
            kind: TemplateKind::Statement { matching, .. },
            ..
        } = &mut segments[open_at]
        {
            *matching = Some(close_at);
        }
    }
}

// ---- directive pairing ----

/// Pairs `:::name` with the `:::` that closes it, by colon count and by the
/// container the two lines sit in.
pub fn match_directives(source: &str, segments: &mut [Segment], diagnostics: &mut Diagnostics) {
    let mut stack: Vec<(usize, u8, usize)> = Vec::new();
    let mut pairs = Vec::new();
    for at in 0..segments.len() {
        let depth = quote_depth(source, segments[at].span());
        match &segments[at] {
            Segment::DirectiveOpen { colons, .. } => stack.push((at, *colons, depth)),
            Segment::DirectiveClose { span } => {
                let colons = source[span.start as usize..span.end as usize]
                    .trim_start()
                    .bytes()
                    .take_while(|b| *b == b':')
                    .count() as u8;
                match stack
                    .iter()
                    .rposition(|(_, open, quotes)| *open == colons && *quotes == depth)
                {
                    Some(found) => {
                        for (unclosed, _, _) in stack.drain(found + 1..) {
                            diagnostics.push(
                                Diagnostic::new(code::E0310, "container directive is never closed")
                                    .at(segments[unclosed].span())
                                    .label(*span, "a shallower directive closed here"),
                            );
                        }
                        if let Some((open_at, _, _)) = stack.pop() {
                            pairs.push((open_at, at));
                        }
                    }
                    None => diagnostics.push(
                        Diagnostic::new(code::E0311, "directive close without a matching open")
                            .at(*span),
                    ),
                }
            }
            _ => {}
        }
    }
    for (at, _, _) in stack {
        diagnostics.push(
            Diagnostic::new(code::E0310, "container directive is never closed")
                .at(segments[at].span()),
        );
    }
    for (open_at, close_at) in pairs {
        if let Segment::DirectiveOpen { matching, .. } = &mut segments[open_at] {
            *matching = Some(close_at);
        }
    }
}

fn quote_depth(source: &str, span: Span) -> usize {
    source[span.start as usize..]
        .bytes()
        .take_while(|b| matches!(b, b' ' | b'\t' | b'>'))
        .filter(|b| *b == b'>')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: SourceId = SourceId(0);

    fn document(text: &str) -> (SourceDocument, Diagnostics) {
        scan(text, ID)
    }

    fn codes(diagnostics: &Diagnostics) -> Vec<&'static str> {
        diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    fn at(text: &str, span: Span) -> &str {
        &text[span.start as usize..span.end as usize]
    }

    /// The invariant of §7.16: the front matter span and then the segment
    /// spans reproduce the file byte for byte.
    fn assert_tiles(text: &str) {
        let (document, _) = document(text);
        let mut rebuilt = String::new();
        if let Some(front) = &document.frontmatter {
            assert_eq!(front.span.start, 0);
            rebuilt.push_str(at(text, front.span));
        }
        let mut previous = rebuilt.len() as u32;
        for segment in &document.segments {
            let span = segment.span();
            assert_eq!(
                span.start, previous,
                "segment {segment:?} leaves a hole in {text:?}"
            );
            rebuilt.push_str(at(text, span));
            previous = span.end;
        }
        assert_eq!(rebuilt, text, "segments do not tile {text:?}");
    }

    fn kinds(document: &SourceDocument) -> Vec<&'static str> {
        document
            .segments
            .iter()
            .map(|segment| match segment {
                Segment::Markdown { .. } => "markdown",
                Segment::Code { .. } => "code",
                Segment::Template { .. } => "template",
                Segment::DirectiveOpen { .. } => "open",
                Segment::DirectiveClose { .. } => "close",
                Segment::DirectiveLeaf { .. } => "leaf",
            })
            .collect()
    }

    // ---- front matter (CM-01) ----

    #[test]
    fn front_matter_is_parsed_and_the_body_follows() {
        let text = "---\ntitle: Install\n---\n\nbody\n";
        let (document, diagnostics) = document(text);
        let front = document.frontmatter.as_ref().expect("front matter");
        assert_eq!(at(text, front.span), "---\ntitle: Install\n---\n");
        assert_eq!(front.typed.title.as_deref(), Some("Install"));
        assert_eq!(front.value["title"], "Install");
        assert_eq!(kinds(&document), ["markdown"]);
        assert_eq!(at(text, document.segments[0].span()), "\nbody\n");
        assert!(diagnostics.is_empty());
        assert_tiles(text);
    }

    #[test]
    fn unknown_front_matter_keys_stay_in_the_value() {
        let text = "---\ntitle: A\nhouse_style: loud\n---\nbody\n";
        let (document, diagnostics) = document(text);
        let front = document.frontmatter.as_ref().expect("front matter");
        assert_eq!(front.value["house_style"], "loud");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn an_empty_block_is_not_front_matter() {
        // RFC 0020: `---\n---` is two thematic breaks, as the parser reads it.
        let text = "---\n---\n\nbody\n";
        let (document, _) = document(text);
        assert!(document.frontmatter.is_none());
        assert_tiles(text);
    }

    #[test]
    fn front_matter_must_start_the_file() {
        let text = "\n---\ntitle: A\n---\n";
        assert!(document(text).0.frontmatter.is_none());
        assert_tiles(text);
    }

    #[test]
    fn dashes_in_the_body_are_not_a_second_block() {
        let text = "---\ntitle: A\n---\n\nbody\n\n---\n\nmore\n";
        let (document, _) = document(text);
        let front = document.frontmatter.as_ref().expect("front matter");
        assert_eq!(at(text, front.span), "---\ntitle: A\n---\n");
        assert_tiles(text);
    }

    #[test]
    fn invalid_front_matter_is_reported_and_the_body_still_scans() {
        let text = "---\ntitle: [unclosed\n---\n\nbody\n";
        let (document, diagnostics) = document(text);
        assert!(document.frontmatter.is_none());
        assert_eq!(codes(&diagnostics), ["E0101"]);
        assert_eq!(at(text, document.segments[0].span()), "\nbody\n");
    }

    #[test]
    fn an_alias_bomb_in_front_matter_is_refused() {
        let text = "---\na: &a [x, x]\nb: [*a, *a]\n---\nbody\n";
        assert_eq!(codes(&document(text).1), ["E0102"]);
    }

    #[test]
    fn a_page_with_no_front_matter_is_all_body() {
        let text = "# Title\n\nbody\n";
        let (document, _) = document(text);
        assert!(document.frontmatter.is_none());
        assert_eq!(kinds(&document), ["markdown"]);
        assert_tiles(text);
    }

    // ---- fences (CM-11) ----

    #[test]
    fn a_fence_is_one_segment_and_is_not_templated() {
        let text = "before\n\n```bash\necho {{ x }}\n```\n\nafter\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["markdown", "code", "markdown"]);
        let Segment::Code { span, info, body } = &document.segments[1] else {
            panic!("expected a code segment");
        };
        assert_eq!(at(text, *span), "```bash\necho {{ x }}\n```\n");
        assert_eq!(at(text, *body), "echo {{ x }}\n");
        assert_eq!(info.lang.as_deref(), Some("bash"));
        assert!(info.attrs.flags.is_empty());
        assert_tiles(text);
    }

    #[test]
    fn a_fence_opts_into_templating_with_an_attribute() {
        let text = "```bash template\necho {{ x }}\n```\n";
        let (document, _) = document(text);
        let Segment::Code { info, .. } = &document.segments[0] else {
            panic!("expected a code segment");
        };
        assert_eq!(info.lang.as_deref(), Some("bash"));
        assert!(info.attrs.flags.contains("template"));
    }

    #[test]
    fn a_fence_with_only_the_attribute_has_no_language() {
        let text = "``` template\n{{ x }}\n```\n";
        let (document, _) = document(text);
        let Segment::Code { info, .. } = &document.segments[0] else {
            panic!("expected a code segment");
        };
        assert_eq!(info.lang, None);
        assert!(info.attrs.flags.contains("template"));
    }

    #[test]
    fn fence_attributes_are_parsed() {
        let text = "```rust title=\"main rs\" {1,3-5}\ncode\n```\n";
        let (document, _) = document(text);
        let Segment::Code { info, .. } = &document.segments[0] else {
            panic!("expected a code segment");
        };
        assert_eq!(
            info.attrs.kv.get("title").map(String::as_str),
            Some("main rs")
        );
        assert_eq!(info.attrs.highlight, [(1, 1), (3, 5)]);
    }

    #[test]
    fn a_tilde_fence_holds_a_backtick_fence() {
        let text = "~~~\n```\n{{ x }}\n```\n~~~\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["code"]);
        assert_tiles(text);
    }

    #[test]
    fn a_longer_run_is_needed_to_close_a_long_fence() {
        let text = "````\n```\n{{ x }}\n```\n````\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["code"]);
    }

    #[test]
    fn an_unclosed_fence_runs_to_the_end_of_the_file() {
        let text = "```\ncode\n";
        let (document, diagnostics) = document(text);
        assert_eq!(codes(&diagnostics), ["E0301"]);
        assert_eq!(kinds(&document), ["code"]);
        assert_tiles(text);
    }

    #[test]
    fn a_backtick_pair_on_one_line_is_not_a_fence() {
        let text = "the ```x``` span\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["markdown"]);
    }

    // ---- indented code ----

    #[test]
    fn indented_code_is_masked() {
        let text = "para\n\n    {{ x }}\n\nafter\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["markdown", "code", "markdown"]);
        assert_eq!(at(text, document.segments[1].span()), "    {{ x }}\n");
        assert_tiles(text);
    }

    #[test]
    fn a_list_item_continuation_is_not_indented_code() {
        // The list item's content column is 2, so code starts at column 6.
        let text = "- item\n\n    {{ x }}\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["markdown", "template", "markdown"]);
    }

    #[test]
    fn indented_code_inside_a_list_item_starts_at_its_own_column() {
        let text = "- item\n\n      {{ x }}\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["markdown", "code"]);
    }

    #[test]
    fn indentation_cannot_interrupt_a_paragraph() {
        let text = "para\n    {{ x }}\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["markdown", "template", "markdown"]);
    }

    // ---- inline code spans (CM-23) ----

    #[test]
    fn a_code_span_makes_template_syntax_inert() {
        let text = "write `{{ x }}` to print\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["markdown"]);
        assert_tiles(text);
    }

    #[test]
    fn a_code_span_crossing_a_soft_break_still_masks() {
        let text = "write `{{ x }}\nand {% for %}` in prose\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["markdown"]);
    }

    #[test]
    fn a_span_does_not_cross_a_blank_line() {
        let text = "a ` b\n\nc {{ x }} ` d\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["markdown", "template", "markdown"]);
    }

    #[test]
    fn a_span_does_not_cross_a_list_item_boundary() {
        let text = "- a `\n- b {{ x }} `\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["markdown", "template", "markdown"]);
    }

    #[test]
    fn a_double_run_masks_a_single_backtick() {
        let text = "``{{ x }} ` y`` tail\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["markdown"]);
    }

    #[test]
    fn an_unmatched_run_leaves_the_tag_live() {
        let text = "a ` b {{ x }} c\n";
        let (document, _) = document(text);
        assert_eq!(kinds(&document), ["markdown", "template", "markdown"]);
    }

    // ---- template tags (CM-10) ----

    #[test]
    fn the_three_tag_forms_are_each_a_segment() {
        let text = "{{ x }} {% if y %}z{% endif %} {# c #}\n";
        let (document, _) = document(text);
        assert_eq!(
            kinds(&document),
            [
                "template", "markdown", "template", "markdown", "template", "markdown", "template",
                "markdown"
            ]
        );
        assert_tiles(text);
    }

    #[test]
    fn a_statement_records_its_name_and_its_closer() {
        let text = "{% for row in rows %}\n- {{ row }}\n{% endfor %}\n";
        let (document, _) = document(text);
        let Segment::Template {
            kind: TemplateKind::Statement { name, matching },
            ..
        } = &document.segments[0]
        else {
            panic!("expected a statement");
        };
        assert_eq!(name, "for");
        let closer = matching.expect("a matching endfor");
        assert_eq!(at(text, document.segments[closer].span()), "{% endfor %}");
        assert_tiles(text);
    }

    #[test]
    fn a_closer_matches_nothing_of_its_own() {
        let text = "{% if a %}x{% endif %}\n";
        let (document, _) = document(text);
        let Segment::Template {
            kind: TemplateKind::Statement { matching, .. },
            ..
        } = &document.segments[2]
        else {
            panic!("expected the endif");
        };
        assert_eq!(*matching, None);
    }

    #[test]
    fn an_inline_set_opens_no_block() {
        let text = "{% set x = 1 %}\n";
        let (_, diagnostics) = document(text);
        assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
    }

    #[test]
    fn a_set_block_opens_one() {
        let text = "{% set x %}body{% endset %}\n";
        let (document, diagnostics) = document(text);
        assert!(diagnostics.is_empty());
        let Segment::Template {
            kind: TemplateKind::Statement { matching, .. },
            ..
        } = &document.segments[0]
        else {
            panic!("expected a statement");
        };
        assert_eq!(*matching, Some(2));
    }

    #[test]
    fn template_inheritance_is_not_a_page_statement() {
        assert_eq!(codes(&document("{% extends \"base.md\" %}\n").1), ["E0202"]);
        assert_eq!(
            codes(&document("{% block main %}x{% endblock %}\n").1),
            ["E0202"]
        );
    }

    #[test]
    fn an_unclosed_statement_is_reported() {
        let text = "{% for row in rows %}\nbody\n";
        let (_, diagnostics) = document(text);
        assert_eq!(codes(&diagnostics), ["E0202"]);
    }

    #[test]
    fn a_mismatched_closer_is_reported() {
        let text = "{% for row in rows %}\n{% endif %}\n";
        let (_, diagnostics) = document(text);
        assert_eq!(codes(&diagnostics), ["E0202"]);
        let reported = diagnostics.iter().next().expect("a diagnostic");
        assert!(reported.message.contains("closes `{% for %}`"));
        assert_eq!(reported.labels.len(), 1);
    }

    #[test]
    fn a_closer_without_an_opener_is_reported() {
        let text = "{% endfor %}\n";
        let (_, diagnostics) = document(text);
        assert_eq!(codes(&diagnostics), ["E0202"]);
    }

    #[test]
    fn an_unclosed_tag_is_reported() {
        let text = "{{ x\n";
        let (_, diagnostics) = document(text);
        assert_eq!(codes(&diagnostics), ["E0202"]);
    }

    #[test]
    fn a_closing_delimiter_inside_a_string_does_not_end_the_tag() {
        let text = "{{ \"100%}\" }} tail\n";
        let (document, diagnostics) = document(text);
        assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
        assert_eq!(at(text, document.segments[0].span()), "{{ \"100%}\" }}");
    }

    #[test]
    fn whitespace_control_stays_inside_the_tag() {
        let text = "a\n{%- if x -%}\nb\n{%- endif -%}\n";
        let (document, _) = document(text);
        assert_eq!(at(text, document.segments[1].span()), "{%- if x -%}");
        assert_tiles(text);
    }

    // ---- raw (CM-11) ----

    #[test]
    fn a_raw_block_disables_templating_inside_it() {
        let text = "{% raw %}\n{{ x }} and {% for %}\n{% endraw %}\n";
        let (document, diagnostics) = document(text);
        assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
        assert_eq!(
            kinds(&document),
            ["template", "markdown", "template", "markdown"]
        );
        assert_tiles(text);
    }

    #[test]
    fn a_directive_inside_a_raw_block_is_text() {
        let text = "{% raw %}\n:::note\nbody\n:::\n{% endraw %}\n";
        let (document, diagnostics) = document(text);
        assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
        assert_eq!(
            kinds(&document),
            ["template", "markdown", "template", "markdown"]
        );
    }

    // ---- directives ----

    #[test]
    fn a_container_directive_pairs_with_its_close() {
        let text = ":::note\nbody\n:::\n";
        let (document, diagnostics) = document(text);
        assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
        assert_eq!(kinds(&document), ["open", "markdown", "close"]);
        let Segment::DirectiveOpen {
            name,
            colons,
            matching,
            ..
        } = &document.segments[0]
        else {
            panic!("expected an open");
        };
        assert_eq!((name.as_str(), *colons, *matching), ("note", 3, Some(2)));
        assert_tiles(text);
    }

    #[test]
    fn directive_props_are_parsed_and_expressions_kept() {
        let text = ":::card{title=\"Set up\" href={{ page.url }} .wide #intro}\nbody\n:::\n";
        let (document, _) = document(text);
        let Segment::DirectiveOpen { props, .. } = &document.segments[0] else {
            panic!("expected an open");
        };
        assert_eq!(
            props.get("title"),
            Some(&PropValue::Str("Set up".to_owned()))
        );
        assert_eq!(
            props.get("href"),
            Some(&PropValue::Expr("page.url".to_owned()))
        );
        assert_eq!(props.get("class"), Some(&PropValue::Str("wide".to_owned())));
        assert_eq!(props.get("id"), Some(&PropValue::Str("intro".to_owned())));
    }

    #[test]
    fn a_leaf_directive_takes_two_colons() {
        let text = "::button{label=\"Go\"}\n";
        let (document, _) = document(text);
        let Segment::DirectiveLeaf { name, props, .. } = &document.segments[0] else {
            panic!("expected a leaf");
        };
        assert_eq!(name, "button");
        assert_eq!(props.get("label"), Some(&PropValue::Str("Go".to_owned())));
    }

    #[test]
    fn nested_containers_pair_by_colon_count() {
        let text = "::::group\n:::note\nbody\n:::\n::::\n";
        let (document, diagnostics) = document(text);
        assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
        let Segment::DirectiveOpen { matching, .. } = &document.segments[0] else {
            panic!("expected an open");
        };
        assert_eq!(*matching, Some(4));
    }

    #[test]
    fn an_unclosed_container_is_reported() {
        let text = ":::note\nbody\n";
        assert_eq!(codes(&document(text).1), ["E0310"]);
    }

    #[test]
    fn a_close_without_an_open_is_reported() {
        let text = "body\n:::\n";
        assert_eq!(codes(&document(text).1), ["E0311"]);
    }

    #[test]
    fn broken_props_are_reported_and_the_line_stays_text() {
        let text = ":::note{title=\n";
        let (document, diagnostics) = document(text);
        assert_eq!(codes(&diagnostics), ["E0312"]);
        assert_eq!(kinds(&document), ["markdown"]);
    }

    #[test]
    fn a_directive_inside_a_fence_is_text() {
        let text = "```\n:::note\n:::\n```\n";
        let (document, diagnostics) = document(text);
        assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
        assert_eq!(kinds(&document), ["code"]);
    }

    // ---- private-use characters (CM-22) ----

    #[test]
    fn a_private_use_character_is_rejected() {
        let text = "body \u{e000} more\n";
        let (_, diagnostics) = document(text);
        assert_eq!(codes(&diagnostics), ["E0212"]);
    }

    #[test]
    fn the_supplementary_private_planes_are_rejected_too() {
        let text = "a \u{f0000} b \u{100000} c\n";
        assert_eq!(codes(&document(text).1), ["E0212", "E0212"]);
    }

    #[test]
    fn ordinary_text_keeps_its_characters() {
        let text = "café — naïve 日本語 🎉\n";
        assert!(document(text).1.is_empty());
        assert_tiles(text);
    }

    // ---- tiling ----

    #[test]
    fn segments_tile_every_shape() {
        for text in [
            "",
            "\n",
            "no newline at the end",
            "---\ntitle: A\n---\n",
            "---\ntitle: A\n---",
            ":::note\n",
            "{{ x }}",
            "{% if x %}",
            "```\n",
            "    code\n",
            "> quoted {{ x }}\n",
            "- item\n  - inner :::note\n",
            "|a|b|\n|-|-|\n|{{ x }}|y|\n",
        ] {
            assert_tiles(text);
        }
    }

    /// A cheap stand-in for the proptest of §7.3.1: deterministic pseudo-random
    /// documents assembled from the fragments that make segmentation hard.
    #[test]
    fn segments_tile_generated_documents() {
        const FRAGMENTS: &[&str] = &[
            "para text\n",
            "\n",
            "{{ value }}\n",
            "{% for row in rows %}\n",
            "{% endfor %}\n",
            "{# note #}\n",
            "```bash\ncode {{ x }}\n```\n",
            "``` template\n{{ x }}\n```\n",
            ":::note\n",
            ":::\n",
            "::button{label=\"Go\"}\n",
            "- item with `{{ code }}`\n",
            "    indented\n",
            "> quoted\n",
            "# Heading `x`\n",
            "{% raw %}\n{{ literal }}\n{% endraw %}\n",
            "a ` unmatched {{ x }}\n",
            "~~~\n:::note\n~~~\n",
        ];
        let mut state = 0x2545_f491_4f6c_dd1du64;
        for _ in 0..500 {
            let mut text = String::new();
            let length = {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state % 9) as usize + 1
            };
            for _ in 0..length {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let at = (state % FRAGMENTS.len() as u64) as usize;
                text.push_str(FRAGMENTS[at]);
            }
            assert_tiles(&text);
        }
    }
}
