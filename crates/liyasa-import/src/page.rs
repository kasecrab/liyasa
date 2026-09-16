//! Turning one MDX page into a Liyasa Markdown page (CM-04, MIG-01, MIG-03).
//!
//! The pass is a scanner over the page's bytes rather than a parse: an MDX file
//! that a React toolchain accepted may still be anything, and a migration that
//! drops what it did not understand is worse than one that carries it across
//! and says so. Everything outside a JSX element, an expression container, or a
//! module statement is copied byte for byte.
//!
//! What it does convert:
//!
//! - `{/* … */}` becomes a template comment, which the Markdown parser erases;
//! - `import X from './partial.mdx'` plus `<X />` becomes `{% snippet "partial" %}`,
//!   which is how a Mintlify or Docusaurus partial reaches `snippets/` (CM-70);
//! - `export const NAME = "literal"` becomes front matter and `{NAME}` becomes
//!   `{{ NAME }}`, the mapping of §7.1;
//! - a multi-line JSX tag is joined onto one line, which is what the formatter's
//!   tag form expects (CM-53);
//! - each importer's own component table, through [`Convert`].
//!
//! Everything else it leaves where it is and reports: a JavaScript expression,
//! a module statement that is not a partial, a component Liyasa cannot render.

use std::collections::BTreeMap;

use liyasa_core::span::{SourceId, Span};
use liyasa_markdown::source::{FormatOptions, format_with, normalize, scan};

use crate::report::{Attention, Kind};

/// How a tag was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    /// `<Card …>`
    Open,
    /// `</Card>`
    Close,
    /// `<Icon … />`
    SelfClosing,
}

/// One JSX attribute, with its value exactly as the author wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prop {
    pub name: String,
    /// `None` for a bare attribute, which JSX reads as `true`.
    pub value: Option<String>,
}

impl Prop {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: Some(value.into()),
        }
    }

    /// The value with its quotes removed, when it is a quoted string.
    pub fn text(&self) -> Option<&str> {
        let value = self.value.as_deref()?;
        let bytes = value.as_bytes();
        match bytes.first() {
            Some(b'"') if value.ends_with('"') && value.len() >= 2 => {
                Some(&value[1..value.len() - 1])
            }
            Some(b'\'') if value.ends_with('\'') && value.len() >= 2 => {
                Some(&value[1..value.len() - 1])
            }
            _ => None,
        }
    }
}

/// One JSX element tag found in a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub kind: TagKind,
    pub props: Vec<Prop>,
    pub span: Span,
}

impl Tag {
    pub fn prop(&self, name: &str) -> Option<&Prop> {
        self.props.iter().find(|prop| prop.name == name)
    }

    /// A prop's quoted string value.
    pub fn text(&self, name: &str) -> Option<&str> {
        self.prop(name)?.text()
    }

    /// The directive spelling of the tag's name: `CodeGroup` is `code-group`
    /// (CM-53). The registry is keyed by it, so every lookup goes through here.
    pub fn directive_name(&self) -> String {
        directive_name(&self.name)
    }

    fn render(&self, name: &str, props: &[Prop]) -> String {
        match self.kind {
            TagKind::Close => format!("</{name}>"),
            _ => {
                let mut out = format!("<{name}");
                for prop in props {
                    out.push(' ');
                    out.push_str(&prop.name);
                    if let Some(value) = &prop.value {
                        out.push('=');
                        out.push_str(value);
                    }
                }
                if self.kind == TagKind::SelfClosing {
                    out.push_str(" />");
                } else {
                    out.push('>');
                }
                out
            }
        }
    }
}

/// What an importer decided about one tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Leave the tag as the author wrote it. A name Liyasa does not know is
    /// then reported, so this is the right answer for every built-in.
    Keep,
    /// Write it as a different component, with these props.
    Rewrite { name: String, props: Vec<Prop> },
    /// Replace the whole tag with this text.
    Replace(String),
    /// Drop the tag and keep whatever it wrapped.
    Unwrap,
}

/// Each importer's component table.
pub trait Convert {
    /// What one tag becomes. The default keeps every tag, which is already
    /// right for the components Liyasa took from Mintlify.
    fn element(&self, _tag: &Tag) -> Action {
        Action::Keep
    }

    /// Whether Liyasa can render a component of this name.
    fn known(&self, name: &str) -> bool;

    /// The registered name closest to one Liyasa does not know.
    fn suggest(&self, _name: &str) -> Option<String> {
        None
    }

    /// Whether an `import` of this module specifier is boilerplate the Liyasa
    /// page no longer needs, such as Docusaurus's `@theme/Tabs`.
    fn boilerplate(&self, _specifier: &str) -> bool {
        false
    }

