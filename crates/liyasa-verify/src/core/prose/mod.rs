//! Prose lint (VER-61).
//!
//! Vale's rule format, read by Liyasa, run over the prose of a page. What a
//! rule can see is a list of [`Passage`]s, each carrying the text of one block
//! and the scope selector that block answers to, so `scope: heading` really
//! only reads headings and the default `text` scope never reads code.

pub mod bundled;
pub mod ini;
pub mod rule;

use liyasa_core::diagnostics::{Diagnostic, Severity, code};
use liyasa_core::document::{Block, BlockKind, Inline, Node};
use liyasa_core::ids::BlockId;
use liyasa_core::span::Span;

use super::spell::SpellChecker;
use ini::{Override, ValeIni};
use rule::{CapStyle, Level, Rule, RuleKind};

pub use ini::Section;
pub use rule::{Rule as ProseRule, RuleError};

/// One block's prose, and what kind of block it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passage {
    pub block: BlockId,
    pub span: Option<Span>,
    pub scope: Scope,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Heading,
    Paragraph,
    List,
    Table,
    BlockQuote,
    Code,
    Alt,
    Raw,
}

impl Scope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Heading => "heading",
            Self::Paragraph => "paragraph",
            Self::List => "list",
            Self::Table => "table",
            Self::BlockQuote => "blockquote",
            Self::Code => "code",
            Self::Alt => "alt",
            Self::Raw => "raw",
        }
    }

    /// Whether this passage answers a rule's `scope:` selector. Vale writes
    /// selectors as `heading`, `heading.h2`, `text`, or `~code`; the part
    /// before the first `.` is what decides, and `text` is everything that is
    /// not code or raw HTML.
    pub fn answers(self, selector: &str) -> bool {
        let selector = selector.trim();
        if let Some(rest) = selector.strip_prefix('~') {
            return !self.answers(rest);
        }
        let head = selector.split('.').next().unwrap_or(selector);
        match head {
            "" | "text" => !matches!(self, Self::Code | Self::Raw),
            "summary" => matches!(self, Self::Paragraph),
            other => other == self.as_str(),
        }
    }

    /// The scope a rule gets when it names none.
    pub fn is_default(self) -> bool {
        self.answers("text")
    }
}

/// One rule's complaint about one passage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: String,
    pub severity: Severity,
    pub message: String,
    pub block: BlockId,
    pub span: Option<Span>,
    /// Byte offset of the match inside the passage's text.
    pub at: usize,
    pub found: String,
    pub link: Option<String>,
}

impl Finding {
    pub fn diagnostic(&self) -> Diagnostic {
        let diagnostic = Diagnostic::new(code::W0631, format!("{}: {}", self.rule, self.message))
            .with_severity(self.severity);
        let diagnostic = match self.span {
            Some(span) => diagnostic.at(span),
            None => diagnostic,
        };
        match &self.link {
            Some(link) => diagnostic.help(format!("see {link}")),
            None => diagnostic,
        }
    }
}

/// A rule Liyasa parsed but does not run, which VER-61 sends to the Vale
/// binary in the companion runtime when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delegated {
    pub rule: String,
    pub extends: String,
}

#[derive(Debug, Default)]
pub struct Linter {
    rules: Vec<Rule>,
    ini: ValeIni,
}

impl Linter {
    pub fn new(rules: Vec<Rule>) -> Self {
        Self {
            rules,
            ini: ValeIni::default(),
        }
    }

