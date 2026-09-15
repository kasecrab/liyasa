//! The `mermaid` parse check (VER-02.4).
//!
//! This is a parse check, not a renderer: §6.12 puts Mermaid pre-rendering in
//! the companion runtime, and VER-03 needs the check itself to run without
//! one. So it asserts what a broken diagram in documentation actually gets
//! wrong — an unknown diagram type, a missing direction, an unclosed bracket
//! or quote, an arrow the grammar does not have, an empty body — rather than
//! reimplementing Mermaid's grammar (RFC 1303).

use std::time::Instant;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::net::BoxFut;
use liyasa_core::verify::{
    CheckOutcome, CheckResult, CheckSpec, Isolation, Runner, Sandbox, SecretSource,
};

use super::{code_of, fail, finish, ready, scrubber_for, skip};
use crate::core::scrub::Scrubber;

/// The diagram headers Mermaid 11 accepts, lowercased.
const DIAGRAMS: &[&str] = &[
    "graph",
    "flowchart",
    "sequencediagram",
    "classdiagram",
    "statediagram",
    "statediagram-v2",
    "erdiagram",
    "journey",
    "gantt",
    "pie",
    "quadrantchart",
    "requirementdiagram",
    "gitgraph",
    "c4context",
    "c4container",
    "c4component",
    "c4dynamic",
    "c4deployment",
    "mindmap",
    "timeline",
    "zenuml",
    "sankey-beta",
    "xychart-beta",
    "block-beta",
    "packet-beta",
    "kanban",
    "architecture-beta",
    "radar-beta",
    "treemap-beta",
];

/// Headers that must name a direction on the same line.
const NEEDS_DIRECTION: &[&str] = &["graph", "flowchart"];
const DIRECTIONS: &[&str] = &["tb", "td", "bt", "rl", "lr"];

/// Every arrow `sequenceDiagram` has, longest first so `->>` is not read as
/// `->` with a stray `>`.
const SEQUENCE_ARROWS: &[&str] = &[
    "--)", "-->>", "--x", "--X", "-->", "->>", "-)", "-x", "-X", "->", "--",
];

pub struct MermaidRunner;

impl MermaidRunner {
    pub const ID: &'static str = "mermaid";
}

impl Runner for MermaidRunner {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn languages(&self) -> &'static [&'static str] {
        &["mermaid"]
    }

    fn isolation(&self) -> Isolation {
        Isolation::InProcess
    }

    fn run<'a>(
        &'a self,
        spec: &'a CheckSpec,
        _sandbox: &'a dyn Sandbox,
        secrets: &'a dyn SecretSource,
    ) -> BoxFut<'a, CheckResult> {
        let started = Instant::now();
        let scrubber = scrubber_for(spec, secrets);
        let outcome = match code_of(spec) {
            Some((lang, source)) if lang.eq_ignore_ascii_case("mermaid") => {
                check(source, &scrubber)
            }
            Some((lang, _)) => skip(format!("the `mermaid` runner does not claim `{lang}`")),
            None => skip("the `mermaid` runner reads code blocks only"),
        };
        ready(finish(spec, Self::ID, outcome, started))
    }
}

fn check(source: &str, scrubber: &Scrubber) -> CheckOutcome {
    let lines = meaningful_lines(source);
    let Some((header_line, header)) = lines.first().copied() else {
        return CheckOutcome::Error(Diagnostic::new(
            code::E0601,
            "a `mermaid` block is empty, so there is no diagram to parse",
        ));
    };
    let mut problems = Vec::new();

    let (keyword, rest) = split_header(header);
    if !DIAGRAMS.contains(&keyword.to_ascii_lowercase().as_str()) {
        problems.push(format!(
            "line {header_line}: `{keyword}` is not a Mermaid diagram type"
        ));
    } else if NEEDS_DIRECTION.contains(&keyword.to_ascii_lowercase().as_str())
        && !DIRECTIONS.contains(&rest.trim().to_ascii_lowercase().as_str())
    {
        problems.push(format!(
            "line {header_line}: `{keyword}` needs a direction (TB, TD, BT, RL, or LR), found `{}`",
            rest.trim()
        ));
    }

    if lines.len() < 2 {
        problems.push(format!(
            "line {header_line}: the diagram has a header and no statements"
        ));
    }

    let sequence = keyword.eq_ignore_ascii_case("sequenceDiagram");
    for (number, text) in &lines[1.min(lines.len())..] {
        if let Some(problem) = unbalanced(&mask_connectors(text, keyword)) {
            problems.push(format!("line {number}: {problem}"));
        }
        if sequence && let Some(problem) = bad_sequence_arrow(text) {
            problems.push(format!("line {number}: {problem}"));
        }
    }

    if problems.is_empty() {
        CheckOutcome::Pass
    } else {
        fail(scrubber, problems.join("\n"))
    }
}