    /// Source front matter key to Liyasa key. A key that is not listed is kept.
    fn frontmatter(&self) -> &BTreeMap<String, String> {
        static EMPTY: std::sync::OnceLock<BTreeMap<String, String>> = std::sync::OnceLock::new();
        EMPTY.get_or_init(BTreeMap::new)
    }

    /// Front matter keys the importer read elsewhere, which the page drops.
    fn consumed(&self) -> &[&str] {
        &[]
    }
}

/// How to convert one page.
pub struct Options<'a> {
    pub convert: &'a dyn Convert,
    /// Write components in the directive form rather than the tag form. MIG-01
    /// keeps the tag form by default, because it is valid Liyasa Markdown
    /// (CM-53) and leaves the smallest diff against the source repository.
    pub directives: bool,
}

/// One converted page.
#[derive(Debug, Clone, Default)]
pub struct Page {
    pub text: String,
    pub attention: Vec<Attention>,
    /// Partials the page included, as `(module specifier, snippet name)`, so
    /// the importer can carry each one into `snippets/`.
    pub snippets: Vec<(String, String)>,
    /// The page's front matter, parsed, for a caller that needs its `title` or
    /// `slug`. Null when the page has none.
    pub front: serde_json::Value,
}

/// Converts one MDX page.
pub fn convert(source: &str, options: &Options<'_>) -> Page {
    let text = normalize(source).into_owned();
    let (front_text, body_at) = split_frontmatter(&text);

    let mut page = Page::default();
    let mut state = Walk {
        text: &text,
        code: code_spans(&text),
        convert: options.convert,
        snippets: BTreeMap::new(),
        exports: Vec::new(),
        attention: Vec::new(),
        out: String::with_capacity(text.len()),
    };
    state.body(body_at);

    let Walk {
        snippets,
        exports,
        attention,
        out,
        ..
    } = state;

    let body = if options.directives {
        match format_with(&out, &FormatOptions { directives: true }) {
            Ok(text) => text,
            Err(reported) => {
                page.attention.extend(reported.iter().map(|d| {
                    Attention::new(Kind::CustomComponent, d.message.clone())
                        .help("the page could not be rewritten into the directive form")
                }));
                out
            }
        }
    } else {
        out
    };

    let front = frontmatter(front_text, &exports, options.convert);
    page.front = front_value(front.as_ref());
    page.text = match &front {
        Some(text) => format!("{text}{body}"),
        None => body,
    };
    page.attention.extend(attention);
    page.snippets = snippets
        .into_iter()
        .map(|(_, (specifier, name))| (specifier, name))
        .collect();
    page.snippets.sort();
    page.snippets.dedup();
    page
}

/// `CodeGroup` becomes `code-group`, the spelling the component registry uses
/// (CM-53). A name that is already lowercase is left alone.
pub fn directive_name(tag: &str) -> String {
    let mut out = String::with_capacity(tag.len() + 2);
    for (at, ch) in tag.char_indices() {
        if ch.is_ascii_uppercase() && at > 0 {
            out.push('-');
        }
        out.extend(ch.to_lowercase());
    }
    out
}

/// The snippet name a partial's module specifier gets: its file stem, without
/// the leading underscore a Docusaurus partial carries (CM-03 would make an
/// underscored file unroutable, which a snippet does not need).
pub fn snippet_name(specifier: &str) -> String {
    specifier
        .rsplit('/')
        .next()
        .unwrap_or(specifier)
        .rsplit_once('.')
        .map_or(specifier, |(stem, _)| stem)
        .trim_start_matches('_')
        .to_owned()
}

struct Walk<'a> {
    text: &'a str,
    code: Vec<(usize, usize)>,
    convert: &'a dyn Convert,
    /// Local JSX name to `(module specifier, snippet name)`.
    snippets: BTreeMap<String, (String, String)>,
    /// `export const NAME = <literal>` pairs, in source order.
    exports: Vec<(String, String)>,
    attention: Vec<Attention>,
    out: String,
}