    #[must_use]
    pub fn with_ini(mut self, ini: ValeIni) -> Self {
        self.ini = ini;
        self
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn ini(&self) -> &ValeIni {
        &self.ini
    }

    /// The rules Liyasa does not implement, so the caller can hand them to the
    /// companion runtime or report them as skipped.
    pub fn delegated(&self, path: &str) -> Vec<Delegated> {
        self.rules
            .iter()
            .filter(|rule| self.enabled(path, rule).is_some())
            .filter_map(|rule| match &rule.kind {
                RuleKind::Unsupported(extends) => Some(Delegated {
                    rule: rule.name.clone(),
                    extends: extends.clone(),
                }),
                _ => None,
            })
            .collect()
    }

    /// `None` when `.vale.ini` turned this rule off for this file; otherwise
    /// the level it runs at.
    fn enabled(&self, path: &str, rule: &Rule) -> Option<Level> {
        let level = match self.ini.override_for(path, &rule.name) {
            Some(Override::Off) => return None,
            Some(Override::Level(level)) => level,
            None => rule.level,
        };
        // A rule from a style the file's sections do not turn on still runs
        // when the config named no styles at all, which is what a project with
        // no `.vale.ini` gets.
        let styles = self.ini.styles_for(path);
        if !styles.is_empty() {
            let style = rule.name.split('.').next().unwrap_or_default();
            if !styles.contains(&style) {
                return None;
            }
        }
        self.ini.reports(level).then_some(level)
    }

    pub fn check(
        &self,
        path: &str,
        passages: &[Passage],
        speller: Option<&SpellChecker>,
    ) -> Vec<Finding> {
        let mut out = Vec::new();
        for rule in &self.rules {
            let Some(level) = self.enabled(path, rule) else {
                continue;
            };
            let severity = level.severity();
            let chosen: Vec<&Passage> = passages
                .iter()
                .filter(|passage| in_scope(rule, passage))
                .collect();
            match &rule.kind {
                RuleKind::Existence { pattern } => {
                    for passage in &chosen {
                        for found in pattern.find_iter(&passage.text) {
                            push(
                                &mut out,
                                rule,
                                severity,
                                passage,
                                found.start(),
                                found.as_str(),
                                None,
                            );
                        }
                    }
                }
                RuleKind::Substitution { pattern, swap } => {
                    for passage in &chosen {
                        for found in pattern.find_iter(&passage.text) {
                            let suggestion = swap
                                .iter()
                                .find(|(from, _)| from.is_match(found.as_str()))
                                .map(|(_, to)| to.as_str());
                            push(
                                &mut out,
                                rule,
                                severity,
                                passage,
                                found.start(),
                                found.as_str(),
                                suggestion,
                            );
                        }
                    }
                }
                RuleKind::Occurrence { pattern, max, min } => {
                    for passage in &chosen {
                        let hits: Vec<_> = pattern.find_iter(&passage.text).collect();
                        let over = max.is_some_and(|max| hits.len() > max);
                        let under = min.is_some_and(|min| hits.len() < min);
                        if over || under {
                            let at = hits.first().map_or(0, |m| m.start());
                            let found = hits.first().map_or("", |m| m.as_str());
                            push(&mut out, rule, severity, passage, at, found, None);
                        }
                    }
                }
                RuleKind::Capitalization { style, exceptions } => {
                    for passage in &chosen {
                        let text = passage.text.trim();
                        if text.is_empty() || exceptions.iter().any(|e| e == text) {
                            continue;
                        }
                        if !matches_capitalization(style, text, exceptions) {
                            push(&mut out, rule, severity, passage, 0, text, None);
                        }
                    }
                }
                RuleKind::Spelling {
                    ignore, filters, ..
                } => {
                    let Some(speller) = speller else {
                        continue;
                    };
                    for passage in &chosen {
                        for miss in speller.check(&passage.text) {
                            if filters.iter().any(|f| f.is_match(&miss.word))
                                || ignore.iter().any(|i| i.eq_ignore_ascii_case(&miss.word))
                            {
                                continue;
                            }
                            push(&mut out, rule, severity, passage, miss.at, &miss.word, None);
                        }
                    }
                }
                RuleKind::Consistency { either } => {
                    for (first, second, a, b) in either {
                        // Consistency is about the document, not one passage:
                        // both spellings anywhere in it is the finding.
                        let uses_first = chosen.iter().find(|p| first.is_match(&p.text));
                        let uses_second = chosen.iter().find(|p| second.is_match(&p.text));
                        if let (Some(_), Some(passage)) = (uses_first, uses_second) {
                            let at = second.find(&passage.text).map_or(0, |m| m.start());
                            let mut finding =
                                finding(rule, severity, passage, at, b, Some(a.as_str()));
                            finding.message = rule.message_for(b, Some(a));
                            out.push(finding);
                        }
                    }
                }
                RuleKind::Sequence { patterns } => {
                    for passage in &chosen {
                        out.extend(sequence_hits(rule, severity, passage, patterns));
                    }
                }
                RuleKind::Unsupported(_) => {}
            }
        }
        out.retain(|f| !rule_excepted(&self.rules, f));
        out
    }
}

fn in_scope(rule: &Rule, passage: &Passage) -> bool {
    if rule.scope.is_empty() {
        return passage.scope.is_default();
    }
    rule.scope.iter().any(|s| passage.scope.answers(s))
}

fn rule_excepted(rules: &[Rule], finding: &Finding) -> bool {
    rules
        .iter()
        .find(|r| r.name == finding.rule)
        .is_some_and(|rule| rule.excepted(&finding.found))
}

fn push(
    out: &mut Vec<Finding>,
    rule: &Rule,
    severity: Severity,
    passage: &Passage,
    at: usize,
    found: &str,
    suggestion: Option<&str>,
) {
    out.push(finding(rule, severity, passage, at, found, suggestion));
}

fn finding(
    rule: &Rule,
    severity: Severity,
    passage: &Passage,
    at: usize,
    found: &str,
    suggestion: Option<&str>,
) -> Finding {
    Finding {
        rule: rule.name.clone(),
        severity,
        message: rule.message_for(found, suggestion),
        block: passage.block,
        span: passage.span,
        at,
        found: found.to_owned(),
        link: rule.link.clone(),
    }
}

fn sequence_hits(
    rule: &Rule,
    severity: Severity,
    passage: &Passage,
    patterns: &[regex::Regex],
) -> Vec<Finding> {
    let words: Vec<(usize, &str)> = passage
        .text
        .split_whitespace()
        .map(|word| {
            let at = word.as_ptr() as usize - passage.text.as_ptr() as usize;
            (at, word)
        })
        .collect();
    let mut out = Vec::new();
    if patterns.is_empty() || words.len() < patterns.len() {
        return out;
    }
    for window in words.windows(patterns.len()) {
        if window
            .iter()
            .zip(patterns)
            .all(|((_, word), pattern)| pattern.is_match(word))
        {
            let at = window[0].0;
            let end = window[window.len() - 1].0 + window[window.len() - 1].1.len();
            out.push(finding(
                rule,
                severity,
                passage,
                at,
                &passage.text[at..end],
                None,
            ));
        }
    }
    out
}

fn matches_capitalization(style: &CapStyle, text: &str, exceptions: &[String]) -> bool {
    match style {
        CapStyle::Lower => text == text.to_lowercase(),
        CapStyle::Upper => text == text.to_uppercase(),
        CapStyle::Sentence => is_sentence_case(text, exceptions),
        CapStyle::Title => is_title_case(text, exceptions),
        CapStyle::Pattern(pattern) => regex::Regex::new(pattern)
            .map(|p| p.is_match(text))
            .unwrap_or(true),
    }
}

/// The first word is capitalized and the rest are not, unless they are an
/// exception or already all capitals (an acronym).
fn is_sentence_case(text: &str, exceptions: &[String]) -> bool {
    let mut words = text.split_whitespace();
    let Some(first) = words.next() else {
        return true;
    };
    if !starts_upper(first) && !is_exception(first, exceptions) {
        return false;
    }
    words.all(|word| !starts_upper(word) || is_acronym(word) || is_exception(word, exceptions))
}

/// Every word is capitalized except the short function words, which Vale's
/// `$title` treats the same way whichever style guide is named.
fn is_title_case(text: &str, exceptions: &[String]) -> bool {
    const MINOR: &[&str] = &[
        "a", "an", "and", "as", "at", "but", "by", "en", "for", "if", "in", "nor", "of", "on",
        "or", "per", "the", "to", "v", "via", "vs",
    ];
    let words: Vec<&str> = text.split_whitespace().collect();
    for (index, word) in words.iter().enumerate() {
        let minor = MINOR.contains(&word.to_lowercase().as_str());
        let first_or_last = index == 0 || index + 1 == words.len();
        if is_exception(word, exceptions) || is_acronym(word) {
            continue;
        }
        if minor && !first_or_last {
            continue;
        }
        if !starts_upper(word) {
            return false;
        }
    }
    true
}

fn starts_upper(word: &str) -> bool {
    word.chars()
        .find(|c| c.is_alphabetic())
        .is_none_or(char::is_uppercase)
}

fn is_acronym(word: &str) -> bool {
    let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
    letters.len() > 1 && letters.iter().all(|c| c.is_uppercase())
}

fn is_exception(word: &str, exceptions: &[String]) -> bool {
    let bare = word.trim_matches(|c: char| !c.is_alphanumeric());
    exceptions.iter().any(|e| e == bare || e == word)
}

/// Every passage of a page, in document order. Code blocks and raw HTML are
/// their own scopes so a `text`-scoped rule never reads them.
pub fn passages(root: &Block) -> Vec<Passage> {
    let mut out = Vec::new();
    collect(root, &mut out);
    out
}

fn collect(block: &Block, out: &mut Vec<Passage>) {
    let scope = scope_of(&block.kind);
    match &block.kind {
        BlockKind::CodeBlock { .. } | BlockKind::HtmlBlock { .. } | BlockKind::Math { .. } => {
            let text = match &block.kind {
                BlockKind::CodeBlock { .. } => text_of(block),
                BlockKind::HtmlBlock { html } => html.clone(),
                BlockKind::Math { src, .. } => src.clone(),
                _ => unreachable!(),
            };
            out.push(Passage {
                block: block.id,
                span: block.origin.span,
                scope,
                text,
            });
            return;
        }
        _ => {}
    }

    let text = text_of(block);
    if !text.trim().is_empty() {
        out.push(Passage {
            block: block.id,
            span: block.origin.span,
            scope,
            text,
        });
    }
    for alt in alt_texts(block) {
        out.push(Passage {
            block: block.id,
            span: block.origin.span,
            scope: Scope::Alt,
            text: alt,
        });
    }
    for child in &block.children {
        if let Node::Block(inner) = child {
            collect(inner, out);
        }
    }
}

const fn scope_of(kind: &BlockKind) -> Scope {
    match kind {
        BlockKind::Heading { .. } => Scope::Heading,
        BlockKind::List { .. } | BlockKind::ListItem { .. } => Scope::List,
        BlockKind::Table { .. } | BlockKind::TableRow { .. } | BlockKind::TableCell => Scope::Table,
        BlockKind::BlockQuote => Scope::BlockQuote,
        BlockKind::CodeBlock { .. } | BlockKind::Math { .. } => Scope::Code,
        BlockKind::HtmlBlock { .. } => Scope::Raw,
        _ => Scope::Paragraph,
    }
}

/// The inline text directly under a block. Inline code is dropped: it is code,
/// and a `text`-scoped rule must not read it.
fn text_of(block: &Block) -> String {
    let mut out = String::new();
    for child in &block.children {
        if let Node::Inline(inline) = child {
            write_inline(inline, &mut out);
        }
    }
    out.trim().to_owned()
}

fn write_inline(inline: &Inline, out: &mut String) {
    match inline {
        Inline::Text(text) => out.push_str(text),
        Inline::SoftBreak | Inline::HardBreak => out.push(' '),
        Inline::Emph(children)
        | Inline::Strong(children)
        | Inline::Strike(children)
        | Inline::Link { children, .. }
        | Inline::InlineComponent { children, .. } => {
            for child in children {
                write_inline(child, out);
            }
        }
        // Code, math, raw HTML, footnote markers, and unexpanded templates are
        // not prose.
        _ => {}
    }
}

fn alt_texts(block: &Block) -> Vec<String> {
    let mut out = Vec::new();
    for child in &block.children {
        if let Node::Inline(inline) = child {
            collect_alt(inline, &mut out);
        }
    }
    out
}

fn collect_alt(inline: &Inline, out: &mut Vec<String>) {
    match inline {
        Inline::Image { alt, .. } if !alt.trim().is_empty() => out.push(alt.clone()),
        Inline::Emph(children)
        | Inline::Strong(children)
        | Inline::Strike(children)
        | Inline::Link { children, .. }
        | Inline::InlineComponent { children, .. } => {
            for child in children {
                collect_alt(child, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
