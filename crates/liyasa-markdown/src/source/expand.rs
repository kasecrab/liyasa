//! Expansion and the span map (PRD §7.3.1; CM-10 … CM-22).
//!
//! minijinja renders to a string and reports no origins, so Liyasa builds the
//! map itself. The document is handed to minijinja not as its own bytes but as
//! a template assembled from the Source Document's segments with a **sentinel**
//! in front of every segment's literal text. Sentinels are private-use
//! characters, and the scanner rejects private-use characters in content
//! (`E0212`), so no page and no interpolated value can forge one.
//!
//! After rendering, the output is split on the sentinels: each chunk belongs to
//! the segment whose sentinel preceded it. A chunk from a `Markdown` segment is
//! the segment's literal text, possibly shortened at one edge by `{%-` or
//! `-%}`, so aligning it against the known literal gives a byte-exact origin. A
//! chunk from an `Output` segment maps to the whole expression. A chunk emitted
//! inside a loop carries that loop's iteration counter, which the sentinel
//! itself renders.
//!
//! Code segments never reach minijinja at all unless their fence carries
//! `template` (CM-11): their sentinel stands alone and the original bytes are
//! restored after rendering.
//!
//! `plan/rfcs/0021-source-text-for-expansion.md` records why these entry points
//! take a `SourceMap` that §34.9 does not name.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{Frame, Origin, Segment, SourceDocument, TemplateKind};
use liyasa_core::ids::FactId;
use liyasa_core::markdown::{Expanded, ExpansionRecord, SpanMap, TemplateContext};
use liyasa_core::source_map::SourceMap;
use liyasa_core::span::{SourceId, Span};
use liyasa_core::vfs::VfsPath;

use super::{filters, scan};

/// Opens a sentinel; the segment index follows in decimal.
pub const SENTINEL_START: char = '\u{e000}';
/// Separates the segment index from the loop iteration index.
pub const SENTINEL_LOOP: char = '\u{e001}';
/// Closes a sentinel.
pub const SENTINEL_END: char = '\u{e002}';

/// The name the page template is registered under.
const PAGE: &str = "<page>";

/// The dimensions a page may read, which are recorded when it does (CM-12).
const DIMENSIONS: &[&str] = &["version", "locale", "product", "region"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Undefined {
    /// `build`: an undefined name is `E0201`.
    #[default]
    Strict,
    /// `dev`: an undefined name renders as `⚠ undefined: name`.
    Lenient,
}

/// The CM-16 caps. `depth` and `iterations` are enforced by minijinja and are
/// applied by [`environment`]; `output_bytes` and `cpu` are enforced by the
/// writer, which is the only place that sees output as it is produced.
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub depth: usize,
    pub iterations: u64,
    pub output_bytes: usize,
    pub cpu: Duration,
}

impl Budget {
    /// Per page at build time.
    pub const BUILD: Self = Self {
        depth: 32,
        iterations: 100_000,
        output_bytes: 4 << 20,
        cpu: Duration::from_secs(2),
    };

    /// Per dynamic page in the request path (§6.6.4).
    pub const REQUEST: Self = Self {
        depth: 32,
        iterations: 100_000,
        output_bytes: 1 << 20,
        cpu: Duration::from_millis(200),
    };
}

impl Default for Budget {
    fn default() -> Self {
        Self::BUILD
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ExpandOptions {
    pub budget: Budget,
    pub undefined: Undefined,
}

/// An environment with the CM-16 caps and the CM-17 undefined mode applied.
///
/// The caller adds Liyasa's filters and functions (CM-14, CM-15) and registers
/// the snippets a page may include; everything here is policy that expansion
/// itself depends on.
pub fn environment(options: &ExpandOptions) -> minijinja::Environment<'static> {
    let mut env = minijinja::Environment::new();
    env.set_recursion_limit(options.budget.depth);
    env.set_fuel(Some(options.budget.iterations));
    env.set_undefined_behavior(match options.undefined {
        Undefined::Strict => minijinja::UndefinedBehavior::Strict,
        Undefined::Lenient => minijinja::UndefinedBehavior::Lenient,
    });
    env.set_keep_trailing_newline(true);
    // Markdown output is not HTML, so autoescaping is off here (CM-20); the
    // escaping that matters happens where a value enters the context
    // ([`escape_untrusted_markdown`](super::escape_untrusted_markdown)) and
    // where one reaches a component or HTML attribute, in the render pass.
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
    filters::install(&mut env);
    env
}

/// Expands a page at the build budgets.
pub fn expand(
    map: &SourceMap,
    document: &SourceDocument,
    context: &TemplateContext,
    env: &minijinja::Environment<'static>,
) -> Result<Expanded, Diagnostics> {
    expand_with(map, document, context, env, &ExpandOptions::default())
}

pub fn expand_with(
    map: &SourceMap,
    document: &SourceDocument,
    context: &TemplateContext,
    env: &minijinja::Environment<'static>,
    options: &ExpandOptions,
) -> Result<Expanded, Diagnostics> {
    let Some(file) = map.try_get(document.source) else {
        let mut diagnostics = Diagnostics::new();
        diagnostics.push(Diagnostic::new(
            code::E0205,
            format!(
                "source {} is not in this build's source map",
                document.source.0
            ),
        ));
        return Err(diagnostics);
    };
    let text = &*file.text;

    let (record, mut diagnostics) = record(map, text, document);
    if diagnostics.has_errors() {
        return Err(diagnostics);
    }

    let assembly = assemble(text, document);
    let mut local = env.clone();
    if let Err(error) = local.add_template_owned(PAGE, assembly.text.clone()) {
        diagnostics.push(describe(&error, &assembly, document.source, context));
        return Err(diagnostics);
    }
    let template = match local.get_template(PAGE) {
        Ok(template) => template,
        Err(error) => {
            diagnostics.push(describe(&error, &assembly, document.source, context));
            return Err(diagnostics);
        }
    };

    let values = minijinja::Value::from_object(Root {
        values: context.values.clone(),
        lenient: options.undefined == Undefined::Lenient,
    });
    let mut sink = Sink::new(options.budget);
    if let Err(error) = template.render_captured_to(values, &mut sink).map(|_| ()) {
        if let Some(reason) = sink.overflow {
            diagnostics.push(Diagnostic::new(code::E0204, reason).at(Span::new(
                document.source,
                0,
                0,
            )));
        } else {
            diagnostics.push(describe(&error, &assembly, document.source, context));
        }
        return Err(diagnostics);
    }
    let rendered = match String::from_utf8(sink.out) {
        Ok(rendered) => rendered,
        Err(_) => {
            diagnostics.push(Diagnostic::new(
                code::E0202,
                "expansion produced text that is not UTF-8",
            ));
            return Err(diagnostics);
        }
    };

    let (expanded, span_map) = split(&rendered, text, document, &mut diagnostics);
    if diagnostics.has_errors() {
        return Err(diagnostics);
    }
    Ok(Expanded {
        text: expanded,
        map: SpanMap(span_map),
        record,
    })
}

// ---- assembly ----

struct Assembly {
    text: String,
    /// `(assembled start, length, source start)` for every run copied
    /// verbatim, so a minijinja error position maps back to the page.
    runs: Vec<(u32, u32, u32)>,
}

impl Assembly {
    fn copy(&mut self, text: &str, source_at: usize) {
        self.runs
            .push((self.text.len() as u32, text.len() as u32, source_at as u32));
        self.text.push_str(text);
    }