impl Walk<'_> {
    fn body(&mut self, from: usize) {
        let mut at = from;
        let mut line_start = true;
        while at < self.text.len() {
            if let Some((_, end)) = self
                .code
                .iter()
                .find(|(start, end)| at >= *start && at < *end)
                .copied()
            {
                self.out.push_str(&self.text[at..end]);
                at = end;
                line_start = self.text[..at].ends_with('\n');
                continue;
            }

            let rest = &self.text[at..];
            let byte = rest.as_bytes()[0];

            if line_start && (rest.starts_with("import ") || rest.starts_with("export ")) {
                at = self.module(at);
                line_start = true;
                continue;
            }
            if byte == b'`' {
                at = self.inline_code(at);
                line_start = false;
                continue;
            }
            if byte == b'<'
                && let Some(next) = self.element(at)
            {
                at = next;
                line_start = false;
                continue;
            }
            if byte == b'{'
                && let Some(next) = self.brace(at)
            {
                at = next;
                line_start = false;
                continue;
            }

            let width = rest.chars().next().map_or(1, |c: char| c.len_utf8());
            self.out.push_str(&rest[..width]);
            line_start = byte == b'\n';
            at += width;
        }
    }

    /// An inline code span is content: `` `<Card />` `` names a component, it
    /// does not use one.
    fn inline_code(&mut self, at: usize) -> usize {
        let rest = &self.text[at..];
        let ticks = rest.len() - rest.trim_start_matches('`').len();
        let fence = &rest[..ticks];
        let end = rest[ticks..]
            .find(fence)
            .map_or(rest.len(), |found| ticks + found + ticks);
        self.out.push_str(&rest[..end]);
        at + end
    }

    /// An `import` or `export` line. MDX module statements have no Liyasa
    /// equivalent and would render as a paragraph of JavaScript, so the line
    /// always goes; what differs is whether anything is lost with it.
    fn module(&mut self, at: usize) -> usize {
        let rest = &self.text[at..];
        let end = rest.find('\n').map_or(rest.len(), |found| found + 1);
        let line = rest[..end].trim_end();
        let span = Span::new(SourceId(0), at as u32, (at + line.len()) as u32);

        if let Some((name, specifier)) = parse_import(line) {
            if is_partial(&specifier) {
                let snippet = snippet_name(&specifier);
                self.snippets.insert(name, (specifier.clone(), snippet));
                return at + end;
            }
            if self.convert.boilerplate(&specifier) {
                return at + end;
            }
            self.attention.push(
                Attention::new(Kind::Module, line.to_owned()).at(span).help(
                    "move shared content into `snippets/` and include it with `{% snippet %}`",
                ),
            );
            return at + end;
        }

        if let Some((name, value)) = parse_export_literal(line) {
            self.exports.push((name, value));
            return at + end;
        }

        if line.starts_with("export ") {
            self.attention.push(
                Attention::new(Kind::Module, line.to_owned())
                    .at(span)
                    .help("a value that is not a literal belongs in front matter or a fact"),
            );
            return at + end;
        }
        self.attention.push(
            Attention::new(Kind::Module, line.to_owned())
                .at(span)
                .help("move shared content into `snippets/` and include it with `{% snippet %}`"),
        );
        at + end
    }

    /// A JSX element tag. Returns `None` when what follows `<` is not one, so
    /// the caller copies the byte and carries on: `a < b` is prose and
    /// `<div>` is HTML (CM-53).
    fn element(&mut self, at: usize) -> Option<usize> {
        let tag = parse_tag(self.text, at)?;
        let end = tag.span.end as usize;

        if let Some((_, snippet)) = self.snippets.get(&tag.name) {
            let text = match tag.kind {
                TagKind::Close => String::new(),
                _ => snippet_call(snippet, &tag.props),
            };
            self.out.push_str(&text);
            return Some(end);
        }

        match self.convert.element(&tag) {
            Action::Keep => {
                let spelling = tag.directive_name();
                if tag.kind != TagKind::Close
                    && !self.convert.known(&tag.name)
                    && !self.convert.known(&spelling)
                {
                    let mut item =
                        Attention::new(Kind::CustomComponent, tag.name.clone()).at(tag.span);
                    if let Some(closest) = self.convert.suggest(&spelling) {
                        item = item.help(format!("did you mean `{closest}`?"));
                    } else {
                        item = item.help(
                            "write it as a user-defined component in `components/`, \
                             or replace it with a built-in",
                        );
                    }
                    self.attention.push(item);
                }
                self.out.push_str(&tag.render(&tag.name, &tag.props));
            }
            Action::Rewrite { name, props } => {
                self.out.push_str(&tag.render(&name, &props));
            }
            Action::Replace(text) => self.out.push_str(&text),
            Action::Unwrap => {}
        }
        Some(end)
    }

    /// A brace construct. Returns `None` when it is not one of MDX's.
    fn brace(&mut self, at: usize) -> Option<usize> {
        let rest = &self.text[at..];
        if let Some(body) = rest.strip_prefix("{/*") {
            let end = body.find("*/}")?;
            self.out.push_str("{#");
            self.out.push_str(&body[..end]);
            self.out.push_str("#}");
            return Some(at + 3 + end + 3);
        }
        // A construct that is already Liyasa's belongs to the template layer and
        // is copied whole: declining here would re-enter at its second brace
        // and read the remainder as an expression container.
        for (open, close) in [("{{", "}}"), ("{%", "%}"), ("{#", "#}")] {
            if !rest.starts_with(open) {
                continue;
            }
            let end = rest[2..].find(close).map_or_else(
                || rest.find('\n').unwrap_or(rest.len()),
                |found| 2 + found + 2,
            );
            self.out.push_str(&rest[..end]);
            return Some(at + end);
        }

        let end = match_brace(rest)?;
        let inner = rest[1..end - 1].trim();
        if inner.is_empty() {
            return None;
        }

        // `{NAME}` where an `export const NAME` supplied the value is §7.1's
        // mapping and needs no human.
        if self.exports.iter().any(|(name, _)| name == inner) {
            self.out.push_str(&format!("{{{{ {inner} }}}}"));
            return Some(at + end);
        }

        self.attention.push(
            Attention::new(Kind::Expression, rest[..end].to_owned())
                .at(Span::new(SourceId(0), at as u32, (at + end) as u32))
                .help("write it as `{{ … }}` over front matter, a snippet variable, or a fact"),
        );
        self.out.push_str(&rest[..end]);
        Some(at + end)
    }
}

