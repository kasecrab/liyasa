//! §14.12 end to end: a page parsed by the parser the build uses, the edges it
//! implies, and the blocks a fact change reaches through them.
//!
//! `liyasa-verify`'s own tests build their ASTs by hand, which cannot catch an
//! AST shape the extractor does not expect. This one parses Markdown.

use liyasa_components::Registry;
use liyasa_core::document::{DepTarget, Edge, EdgeKind, EdgeOrigin};
use liyasa_core::ids::{BuildId, FactId, Fingerprint, PageId, Route};
use liyasa_core::markdown::{Expanded, ExpansionRecord, ParseOptions, SpanMap};
use liyasa_core::verify::{DependencyExtractor, GraphStore};
use liyasa_verify::graph::{MemoryGraph, PageExtractor};

const PRICING: &str = "\
# Pricing

Pro costs twenty dollars a month. See the [plans](/plans) and the
[reference](/reference#limits), or read the [terms](https://example.com/terms).

![The plan comparison](img/plans.svg)
";

const INDEX: &str = "\
# Liyasa

Start with [pricing](/pricing).
";

fn page_id(text: &str) -> PageId {
    PageId::parse(text).unwrap_or_else(|| panic!("`{text}` is a ULID"))
}

/// The page as the build hands it to the extractor: the AST the parser
/// produced, and the record the same expansion wrote.
fn edges_of(text: &str, facts: &[&str], page: PageId) -> Vec<Edge> {
    let expanded = Expanded {
        text: text.to_owned(),
        map: SpanMap::default(),
        record: ExpansionRecord {
            facts: facts.iter().copied().map(FactId::new).collect(),
            ..ExpansionRecord::default()
        },
    };
    let document =
        liyasa_markdown::parse(&expanded, &Registry::builtins(), &ParseOptions::default());
    PageExtractor::for_page(page).extract(&document, &expanded.record)
}

fn target(edges: &[Edge], kind: EdgeKind, to: DepTarget) -> &Edge {
    edges
        .iter()
        .find(|edge| edge.kind == kind && edge.to == to)
        .unwrap_or_else(|| panic!("no {kind:?} edge to {to:?} in {edges:#?}"))
}

#[test]
fn a_parsed_page_yields_one_edge_per_thing_it_depends_on() {
    let page = page_id("01ARZ3NDEKTSV4RRFFQ69G5FAV");
    let edges = edges_of(PRICING, &["plan.pro.price"], page);

    let fact = target(
        &edges,
        EdgeKind::Reads,
        DepTarget::Fact(FactId::new("plan.pro.price")),
    );
    assert_eq!(fact.from, EdgeOrigin::Page(page));

    for route in ["/plans", "/reference"] {
        let link = target(&edges, EdgeKind::Links, DepTarget::Page(Route::new(route)));
        assert!(
            matches!(link.from, EdgeOrigin::Block(owner, _) if owner == page),
            "a link belongs to the block it is written in"
        );
    }
    target(
        &edges,
        EdgeKind::Links,
        DepTarget::ExternalUrl("https://example.com/terms".to_owned()),
    );
    target(
        &edges,
        EdgeKind::Embeds,
        DepTarget::Asset("img/plans.svg".to_owned()),
    );
}

#[test]
fn a_fact_change_reaches_the_page_that_reads_it_and_the_page_that_links_there() {
    let pricing = page_id("01ARZ3NDEKTSV4RRFFQ69G5FAV");
    let index = page_id("01BX5ZZKBKACTAV9WEVGEMMVRZ");
    let build = BuildId(Fingerprint::of("a-build"));

    let graph = MemoryGraph::new();
    graph
        .replace_page_edges(
            build,
            &Route::new("/pricing"),
            &edges_of(PRICING, &["plan.pro.price"], pricing),
        )
        .expect("the pricing page's edges");
    graph
        .replace_page_edges(build, &Route::new("/index"), &edges_of(INDEX, &[], index))
        .expect("the index page's edges");

    let changed = DepTarget::Fact(FactId::new("plan.pro.price"));

    assert_eq!(
        graph.dependents(&changed).expect("dependents"),
        vec![EdgeOrigin::Page(pricing)],
        "one hop from the fact is the page whose expansion read it"
    );

    let reached = graph.paths_to(&changed).expect("paths");
    let origins: Vec<&EdgeOrigin> = reached.iter().map(|(origin, _)| origin).collect();
    assert!(origins.contains(&&EdgeOrigin::Page(pricing)));
    let (_, evidence) = reached
        .iter()
        .find(|(origin, _)| matches!(origin, EdgeOrigin::Block(owner, _) if *owner == index))
        .expect("the index block that links to pricing");
    assert_eq!(
        evidence
            .iter()
            .map(|edge| (edge.kind, edge.to.clone()))
            .collect::<Vec<_>>(),
        vec![
            (EdgeKind::Links, DepTarget::Page(Route::new("/pricing"))),
            (EdgeKind::Reads, changed),
        ],
        "the evidence reads from the block outward to the fact"
    );
}

#[test]
fn a_page_rebuilt_without_the_fact_stops_depending_on_it() {
    let pricing = page_id("01ARZ3NDEKTSV4RRFFQ69G5FAV");
    let graph = MemoryGraph::new();
    let route = Route::new("/pricing");
    let first = BuildId(Fingerprint::of("first"));
    let second = BuildId(Fingerprint::of("second"));

    graph
        .replace_page_edges(
            first,
            &route,
            &edges_of(PRICING, &["plan.pro.price"], pricing),
        )
        .expect("the first build");
    graph
        .replace_page_edges(second, &route, &edges_of(PRICING, &[], pricing))
        .expect("the second build");

    let changed = DepTarget::Fact(FactId::new("plan.pro.price"));
    assert!(graph.dependents(&changed).expect("dependents").is_empty());

    let diff = graph.diff(first, second).expect("diff");
    assert!(diff.added.is_empty());
    assert_eq!(diff.removed.len(), 1);
    assert_eq!(diff.removed[0].to, changed);
}