    /// The source offset an assembled offset came from, or `None` for a run the
    /// assembly generated (a sentinel, a rewritten `{% snippet %}`).
    fn to_source(&self, at: u32) -> Option<u32> {
        let found = self.runs.partition_point(|(start, _, _)| *start <= at);
        let (start, len, source) = *self.runs.get(found.checked_sub(1)?)?;
        (at < start + len).then_some(source + (at - start))
    }
}

fn assemble(text: &str, document: &SourceDocument) -> Assembly {
    let mut out = Assembly {
        text: String::with_capacity(text.len() + document.segments.len() * 12),
        runs: Vec::with_capacity(document.segments.len()),
    };
    let masked = masked_ranges(text, document);
    let mut loops: Vec<Span> = Vec::new();
    let mut raw = 0usize;

    for segment in &document.segments {
        let span = segment.span();
        let literal = &text[span.start as usize..span.end as usize];
        let in_loop = raw == 0 && !loops.is_empty();
        match segment {
            Segment::Template { kind, .. } => match kind {
                TemplateKind::Comment => out.copy(literal, span.start as usize),
                TemplateKind::Output => {
                    sentinel(&mut out, Piece::Expression, span, in_loop);
                    out.copy(literal, span.start as usize);
                }
                TemplateKind::Statement { name, .. } => {
                    match name.as_str() {
                        "raw" => raw += 1,
                        "endraw" => raw = raw.saturating_sub(1),
                        "for" if raw == 0 => loops.push(span),
                        "endfor" if raw == 0 => {
                            loops.pop();
                        }
                        _ => {}
                    }
                    if raw == 0 && name == "snippet" {
                        snippet(&mut out, literal);
                    } else {
                        out.copy(literal, span.start as usize);
                    }
                }
            },
            // A fence that did not opt into templating never reaches
            // minijinja: its sentinel stands alone and the bytes come back
            // from the source (CM-11).
            Segment::Code { info, .. } if !info.attrs.flags.contains("template") => {
                sentinel(&mut out, Piece::Restore, span, in_loop);
            }
            Segment::Code { .. } => {
                sentinel(&mut out, Piece::Literal, span, in_loop);
                out.copy(literal, span.start as usize);
            }
            // Inline code spans are held out the same way (CM-11, CM-23); a
            // raw block already stops templating, so it needs no splitting.
            _ if raw == 0 => {
                let mut at = span.start as usize;
                for range in spans_within(&masked, span) {
                    if range.start > at {
                        emit_literal(&mut out, text, at..range.start, in_loop);
                    }
                    sentinel(
                        &mut out,
                        Piece::Restore,
                        Span::new(span.source, range.start as u32, range.end as u32),
                        in_loop,
                    );
                    at = range.end;
                }
                if at < span.end as usize {
                    emit_literal(&mut out, text, at..span.end as usize, in_loop);
                }
            }
            _ => {
                sentinel(&mut out, Piece::Literal, span, in_loop);
                out.copy(literal, span.start as usize);
            }
        }
    }
    out
}

fn emit_literal(out: &mut Assembly, text: &str, range: std::ops::Range<usize>, in_loop: bool) {
    sentinel(
        out,
        Piece::Literal,
        Span::new(SourceId(0), range.start as u32, range.end as u32),
        in_loop,
    );
    out.copy(&text[range.clone()], range.start);
}

/// The inline code spans of the body, computed the way the scanner computes
/// them: per run of segments that templating may touch, so a span cannot cross
/// a fence or a directive line.
fn masked_ranges(text: &str, document: &SourceDocument) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut run: Option<std::ops::Range<usize>> = None;
    for segment in &document.segments {
        let span = segment.span();
        let touchable = matches!(segment, Segment::Markdown { .. } | Segment::Template { .. });
        match (touchable, run.take()) {
            (true, Some(range)) => run = Some(range.start..span.end as usize),
            (true, None) => run = Some(span.start as usize..span.end as usize),
            (false, Some(range)) => out.extend(scan::masked_ranges(text, range)),
            (false, None) => {}
        }
    }
    if let Some(range) = run {
        out.extend(scan::masked_ranges(text, range));
    }
    out.sort_by_key(|range| range.start);
    out
}