/// Numbered lines with blanks, `%%` comments, and `%%{…}%%` directives gone.
fn meaningful_lines(source: &str) -> Vec<(usize, &str)> {
    source
        .lines()
        .enumerate()
        .map(|(index, text)| (index + 1, text.trim()))
        .filter(|(_, text)| !text.is_empty() && !text.starts_with("%%"))
        .collect()
}

fn split_header(header: &str) -> (&str, &str) {
    // `flowchart LR`, but also `graph TD;` and `stateDiagram-v2`.
    let header = header.trim_end_matches(';');
    match header.find(char::is_whitespace) {
        Some(at) => (&header[..at], &header[at..]),
        None => (header, ""),
    }
}

/// Blanks out the connector tokens that spell a bracket without opening one:
/// `-)` and `--)` in a sequence diagram, and every entity-relationship
/// cardinality, which is written with braces (`||--o{`).
fn mask_connectors(text: &str, keyword: &str) -> String {
    let tokens: &[&str] = if keyword.eq_ignore_ascii_case("sequenceDiagram") {
        &["--)", "-)"]
    } else if keyword.eq_ignore_ascii_case("erDiagram") {
        &["}o", "}|", "o{", "|{", "||", "|o"]
    } else {
        return text.to_owned();
    };
    let mut out = text.to_owned();
    for token in tokens {
        out = out.replace(token, &" ".repeat(token.len()));
    }
    out
}

/// Brackets and quotes, counted in one pass. Text inside a quoted label is
/// not scanned for brackets, because `A["a [b]"]` is a valid node label.
fn unbalanced(text: &str) -> Option<String> {
    let mut stack: Vec<char> = Vec::new();
    let mut quote: Option<char> = None;
    for c in text.chars() {
        if let Some(open) = quote {
            if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '[' | '(' | '{' => stack.push(c),
            ']' | ')' | '}' => {
                let want = match c {
                    ']' => '[',
                    ')' => '(',
                    _ => '{',
                };
                match stack.pop() {
                    Some(open) if open == want => {}
                    Some(open) => {
                        return Some(format!("`{open}` is closed by `{c}`"));
                    }
                    None => return Some(format!("`{c}` closes nothing")),
                }
            }
            _ => {}
        }
    }
    if let Some(open) = quote {
        return Some(format!("a {open} quote is never closed"));
    }
    stack.pop().map(|open| format!("`{open}` is never closed"))
}

/// A `sequenceDiagram` message line whose arrow is not one Mermaid has.
fn bad_sequence_arrow(text: &str) -> Option<String> {
    // Only message lines carry arrows; keywords and blocks do not.
    const KEYWORDS: &[&str] = &[
        "participant",
        "actor",
        "activate",
        "deactivate",
        "note",
        "loop",
        "alt",
        "else",
        "opt",
        "par",
        "and",
        "critical",
        "option",
        "break",
        "rect",
        "end",
        "autonumber",
        "link",
        "links",
        "box",
        "create",
        "destroy",
    ];
    let first = text.split_whitespace().next().unwrap_or_default();
    if KEYWORDS.contains(&first.to_ascii_lowercase().as_str()) {
        return None;
    }
    if !text.contains(':') {
        // Not a message; nothing here claims to be an arrow.
        return None;
    }
    let head = text.split(':').next().unwrap_or_default();
    if SEQUENCE_ARROWS.iter().any(|arrow| head.contains(arrow)) {
        None
    } else if head.contains('-') || head.contains('>') {
        Some(format!("`{}` is not a sequence arrow", head.trim()))
    } else {
        None
    }
}