fn snippet_call(name: &str, props: &[Prop]) -> String {
    let mut out = format!("{{% snippet \"{name}\"");
    for prop in props {
        match prop.text() {
            Some(text) => out.push_str(&format!(" {}=\"{}\"", prop.name, text)),
            None => match &prop.value {
                Some(value) => out.push_str(&format!(" {}={}", prop.name, value)),
                None => out.push_str(&format!(" {}=true", prop.name)),
            },
        }
    }
    out.push_str(" %}");
    out
}

/// Whether an imported module is a Markdown partial rather than code.
fn is_partial(specifier: &str) -> bool {
    specifier.ends_with(".mdx") || specifier.ends_with(".md")
}

/// `import Name from 'specifier'`, for the default import MDX partials use.
fn parse_import(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("import ")?.trim_start();
    let (name, rest) = rest.split_once(" from ")?;
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let specifier = rest.trim().trim_end_matches(';').trim();
    let quoted = specifier
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| {
            specifier
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
        })?;
    Some((name.to_owned(), quoted.to_owned()))
}

/// `export const NAME = "text"`, `= 3`, `= true`: the values that become front
/// matter. Anything else is JavaScript and is reported instead.
fn parse_export_literal(line: &str) -> Option<(String, String)> {
    let rest = line
        .strip_prefix("export const ")
        .or_else(|| line.strip_prefix("export let "))
        .or_else(|| line.strip_prefix("export var "))?;
    let (name, value) = rest.split_once('=')?;
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let value = value.trim().trim_end_matches(';').trim();
    let literal = match value.as_bytes().first()? {
        b'"' if value.ends_with('"') && value.len() >= 2 => value.to_owned(),
        b'\'' if value.ends_with('\'') && value.len() >= 2 => {
            format!("\"{}\"", &value[1..value.len() - 1])
        }
        b'0'..=b'9' | b'-' if value.parse::<f64>().is_ok() => value.to_owned(),
        _ if value == "true" || value == "false" => value.to_owned(),
        _ => return None,
    };
    Some((name.to_owned(), literal))
}

/// The end of the brace-delimited run starting at `text[0]`, counting nesting
/// and ignoring braces inside strings.
fn match_brace(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    for (at, byte) in bytes.iter().enumerate() {
        match quote {
            Some(open) => {
                if *byte == b'\\' {
                    continue;
                }
                if *byte == open {
                    quote = None;
                }
            }
            None => match byte {
                b'"' | b'\'' | b'`' => quote = Some(*byte),
                b'{' => depth += 1,
                b'}' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(at + 1);
                    }
                }
                _ => {}
            },
        }
    }
    None
}

