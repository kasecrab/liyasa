//! The Rendered AST: comrak's tree turned into §7.16's `Document`.
//!
//! [`parse`] is the entry point §34.9 names. It rewrites leaf directives, hands
//! the text to comrak, walks what comes back, and then runs one pass per
//! question that needs page-wide context. The passes are separate because each
//! is a different kind of mistake to make.

pub mod abbr;
pub mod anchors;
pub mod build;
#[cfg(test)]
pub mod corpus;
pub mod fence;
pub mod identity;
pub mod pos;

#[cfg(test)]
mod tests;

use comrak::{Arena, Options};
use liyasa_core::Diagnostics;
use liyasa_core::components::ComponentRegistry;
use liyasa_core::document::{Deps, Document};
use liyasa_core::markdown::{Expanded, HtmlMode, ParseOptions};

use crate::directives::rewrite;

/// The comrak configuration of CM-30.
///
/// Raw HTML is always parsed and never filtered here: `content.html` is
/// enforced by Liyasa's sanitizer over the Rendered AST, so that one pass
/// decides what survives whichever syntax produced it (§7.5.1 item 3).
pub fn options(opts: &ParseOptions) -> Options<'static> {
    let mut options = Options::default();
    let extension = &mut options.extension;
    extension.table = true;
    extension.tasklist = true;
    extension.strikethrough = true;
    extension.autolink = true;
    extension.footnotes = true;
    extension.description_lists = true;
    extension.superscript = true;
    extension.subscript = true;
    extension.multiline_block_quotes = true;
    extension.alerts = true;
    extension.front_matter_delimiter = Some("---".to_owned());
    extension.block_directive = true;
    // CM-30 names `header_ids`; comrak 0.55 spells it `header_id_prefix` and it
    // only reaches comrak's own HTML renderer, which Liyasa never runs.
    // Anchors come from `ast::anchors`, which also owns de-duplication.
    extension.math_dollars = opts.math;
    extension.math_code = opts.math;
    extension.wikilinks_title_after_pipe = opts.wikilinks;
    options.render.r#unsafe = true;
    options
}

/// Expanded text to a Rendered AST, with positions composed back to source.
pub fn parse(
    expanded: &Expanded,
    registry: &dyn ComponentRegistry,
    opts: &ParseOptions,
) -> Document {
    let rewritten = rewrite::rewrite(expanded, opts.build_nonce);
    let mut diagnostics = rewritten.diagnostics;

    let comrak_options = options(opts);
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, &rewritten.text, &comrak_options);

    let mut builder = build::Builder {
        written: Default::default(),
        diagnostics: Diagnostics::new(),
        positions: pos::Positions::new(
            source_of(expanded),
            &rewritten.text,
            &rewritten.map,
            &expanded.map,
        ),
        table: &rewritten.table,
        containers: &rewritten.containers,
        text: &rewritten.text,
        opts,
    };
    let mut document = builder.document(root);
    diagnostics.extend(std::mem::take(&mut builder.diagnostics));

    abbr::apply(&mut document, &rewritten.abbreviations);
    crate::directives::slots::lift(&mut document, &mut diagnostics);
    identity::assign(&mut document, &mut diagnostics);
    crate::directives::validate::check(&mut document, registry, &builder.written, &mut diagnostics);
    if opts.html != HtmlMode::Allow {
        crate::sanitize::run(&mut document, opts.html, &mut diagnostics);
    }

    Document {
        // Component, asset, and link edges are attached by the build, which
        // knows the page's `PageId` and the route table; the parser has
        // neither (§14.12).
        deps: Deps::default(),
        root: document,
        diagnostics,
    }
}

/// Expanded offsets are not a source, but `Span` carries one.
fn source_of(expanded: &Expanded) -> liyasa_core::SourceId {
    expanded
        .map
        .0
        .iter()
        .find_map(|(_, _, origin)| origin.span.map(|span| span.source))
        .unwrap_or(liyasa_core::SourceId(0))
}
