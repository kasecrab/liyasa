//! The four implementations the spike compares (PRD §7.5.2).
//!
//! Every engine produces the same HTML shape for a directive
//! (`<div class="<name>">`), so a corpus case's expectations do not depend on
//! which engine produced them.

use comrak::nodes::NodeValue;
use comrak::{Arena, Options};
use liyasa_core::document::Props;

use crate::corpus::{Case, ExpectedDiagnostic};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outputs {
    pub html: Option<String>,
    pub markdown: Option<String>,
    pub ast: Option<serde_json::Value>,
    pub source_document: Option<serde_json::Value>,
    pub diagnostics: Option<Vec<ExpectedDiagnostic>>,
}

pub trait Engine: Sync {
    fn name(&self) -> &'static str;
    /// What this engine can be held to; anything else is reported as skipped.
    fn produces(&self) -> &'static [&'static str];
    fn run(&self, case: &Case) -> Result<Outputs, String>;
}

pub fn all() -> Vec<Box<dyn Engine>> {
    vec![
        Box::new(ComrakDirective),
        Box::new(ComrakMarkers),
        Box::new(MarkdownRs),
        Box::new(PulldownCmark),
    ]
}

pub fn by_name(name: &str) -> Option<Box<dyn Engine>> {
    all().into_iter().find(|e| e.name() == name)
}

fn comrak_options(case: &Case) -> Options<'static> {
    let mut options = Options::default();
    let extension = &mut options.extension;
    extension.table = true;
    extension.strikethrough = true;
    extension.autolink = true;
    extension.tasklist = true;
    extension.footnotes = true;
    extension.description_lists = true;
    extension.multiline_block_quotes = true;
    extension.superscript = true;
    extension.subscript = true;
    extension.alerts = true;
    extension.front_matter_delimiter = Some("---".to_owned());
    if case.header.options.get("content.math").is_some_and(truthy) {
        extension.math_dollars = true;
        extension.math_code = true;
    }
    if case
        .header
        .options
        .get("content.wikilinks")
        .is_some_and(truthy)
    {
        extension.wikilinks_title_after_pipe = true;
    }
    if case.header.tags.iter().any(|t| t == "commonmark") {
        // The upstream suite is plain CommonMark: GFM extensions change its
        // expected output, so an imported case runs without them.
        options.extension = comrak::options::Extension::default();
    }
    // comrak always parses HTML blocks; `content.html` is enforced by Liyasa's
    // sanitizer pass over the Rendered AST, never by the parser (§7.5.1 item 3).
    options.render.r#unsafe = true;
    options
}

fn truthy(value: &serde_json::Value) -> bool {
    value.as_bool().unwrap_or(true)
}

/// The directive name and props parsed out of comrak's info string.
///
/// The info string is opaque to comrak, so a `-->` or a quote inside a prop
/// value cannot terminate anything: there is no comment to terminate.
pub fn parse_info(info: &str) -> (String, Props) {
    let name_len = info
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(info.len());
    let name = info[..name_len].to_owned();
    let props = crate::spike::scan::props_of(&info[name_len..]).unwrap_or_default();
    (name, props)
}

/// Candidate (e), the one this spike recommends: comrak's own container block
/// directive, with Liyasa parsing the info string.
pub struct ComrakDirective;

impl Engine for ComrakDirective {
    fn name(&self) -> &'static str {
        "comrak-directive"
    }

    fn produces(&self) -> &'static [&'static str] {
        &["html"]
    }

    fn run(&self, case: &Case) -> Result<Outputs, String> {
        let mut options = comrak_options(case);
        options.extension.block_directive = true;
        let arena = Arena::new();
        let root = comrak::parse_document(&arena, &case.source, &options);
        for node in root.descendants() {
            let mut data = node.data.borrow_mut();
            if let NodeValue::BlockDirective(directive) = &mut data.value {
                let (name, _props) = parse_info(&directive.info);
                directive.info = name;
            }
        }
        let mut html = String::new();
        comrak::format_html(root, &options, &mut html).map_err(|e| e.to_string())?;
        Ok(Outputs {
            html: Some(html),
            ..Outputs::default()
        })
    }
}

/// Candidate (a): the marker rewrite of §7.5.1.
pub struct ComrakMarkers;

impl Engine for ComrakMarkers {
    fn name(&self) -> &'static str {
        "comrak-markers"
    }

    fn produces(&self) -> &'static [&'static str] {
        &["html", "diagnostics"]
    }

    fn run(&self, case: &Case) -> Result<Outputs, String> {
        let options = comrak_options(case);
        let arena = Arena::new();
        let parsed = crate::spike::markers::parse(&arena, &case.source, &options, NONCE);
        let broken = crate::spike::markers::position_round_trip(&parsed, &case.source);
        if !broken.is_empty() {
            return Err(format!(
                "position composition failed: {}",
                broken.join("; ")
            ));
        }
        let mut html = String::new();
        comrak::format_html(parsed.root, &options, &mut html).map_err(|e| e.to_string())?;
        Ok(Outputs {
            html: Some(html),
            diagnostics: Some(
                parsed
                    .diagnostics
                    .iter()
                    .map(|d| ExpectedDiagnostic {
                        code: d.code.as_str().to_owned(),
                        line: None,
                        col: None,
                        message: None,
                    })
                    .collect(),
            ),
            ..Outputs::default()
        })
    }
}

/// A fixed nonce: the spike compares output, and a random one would make the
/// rewritten text differ between runs. Production generates 128 bits per build.
const NONCE: [u8; 16] = [0x5a; 16];

/// Candidate (c).
pub struct MarkdownRs;

impl Engine for MarkdownRs {
    fn name(&self) -> &'static str {
        "markdown-rs"
    }

    fn produces(&self) -> &'static [&'static str] {
        &["html"]
    }

    fn run(&self, case: &Case) -> Result<Outputs, String> {
        let options = if case.header.tags.iter().any(|t| t == "commonmark") {
            markdown::Options::default()
        } else {
            markdown::Options::gfm()
        };
        let html =
            markdown::to_html_with_options(&case.source, &options).map_err(|e| e.to_string())?;
        Ok(Outputs {
            html: Some(format!("{html}\n")),
            ..Outputs::default()
        })
    }
}

/// Candidate (d).
pub struct PulldownCmark;

impl Engine for PulldownCmark {
    fn name(&self) -> &'static str {
        "pulldown-cmark"
    }

    fn produces(&self) -> &'static [&'static str] {
        &["html"]
    }

    fn run(&self, case: &Case) -> Result<Outputs, String> {
        let mut options = pulldown_cmark::Options::empty();
        if !case.header.tags.iter().any(|t| t == "commonmark") {
            options.insert(pulldown_cmark::Options::ENABLE_TABLES);
            options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
            options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
            options.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
        }
        let parser = pulldown_cmark::Parser::new_ext(&case.source, options);
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, parser);
        Ok(Outputs {
            html: Some(html),
            ..Outputs::default()
        })
    }
}