/// A JSX tag at `text[at]`, or `None` when this is not one.
fn parse_tag(text: &str, at: usize) -> Option<Tag> {
    let rest = &text[at..];
    let body = rest.strip_prefix('<')?;
    let (body, kind) = match body.strip_prefix('/') {
        Some(body) => (body, TagKind::Close),
        None => (body, TagKind::Open),
    };
    // A component name starts with an uppercase letter; a lowercase tag is raw
    // HTML and is not ours to touch (CM-53).
    if !body.chars().next()?.is_ascii_uppercase() {
        return None;
    }
    let name_len = body
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '.')
        .unwrap_or(body.len());
    let name = &body[..name_len];
    let after = &body[name_len..];

    let close = tag_end(after)?;
    let (props_text, kind) = {
        let inner = &after[..close];
        match inner.trim_end().strip_suffix('/') {
            Some(trimmed) if kind == TagKind::Open => (trimmed, TagKind::SelfClosing),
            _ => (inner, kind),
        }
    };
    let props = if kind == TagKind::Close {
        Vec::new()
    } else {
        parse_props(props_text)
    };
    let end = at + 1 + usize::from(kind == TagKind::Close) + name_len + close + 1;
    Some(Tag {
        name: name.to_owned(),
        kind,
        props,
        span: Span::new(SourceId(0), at as u32, end as u32),
    })
}

/// The offset of the `>` that closes a tag whose attributes start at `text`,
/// skipping quoted values and brace expressions so `title="a > b"` cannot end
/// it early.
fn tag_end(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'>' => return Some(at),
            b'"' | b'\'' => {
                let quote = bytes[at];
                at += 1;
                while at < bytes.len() && bytes[at] != quote {
                    at += 1;
                }
            }
            b'{' => {
                let width = match_brace(&text[at..])?;
                at += width - 1;
            }
            _ => {}
        }
        at += 1;
    }
    None
}

fn parse_props(text: &str) -> Vec<Prop> {
    let mut props = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        let name_end = at
            + text[at..]
                .find(|c: char| c == '=' || c.is_whitespace())
                .unwrap_or(text.len() - at);
        let name = text[at..name_end].trim();
        at = name_end;
        if name.is_empty() {
            at += 1;
            continue;
        }
        if !text[at..].starts_with('=') {
            props.push(Prop {
                name: name.to_owned(),
                value: None,
            });
            continue;
        }
        at += 1;
        let value = &text[at..];
        let width = match value.as_bytes().first() {
            Some(quote @ (b'"' | b'\'')) => value[1..]
                .find(*quote as char)
                .map_or(value.len(), |end| end + 2),
            Some(b'{') => match_brace(value).unwrap_or(value.len()),
            _ => value.find(char::is_whitespace).unwrap_or(value.len()),
        };
        props.push(Prop {
            name: name.to_owned(),
            value: Some(value[..width].to_owned()),
        });
        at += width;
    }
    props
}

/// The fenced code spans of a page, which the walk copies untouched.
fn code_spans(text: &str) -> Vec<(usize, usize)> {
    let (document, _) = scan(text, SourceId(0));
    document
        .segments
        .iter()
        .filter_map(|segment| match segment {
            liyasa_core::document::Segment::Code { span, .. } => {
                Some((span.start as usize, span.end as usize))
            }
            _ => None,
        })
        .collect()
}

/// The front matter block and the offset the body starts at.
fn split_frontmatter(text: &str) -> (Option<&str>, usize) {
    let Some(rest) = text.strip_prefix("---\n") else {
        return (None, 0);
    };
    let mut at = 4usize;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return (Some(&text[4..at]), at + line.len());
        }
        at += line.len();
    }
    (None, 0)
}

/// The page's front matter, with keys renamed and exported constants appended.
///
/// The rewrite is textual so the author's key order and comments survive; a
/// round trip through a YAML value would alphabetize both away.
fn frontmatter(
    text: Option<&str>,
    exports: &[(String, String)],
    convert: &dyn Convert,
) -> Option<String> {
    if text.is_none() && exports.is_empty() {
        return None;
    }
    let rename = convert.frontmatter();
    let consumed = convert.consumed();
    let mut out = String::from("---\n");
    let mut dropping = false;
    for line in text.unwrap_or_default().split_inclusive('\n') {
        let indented = line.starts_with([' ', '\t', '-']);
        if indented {
            if !dropping {
                out.push_str(line);
            }
            continue;
        }
        dropping = false;
        let Some((key, rest)) = line.split_once(':') else {
            out.push_str(line);
            continue;
        };
        let name = key.trim();
        if consumed.contains(&name) {
            dropping = true;
            continue;
        }
        match rename.get(name) {
            Some(to) => out.push_str(&format!("{to}:{rest}")),
            None => out.push_str(line),
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    for (name, value) in exports {
        out.push_str(&format!("{name}: {value}\n"));
    }
    out.push_str("---\n");
    Some(out)
}

fn front_value(front: Option<&String>) -> serde_json::Value {
    let Some(text) = front else {
        return serde_json::Value::Null;
    };
    let body = text
        .strip_prefix("---\n")
        .and_then(|rest| rest.strip_suffix("---\n"))
        .unwrap_or(text);
    liyasa_core::yaml::parse_value(body, None).unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests;