/// The masked ranges that fall inside one segment.
fn spans_within(
    masked: &[std::ops::Range<usize>],
    span: Span,
) -> impl Iterator<Item = std::ops::Range<usize>> {
    let (start, end) = (span.start as usize, span.end as usize);
    masked
        .iter()
        .filter(move |range| range.start < end && range.end > start)
        .map(move |range| range.start.max(start)..range.end.min(end))
        .collect::<Vec<_>>()
        .into_iter()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Piece {
    /// Template text minijinja copies through; the chunk aligns against it.
    Literal,
    /// Held out of the render and restored from the source afterwards.
    Restore,
    /// `{{ … }}`; the chunk is the value.
    Expression,
}

impl Piece {
    const fn letter(self) -> char {
        match self {
            Self::Literal => 'l',
            Self::Restore => 'r',
            Self::Expression => 'e',
        }
    }

    fn of(letter: u8) -> Option<Self> {
        match letter {
            b'l' => Some(Self::Literal),
            b'r' => Some(Self::Restore),
            b'e' => Some(Self::Expression),
            _ => None,
        }
    }
}

/// `U+E000 <kind> <start> , <end> [U+E001 <iteration>] U+E002`.
///
/// The payload names the source range rather than a segment index, so a piece
/// of a segment — the text on either side of an inline code span — is as
/// addressable as a whole one.
fn sentinel(out: &mut Assembly, piece: Piece, span: Span, in_loop: bool) {
    out.text.push(SENTINEL_START);
    out.text.push(piece.letter());
    out.text.push_str(&span.start.to_string());
    out.text.push(',');
    out.text.push_str(&span.end.to_string());
    if in_loop {
        out.text.push(SENTINEL_LOOP);
        out.text.push_str("{{ loop.index0 }}");
    }
    out.text.push(SENTINEL_END);
}

/// `{% snippet "name" a=1 %}` is sugar for an include under a `with` (CM-70).
/// minijinja has no custom statements, so the sugar is removed here rather
/// than taught to the parser.
fn snippet(out: &mut Assembly, tag: &str) {
    let inner = tag
        .trim_start_matches("{%")
        .trim_end_matches("%}")
        .trim_matches('-')
        .trim();
    let rest = inner.trim_start_matches("snippet").trim();
    let (name, props) = match rest.split_once(|c: char| c.is_whitespace()) {
        Some((name, props)) => (name.trim(), props.trim()),
        None => (rest, ""),
    };
    let name = name.trim_matches(['"', '\'']);
    let include = format!("{{% include \"snippets/{name}.md\" %}}");
    if props.is_empty() {
        out.text.push_str(&include);
        return;
    }
    let bindings = props
        .split_whitespace()
        .map(|pair| pair.replacen('=', " = ", 1))
        .collect::<Vec<_>>()
        .join(", ");
    out.text
        .push_str(&format!("{{% with {bindings} %}}{include}{{% endwith %}}"));
}

// ---- splitting the render back apart ----

fn split(
    rendered: &str,
    text: &str,
    document: &SourceDocument,
    diagnostics: &mut Diagnostics,
) -> (String, Vec<(u32, u32, Origin)>) {
    let source = document.source;
    let mut out = String::with_capacity(rendered.len());
    let mut map: Vec<(u32, u32, Origin)> = Vec::new();
    let mut parts = rendered.split(SENTINEL_START);
    let mut last = Span::new(source, 0, 0);

    // Anything before the first sentinel was emitted by a statement, so no
    // piece claims it.
    if let Some(prologue) = parts.next()
        && !prologue.is_empty()
    {
        push(&mut out, &mut map, prologue, Origin::default());
    }

    for part in parts {
        let Some((head, chunk)) = part.split_once(SENTINEL_END) else {
            // The private use area is reserved for these sentinels and the
            // scanner rejects it in content, so a stray one came from a value.
            stray(diagnostics, last);
            push(&mut out, &mut map, part, Origin::default());
            continue;
        };
        let (head, iteration) = match head.split_once(SENTINEL_LOOP) {
            Some((head, iteration)) => (head, iteration.parse::<u32>().ok()),
            None => (head, None),
        };
        let Some((piece, span)) = parse_sentinel(head, source) else {
            stray(diagnostics, last);
            push(&mut out, &mut map, chunk, Origin::default());
            continue;
        };
        last = span;
        if chunk.chars().any(scan::is_private_use) {
            stray(diagnostics, span);
        }

        let literal = &text[span.start as usize..span.end as usize];
        let mut frames = Vec::new();
        if let Some(index) = iteration {
            frames.push(Frame::Loop { at: span, index });
        }

        match piece {
            Piece::Restore => {
                push(
                    &mut out,
                    &mut map,
                    literal,
                    Origin {
                        span: Some(span),
                        frames: frames.clone(),
                    },
                );
                if !chunk.is_empty() {
                    push(&mut out, &mut map, chunk, Origin { span: None, frames });
                }
            }
            Piece::Expression => {
                // The bytes are the value, not the expression, so the run
                // carries a `Generated` frame as well as the span it came from
                // (§7.3.1 items 2 and 5).
                frames.push(Frame::Generated { by: span });
                push(
                    &mut out,
                    &mut map,
                    chunk,
                    Origin {
                        span: Some(span),
                        frames,
                    },
                );
            }
            Piece::Literal => match align(literal, chunk) {
                Some(at) => {
                    let start = span.start + at as u32;
                    push(
                        &mut out,
                        &mut map,
                        chunk,
                        Origin {
                            span: Some(Span::new(source, start, start + chunk.len() as u32)),
                            frames,
                        },
                    );
                }
                None => {
                    frames.push(Frame::Generated { by: span });
                    push(&mut out, &mut map, chunk, Origin { span: None, frames });
                }
            },
        }
    }
    (out, map)
}

fn parse_sentinel(head: &str, source: SourceId) -> Option<(Piece, Span)> {
    let piece = Piece::of(*head.as_bytes().first()?)?;
    let (start, end) = head.get(1..)?.split_once(',')?;
    Some((
        piece,
        Span::new(source, start.parse().ok()?, end.parse().ok()?),
    ))
}

fn stray(diagnostics: &mut Diagnostics, at: Span) {
    diagnostics.push(
        Diagnostic::new(
            code::E0212,
            "expansion produced a private-use character, which is reserved for its sentinels",
        )
        .at(at),
    );
}

fn push(out: &mut String, map: &mut Vec<(u32, u32, Origin)>, chunk: &str, origin: Origin) {
    if chunk.is_empty() {
        return;
    }
    let start = out.len() as u32;
    out.push_str(chunk);
    map.push((start, out.len() as u32, origin));
}

/// Where a rendered chunk sits inside the literal it came from.
///
/// Whitespace control only ever trims at the edges, so the chunk is the
/// literal, the literal without its leading whitespace, or without its
/// trailing whitespace, or both.
fn align(literal: &str, chunk: &str) -> Option<usize> {
    if chunk.is_empty() || literal == chunk {
        return Some(0);
    }
    let lead = literal.len() - literal.trim_start().len();
    if literal
        .get(lead..)
        .is_some_and(|rest| rest.starts_with(chunk))
    {
        return Some(lead);
    }
    literal.find(chunk)
}

// ---- what the page read (CM-12, CM-19) ----

fn record(
    map: &SourceMap,
    text: &str,
    document: &SourceDocument,
) -> (ExpansionRecord, Diagnostics) {
    let mut out = ExpansionRecord::default();
    let mut diagnostics = Diagnostics::new();
    let personalized = document
        .frontmatter
        .as_ref()
        .and_then(|front| front.typed.personalized)
        .unwrap_or_default();

    for segment in &document.segments {
        let Segment::Template { span, kind } = segment else {
            continue;
        };
        if matches!(kind, TemplateKind::Comment) {
            continue;
        }
        let tag = &text[span.start as usize..span.end as usize];

        for field in reads(tag, "reader.") {
            out.reader_fields.insert(field.clone());
            if !personalized {
                diagnostics.push(
                    Diagnostic::new(
                        code::E0208,
                        format!("`reader.{field}` needs `personalized: true` in front matter"),
                    )
                    .at(*span)
                    .help("a page that reads reader fields is rendered on demand (§6.6.4)"),
                );
            }
        }
        for name in calls(tag, "env") {
            out.env.insert(name);
        }
        for id in calls(tag, "fact") {
            out.facts.insert(FactId::new(id));
        }
        for dimension in DIMENSIONS {
            if mentions(tag, dimension) {
                out.dimensions.insert((*dimension).to_owned());
            }
        }
        for name in includes(tag) {
            if let Some(id) = map.find(&VfsPath::new(&name)) {
                out.includes.push(id);
            }
        }
    }
    out.includes.sort_by_key(|id| id.0);
    out.includes.dedup();
    (out, diagnostics)
}

/// Field names read through a `prefix.` accessor.
fn reads(tag: &str, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(found) = tag[at..].find(prefix) {
        let start = at + found;
        at = start + prefix.len();
        if start > 0 && is_name_byte(tag.as_bytes()[start - 1]) {
            continue;
        }
        let rest = &tag[at..];
        let len = rest.find(|c: char| !is_name_char(c)).unwrap_or(rest.len());
        if len > 0 {
            out.push(rest[..len].to_owned());
        }
    }
    out
}

/// The literal first argument of every `name("…")` call in a tag.
fn calls(tag: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while let Some(found) = tag[at..].find(name) {
        let start = at + found;
        at = start + name.len();
        if start > 0 && is_name_byte(tag.as_bytes()[start - 1]) {
            continue;
        }
        let rest = tag[at..].trim_start();
        let Some(rest) = rest.strip_prefix('(') else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        if let Some(end) = rest[1..].find(quote) {
            out.push(rest[1..1 + end].to_owned());
        }
    }
    out
}

fn mentions(tag: &str, name: &str) -> bool {
    let mut at = 0usize;
    while let Some(found) = tag[at..].find(name) {
        let start = at + found;
        at = start + name.len();
        let before = start.checked_sub(1).is_none_or(|before| {
            !is_name_byte(tag.as_bytes()[before]) && tag.as_bytes()[before] != b'.'
        });
        let after = tag[at..].chars().next().is_none_or(|c| !is_name_char(c));
        if before && after {
            return true;
        }
    }
    false
}

/// Template names an `{% include %}`, `{% snippet %}`, `{% import %}`, or
/// `{% from … import %}` statement pulls in (CM-19).
pub(crate) fn includes(tag: &str) -> Vec<String> {
    let inner = tag
        .trim_start_matches("{%")
        .trim_end_matches("%}")
        .trim_matches('-')
        .trim();
    let (statement, rest) = match inner.split_once(|c: char| c.is_whitespace()) {
        Some(split) => split,
        None => return Vec::new(),
    };
    let rest = rest.trim();
    let name = rest
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(['"', '\'']);
    if name.is_empty() {
        return Vec::new();
    }
    match statement {
        "include" | "import" | "from" => vec![name.to_owned()],
        "snippet" => vec![format!("snippets/{name}.md")],
        _ => Vec::new(),
    }
}

fn is_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

// ---- the output sink (CM-16) ----

struct Sink {
    out: Vec<u8>,
    budget: Budget,
    clock: Clock,
    overflow: Option<&'static str>,
}

impl Sink {
    fn new(budget: Budget) -> Self {
        Self {
            out: Vec::new(),
            budget,
            clock: Clock::start(),
            overflow: None,
        }
    }
}

impl std::io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.out.len() + buf.len() > self.budget.output_bytes {
            self.overflow = Some("template output exceeds the page output budget");
            return Err(std::io::Error::other("output budget"));
        }
        if self.clock.elapsed_past(self.budget.cpu) {
            self.overflow = Some("template render exceeds the page time budget");
            return Err(std::io::Error::other("time budget"));
        }
        self.out.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// `Instant` has no backend on `wasm32-unknown-unknown`, where the host owns
/// the time budget instead.
struct Clock(#[cfg(not(target_arch = "wasm32"))] std::time::Instant);

impl Clock {
    fn start() -> Self {
        Self(
            #[cfg(not(target_arch = "wasm32"))]
            std::time::Instant::now(),
        )
    }

    fn elapsed_past(&self, limit: Duration) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.0.elapsed() > limit
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = limit;
            false
        }
    }
}

