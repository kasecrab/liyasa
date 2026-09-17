//! VER-21: referencing a fact records a dependency from the block to the fact
//! and from the fact to its source, and a missing fact fails the build with
//! `E0209`.
//!
//! The page is parsed by the parser the build uses and the edges come from the
//! extractor the build calls, so what is asserted is the edge a build would
//! record rather than one this test minted.

use std::sync::Arc;

use liyasa_components::Registry;
use liyasa_core::document::{DepTarget, Edge, EdgeKind};
use liyasa_core::ids::{BuildId, FactId, Fingerprint, PageId, Route};
use liyasa_core::markdown::{Expanded, ExpansionRecord, ParseOptions, SpanMap};
use liyasa_core::source_map::SourceMap;
use liyasa_core::verify::{DependencyExtractor, GraphStore};
use liyasa_core::vfs::VfsPath;
use liyasa_markdown::source::expand::ExpandOptions;
use liyasa_markdown::source::{Layers, environment, expand, scan};
use liyasa_verify::graph::{MemoryGraph, PageExtractor};
use liyasa_verify::sources::spec::SourceSet;
use serde_json::json;

const PAGE: &str = "# Pricing\n\nPro costs {{ fact(\"plan.pro.price\") }} a month.\n";

fn page_id() -> PageId {
    PageId::parse("01ARZ3NDEKTSV4RRFFQ69G5FAV").expect("a ULID")
}

/// The edges the build records for a page that read these facts.
fn edges(facts: &[&str]) -> Vec<Edge> {
    let expanded = Expanded {
        text: PAGE.to_owned(),
        map: SpanMap::default(),
        record: ExpansionRecord {
            facts: facts.iter().copied().map(FactId::new).collect(),
            ..ExpansionRecord::default()
        },
    };
    let document =
        liyasa_markdown::parse(&expanded, &Registry::builtins(), &ParseOptions::default());
    PageExtractor::for_page(page_id()).extract(&document, &expanded.record)
}

/// The codes expanding this page raises against a given `facts.*` layer.
fn codes(facts: serde_json::Value) -> Vec<String> {
    let mut map = SourceMap::new();
    let id = map.intern(VfsPath::new("pricing.md"), Arc::from(PAGE));
    let (document, diagnostics) = scan(PAGE, id);
    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
    let context = Layers {
        facts,
        ..Layers::default()
    }
    .build();
    match expand(
        &map,
        &document,
        &context,
        &environment(&ExpandOptions::default()),
    ) {
        Ok(_) => Vec::new(),
        Err(problems) => problems
            .iter()
            .map(|d| d.code.as_str().to_owned())
            .collect(),
    }
}

#[test]
fn referencing_a_fact_records_an_edge_from_the_block_to_the_fact() {
    let graph = MemoryGraph::new();
    graph
        .replace_page_edges(
            BuildId(Fingerprint::of("b1")),
            &Route::new("/pricing"),
            &edges(&["plan.pro.price"]),
        )
        .expect("the graph accepts the page");

    let dependents = graph
        .dependents(&DepTarget::Fact(FactId::new("plan.pro.price")))
        .expect("the graph answers");
    assert_eq!(dependents.len(), 1, "{dependents:#?}");

    let from_page = graph
        .dependencies(&dependents[0])
        .expect("the graph answers");
    assert!(
        from_page.iter().any(|edge| {
            edge.kind == EdgeKind::Reads
                && edge.to == DepTarget::Fact(FactId::new("plan.pro.price"))
        }),
        "{from_page:#?}"
    );
}

#[test]
fn the_fact_knows_the_source_it_came_from() {
    let (sources, problems) = SourceSet::parse(&json!({
        "pricing": {
            "kind": "url",
            "url": "https://api.example.com/plans",
            "facts": { "plan.pro.price": "/plans/pro/price" }
        }
    }));
    assert!(problems.is_empty(), "{problems:#?}");
    assert_eq!(
        sources.source_of(&FactId::new("plan.pro.price")),
        Some("pricing")
    );
    // A fact no declaration produces has no source rather than a made-up one.
    assert_eq!(sources.source_of(&FactId::new("plan.free.price")), None);
}

#[test]
fn a_fact_that_does_not_exist_fails_the_build_with_e0209() {
    assert!(
        codes(json!({ "plan": { "pro": { "price": 20 } } })).is_empty(),
        "the fact resolves when it is there"
    );
    assert_eq!(codes(json!({ "plan": { "pro": {} } })), ["E0209"]);
    assert_eq!(codes(json!({ "plan": {} })), ["E0209"]);
}

/// A page with no `facts` layer at all does fail the build, but with `E0201`
/// rather than `E0209`: `fact()` reads `facts` out of the context and walks it,
/// and under the build's strict undefined behaviour the walk fails before the
/// filter can say which fact was meant.
///
/// This is `liyasa-markdown`'s diagnostic, not this package's, and it is
/// recorded here rather than asserted as `E0209` because it is what a build
/// actually reports — and today, with nothing populating the layer, it is what
/// every fact reference reports.
#[test]
fn with_no_facts_at_all_the_build_still_fails_though_not_with_e0209() {
    assert_eq!(codes(json!(null)), ["E0201"]);
}