// ---- the template root ----

/// The root the page is rendered against.
///
/// It does two things the caller's plain map cannot: it makes `env` answer to
/// both `env.CI` and `env("CI")` (CM-12 with CM-15), and in `dev` it renders an
/// undefined name as a visible marker instead of failing the build (CM-17).
#[derive(Debug)]
struct Root {
    values: minijinja::Value,
    lenient: bool,
}

impl minijinja::value::Object for Root {
    fn get_value(self: &Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        let found = self.values.get_item(key).unwrap_or_default();
        if key.as_str() == Some("env") {
            return Some(minijinja::Value::from_object(super::filters::EnvAccessor(
                found,
            )));
        }
        if found.is_undefined() {
            return self
                .lenient
                .then(|| minijinja::Value::from(format!("⚠ undefined: {key}")));
        }
        Some(found)
    }

    fn enumerate(self: &Arc<Self>) -> minijinja::value::Enumerator {
        match self.values.try_iter() {
            Ok(keys) => minijinja::value::Enumerator::Values(keys.collect()),
            Err(_) => minijinja::value::Enumerator::NonEnumerable,
        }
    }
}

// ---- minijinja errors (CM-18) ----

fn describe(
    error: &minijinja::Error,
    assembly: &Assembly,
    source: SourceId,
    context: &TemplateContext,
) -> Diagnostic {
    use minijinja::ErrorKind;

    let detail = error.detail().unwrap_or_default();
    // A filter or function this crate installed puts its own code in the
    // message, because a minijinja error has nowhere else to carry one.
    if let Some((chosen, message)) = tagged_code(detail) {
        let mut diagnostic = Diagnostic::new(chosen, message);
        if let Some(range) = error.range()
            && let Some(start) = assembly.to_source(range.start as u32)
        {
            let end = assembly
                .to_source(range.end.saturating_sub(1) as u32)
                .map_or(start, |end| end + 1);
            diagnostic = diagnostic.at(Span::new(source, start, end.max(start)));
        }
        return diagnostic;
    }
    let chosen = match error.kind() {
        ErrorKind::UndefinedError => code::E0201,
        ErrorKind::UnknownFilter
        | ErrorKind::UnknownFunction
        | ErrorKind::UnknownTest
        | ErrorKind::UnknownMethod => code::E0203,
        ErrorKind::TemplateNotFound | ErrorKind::BadInclude => code::E0205,
        ErrorKind::OutOfFuel => code::E0204,
        _ if detail.contains("recursion") || detail.contains("too many elements") => code::E0204,
        _ => code::E0202,
    };

    let mut diagnostic = Diagnostic::new(chosen, error.to_string());
    let range = error.range();
    if let Some(range) = range.clone()
        && let Some(start) = assembly.to_source(range.start as u32)
    {
        let end = assembly
            .to_source(range.end.saturating_sub(1) as u32)
            .map_or(start, |end| end + 1);
        diagnostic = diagnostic.at(Span::new(source, start, end.max(start)));
    }
    // minijinja reports "undefined value" without naming it; the name is the
    // expression the error points at.
    if chosen == code::E0201
        && let Some(name) = range
            .and_then(|range| assembly.text.get(range))
            .map(str::trim)
        && let Some(closest) = closest(name, &context.values)
    {
        diagnostic = diagnostic.help(format!("did you mean `{closest}`?"));
    }
    diagnostic
}

/// `E0211: …` at the front of a message, put there by [`filters`].
fn tagged_code(detail: &str) -> Option<(liyasa_core::Code, String)> {
    let (text, message) = detail.split_once(": ")?;
    let code = liyasa_core::Code::new(text)?;
    Some((code, message.to_owned()))
}

fn closest(name: &str, values: &minijinja::Value) -> Option<String> {
    let Ok(keys) = values.try_iter() else {
        return None;
    };
    let candidates: BTreeSet<String> = keys
        .filter_map(|key| key.as_str().map(str::to_owned))
        .collect();
    candidates
        .into_iter()
        .map(|candidate| {
            let distance = edit_distance(name, &candidate);
            (distance, candidate)
        })
        .filter(|(distance, _)| *distance <= 2 || *distance * 3 <= name.len())
        .min_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, candidate)| candidate)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (row, left) in a.chars().enumerate() {
        current[0] = row + 1;
        for (column, right) in b.iter().enumerate() {
            let cost = usize::from(left != *right);
            current[column + 1] = (previous[column] + cost)
                .min(previous[column + 1] + 1)
                .min(current[column] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use minijinja::context;

    use super::*;

    fn page(text: &str) -> (SourceMap, SourceDocument) {
        let mut map = SourceMap::new();
        let id = map.intern(VfsPath::new("page.md"), Arc::from(text));
        let (document, diagnostics) = scan::scan(text, id);
        assert!(
            !diagnostics.has_errors(),
            "the fixture does not scan cleanly: {:?}",
            diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
        );
        (map, document)
    }

    fn context(values: minijinja::Value) -> TemplateContext {
        TemplateContext {
            values,
            tracking: true,
        }
    }

    fn render_with(
        text: &str,
        values: minijinja::Value,
        options: &ExpandOptions,
    ) -> Result<Expanded, Diagnostics> {
        let (map, document) = page(text);
        let env = environment(options);
        expand_with(&map, &document, &context(values), &env, options)
    }

    fn render(text: &str, values: minijinja::Value) -> Expanded {
        match render_with(text, values, &ExpandOptions::default()) {
            Ok(expanded) => {
                assert_origins_agree(text, &expanded);
                expanded
            }
            Err(diagnostics) => panic!(
                "expansion failed: {:?}",
                diagnostics
                    .iter()
                    .map(|d| format!("{} {}", d.code, d.message))
                    .collect::<Vec<_>>()
            ),
        }
    }

    fn fails(text: &str, values: minijinja::Value) -> Vec<&'static str> {
        match render_with(text, values, &ExpandOptions::default()) {
            Ok(expanded) => panic!("expected a failure, got {:?}", expanded.text),
            Err(diagnostics) => diagnostics.iter().map(|d| d.code.as_str()).collect(),
        }
    }

    /// §7.3.1 item 3: every byte of a located run maps back to itself.
    fn assert_origins_agree(text: &str, expanded: &Expanded) {
        assert!(expanded.map.is_well_formed(), "{:?}", expanded.map);
        for (start, end, origin) in &expanded.map.0 {
            // A run a template emitted carries a `Generated` frame: its bytes
            // are a value, not the source's.
            if origin
                .frames
                .iter()
                .any(|frame| matches!(frame, Frame::Generated { .. }))
            {
                continue;
            }
            let Some(span) = origin.span else { continue };
            let produced = &expanded.text[*start as usize..*end as usize];
            let original = &text[span.start as usize..span.end as usize];
            assert_eq!(
                produced, original,
                "run {start}..{end} claims {span:?} but the bytes differ"
            );
        }
        let covered: usize = expanded
            .map
            .0
            .iter()
            .map(|(start, end, _)| (end - start) as usize)
            .sum();
        assert_eq!(
            covered,
            expanded.text.len(),
            "the map leaves bytes unclaimed"
        );
    }

    // ---- CM-10 ----

    #[test]
    fn output_statements_and_comments() {
        let text = "{{ x }}\n{% if y %}branch{% endif %}\n{# hidden #}\ntail\n";
        let out = render(text, context! { x => 1, y => true });
        assert!(out.text.contains('1'), "{:?}", out.text);
        assert!(out.text.contains("branch"), "{:?}", out.text);
        assert!(!out.text.contains("hidden"), "{:?}", out.text);
        assert!(out.text.ends_with("tail\n"), "{:?}", out.text);
    }

    #[test]
    fn a_false_branch_emits_nothing() {
        let out = render(
            "before\n{% if y %}branch{% endif %}\nafter\n",
            context! { y => false },
        );
        assert!(!out.text.contains("branch"), "{:?}", out.text);
        assert!(out.text.contains("before"));
        assert!(out.text.contains("after"));
    }

    #[test]
    fn a_loop_repeats_its_body() {
        let out = render(
            "{% for row in rows %}\n- {{ row }}\n{% endfor %}\n",
            context! { rows => vec!["a", "b"] },
        );
        assert!(out.text.contains("- a"), "{:?}", out.text);
        assert!(out.text.contains("- b"), "{:?}", out.text);
    }

    // ---- CM-11 ----

    #[test]
    fn a_fence_is_not_templated() {
        let text = "```bash\necho {{ x }}\n```\n";
        let out = render(text, context! { x => 1 });
        assert_eq!(out.text, text);
    }

    #[test]
    fn a_fence_with_the_attribute_is_templated() {
        let out = render("```bash template\necho {{ x }}\n```\n", context! { x => 1 });
        assert!(out.text.contains("echo 1"), "{:?}", out.text);
    }

    #[test]
    fn an_inline_code_span_is_not_templated() {
        let text = "write `{{ x }}` to print\n";
        assert_eq!(render(text, context! { x => 1 }).text, text);
    }

    #[test]
    fn a_raw_block_is_not_templated() {
        let out = render("{% raw %}\n{{ x }}\n{% endraw %}\n", context! { x => 1 });
        assert!(out.text.contains("{{ x }}"), "{:?}", out.text);
    }

    #[test]
    fn a_fence_inside_a_loop_is_restored_each_time() {
        let out = render(
            "{% for row in rows %}\n```\ncode {{ x }}\n```\n{% endfor %}\n",
            context! { rows => vec![1, 2], x => 9 },
        );
        assert_eq!(
            out.text.matches("code {{ x }}").count(),
            2,
            "{:?}",
            out.text
        );
    }

    // ---- CM-17 ----

    #[test]
    fn an_undefined_name_is_an_error_in_strict_mode() {
        assert_eq!(fails("{{ missing }}\n", context! { x => 1 }), ["E0201"]);
    }

    #[test]
    fn an_undefined_name_is_a_marker_in_lenient_mode() {
        let options = ExpandOptions {
            undefined: Undefined::Lenient,
            ..ExpandOptions::default()
        };
        let out = render_with("{{ missing }}\n", context! { x => 1 }, &options)
            .expect("lenient expansion");
        assert!(out.text.contains("⚠ undefined: missing"), "{:?}", out.text);
    }

    #[test]
    fn an_undefined_name_suggests_the_closest_one() {
        let (map, document) = page("{{ titel }}\n");
        let options = ExpandOptions::default();
        let env = environment(&options);
        let diagnostics = expand(&map, &document, &context(context! { title => "A" }), &env)
            .expect_err("undefined");
        let reported = diagnostics.iter().next().expect("a diagnostic");
        assert_eq!(reported.code, code::E0201);
        assert_eq!(reported.help.as_deref(), Some("did you mean `title`?"));
    }

    // ---- CM-18 ----

    #[test]
    fn a_template_error_points_at_the_source() {
        let text = "line one\nline two\nsay {{ missing }} here\n";
        let (map, document) = page(text);
        let options = ExpandOptions::default();
        let env = environment(&options);
        let diagnostics =
            expand(&map, &document, &context(context! { x => 1 }), &env).expect_err("undefined");
        let span = diagnostics
            .iter()
            .next()
            .and_then(|d| d.span)
            .expect("a located diagnostic");
        let (start, _) = map.line_col(span);
        assert_eq!(start.line, 3);
        assert!(
            text[span.start as usize..span.end as usize].contains("missing"),
            "{:?}",
            &text[span.start as usize..span.end as usize]
        );
    }

    #[test]
    fn an_unknown_filter_is_reported() {
        assert_eq!(
            fails("{{ x | nosuchfilter }}\n", context! { x => 1 }),
            ["E0203"]
        );
    }

    #[test]
    fn a_missing_include_is_reported() {
        assert_eq!(fails("{% include \"nope.md\" %}\n", context! {}), ["E0205"]);
    }

    // ---- CM-22, §7.3.1 ----

    #[test]
    fn a_loop_body_carries_its_iteration_index() {
        let out = render(
            "{% for row in rows %}\n- {{ row }}\n{% endfor %}\n",
            context! { rows => vec!["a", "b", "c"] },
        );
        let indexes: BTreeSet<u32> = out
            .map
            .0
            .iter()
            .flat_map(|(_, _, origin)| origin.frames.iter())
            .filter_map(|frame| match frame {
                Frame::Loop { index, .. } => Some(*index),
                _ => None,
            })
            .collect();
        assert_eq!(indexes, BTreeSet::from([0, 1, 2]));
    }

    #[test]
    fn whitespace_control_only_trims_the_edges() {
        let text = "a\n{%- if y -%}\n   body   \n{%- endif -%}\nb\n";
        let out = render(text, context! { y => true });
        assert!(out.text.contains("body"), "{:?}", out.text);
    }

    #[test]
    fn a_private_use_character_from_a_value_is_rejected() {
        assert_eq!(
            fails("{{ x }}\n", context! { x => "a\u{e000}b" }),
            ["E0212"]
        );
    }

    #[test]
    fn origins_agree_across_generated_documents() {
        const FRAGMENTS: &[&str] = &[
            "para text\n",
            "\n",
            "{{ value }}\n",
            "{% for row in rows %}\n",
            "{% endfor %}\n",
            "{# note #}\n",
            "```bash\ncode {{ value }}\n```\n",
            "``` template\n{{ value }}\n```\n",
            ":::note\n",
            ":::\n",
            "- item with `{{ value }}`\n",
            ":::note\n",
            ":::\n",
            "{% raw %}\n{{ value }}\n{% endraw %}\n",
            "{%- if flag -%}\nbody\n{%- endif -%}\n",
        ];
        let mut state = 0x853c_49e6_748f_ea9bu64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let values = context! { value => "V", rows => vec![1, 2], flag => true };
        for _ in 0..300 {
            let mut text = String::new();
            let mut open_loops = 0usize;
            let mut open_directives = 0usize;
            for _ in 0..(next() % 7) + 1 {
                let fragment = FRAGMENTS[(next() % FRAGMENTS.len() as u64) as usize];
                match fragment {
                    "{% for row in rows %}\n" => open_loops += 1,
                    "{% endfor %}\n" if open_loops == 0 => continue,
                    "{% endfor %}\n" => open_loops -= 1,
                    ":::note\n" => open_directives += 1,
                    ":::\n" if open_directives == 0 => continue,
                    ":::\n" => open_directives -= 1,
                    _ => {}
                }
                text.push_str(fragment);
            }
            for _ in 0..open_directives {
                text.push_str(":::\n");
            }
            for _ in 0..open_loops {
                text.push_str("{% endfor %}\n");
            }
            let (map, document) = page(&text);
            let options = ExpandOptions::default();
            let env = environment(&options);
            if let Ok(expanded) = expand(&map, &document, &context(values.clone()), &env) {
                assert_origins_agree(&text, &expanded);
            }
        }
    }

    // ---- CM-16 ----

    #[test]
    fn a_runaway_loop_hits_the_iteration_budget() {
        assert_eq!(
            fails(
                "{% for a in range(1000) %}{% for b in range(1000) %}x{% endfor %}{% endfor %}\n",
                context! {}
            ),
            ["E0204"]
        );
    }

    #[test]
    fn oversized_output_hits_the_output_budget() {
        let big = "x".repeat(5 << 20);
        assert_eq!(fails("{{ big }}\n", context! { big => big }), ["E0204"]);
    }

    #[test]
    fn unbounded_recursion_hits_the_depth_budget() {
        let text = "{% macro f(n) %}{{ f(n) }}{% endmacro %}{{ f(1) }}\n";
        assert_eq!(fails(text, context! {}), ["E0204"]);
    }

    #[test]
    fn the_request_budget_is_tighter_than_the_build_budget() {
        const { assert!(Budget::REQUEST.output_bytes < Budget::BUILD.output_bytes) };
        assert!(Budget::REQUEST.cpu < Budget::BUILD.cpu);
    }

    // ---- CM-12, CM-19 ----

    #[test]
    fn reading_a_reader_field_without_personalized_is_rejected() {
        let text = "Hello {{ reader.name }}\n";
        assert_eq!(fails(text, context! {}), ["E0208"]);
    }

    #[test]
    fn a_personalized_page_may_read_reader_fields() {
        let text = "---\npersonalized: true\n---\nHello {{ reader.name }}\n";
        let out = render(text, context! { reader => context! { name => "Ada" } });
        assert!(out.text.contains("Ada"), "{:?}", out.text);
        assert!(out.record.reader_fields.contains("name"));
    }

    #[test]
    fn the_record_lists_what_the_page_read() {
        let text = "\
{{ fact(\"plan.pro.price\") }}
{{ env(\"CI\") }}
{% if version >= \"2.0\" %}new{% endif %}
";
        let out = render(
            text,
            context! {
                version => "2.0",
                env => context! { CI => "yes" },
                facts => context! { plan => context! { pro => context! { price => 99 } } },
            },
        );
        assert!(out.record.facts.contains(&FactId::new("plan.pro.price")));
        assert!(out.record.env.contains("CI"));
        assert!(out.record.dimensions.contains("version"));
    }

    #[test]
    fn a_filter_code_reaches_the_diagnostic() {
        let values = context! { env => context! { CI => "yes" } };
        assert_eq!(fails("{{ env(\"SECRET\") }}\n", values), ["E0211"]);
        let values = context! { facts => context! { plan => 1 } };
        assert_eq!(fails("{{ fact(\"plan.pro\") }}\n", values), ["E0209"]);
    }

    #[test]
    fn a_dimension_name_inside_another_word_is_not_a_read() {
        let (map, document) = page("{{ versionless }}\n");
        let options = ExpandOptions::default();
        let env = environment(&options);
        let out = expand(
            &map,
            &document,
            &context(context! { versionless => 1 }),
            &env,
        )
        .expect("expansion");
        assert!(out.record.dimensions.is_empty());
    }

    #[test]
    fn an_include_is_recorded_and_rendered() {
        let mut map = SourceMap::new();
        let snippet = "Shared **body**.\n";
        map.intern(VfsPath::new("snippets/note.md"), Arc::from(snippet));
        let text = "before\n{% include \"snippets/note.md\" %}\nafter\n";
        let id = map.intern(VfsPath::new("page.md"), Arc::from(text));
        let (document, _) = scan::scan(text, id);

        let options = ExpandOptions::default();
        let mut env = environment(&options);
        env.add_template_owned("snippets/note.md", snippet.to_owned())
            .expect("a valid snippet");
        let out = expand(&map, &document, &context(context! {}), &env).expect("expansion");
        assert!(out.text.contains("Shared **body**."), "{:?}", out.text);
        assert_eq!(out.record.includes.len(), 1);
    }

    // ---- CM-70 ----

    #[test]
    fn the_snippet_statement_is_sugar_for_an_include() {
        let mut map = SourceMap::new();
        let snippet = "Audience: {{ audience }}.\n";
        map.intern(VfsPath::new("snippets/note.md"), Arc::from(snippet));
        let text = "{% snippet \"note\" audience=\"admin\" %}\n";
        let id = map.intern(VfsPath::new("page.md"), Arc::from(text));
        let (document, _) = scan::scan(text, id);

        let options = ExpandOptions::default();
        let mut env = environment(&options);
        env.add_template_owned("snippets/note.md", snippet.to_owned())
            .expect("a valid snippet");
        let out = expand(&map, &document, &context(context! {}), &env).expect("expansion");
        assert!(out.text.contains("Audience: admin."), "{:?}", out.text);
        assert_eq!(out.record.includes.len(), 1);
    }

    // ---- directives survive expansion ----

    #[test]
    fn a_directive_generated_by_a_loop_keeps_its_origin() {
        let text = "{% for tab in tabs %}\n:::tab\nbody\n:::\n{% endfor %}\n";
        let out = render(text, context! { tabs => vec![1, 2] });
        assert_eq!(out.text.matches(":::tab").count(), 2, "{:?}", out.text);
    }
}
