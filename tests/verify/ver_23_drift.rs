//! VER-23, the half this package can answer: a snapshot change produces the
//! record of which blocks it reaches, with the old and new values, and the set
//! of routes a build has to render again.
//!
//! The requirement has three clauses and this file covers one and a half:
//!
//! * *"a drift record listing affected pages and blocks, with the old and new
//!   values"* — covered here. The record itself (`DriftEngine`, `DriftRepo`) is
//!   WP-20c's; what is asserted is the `Impact` it is built from.
//! * *"pages using the value through a template are rebuilt automatically"* —
//!   the *set* to rebuild is covered here, by `routes_of`. The rebuild is the
//!   build's, and it is not wired to any of this yet: nothing populates the
//!   `facts.*` layer (see `tests/verify/ver_21_deps.rs`).
//! * *"pages that mention the value in prose are found by the prose scanner"* —
//!   `ClaimScanner` is WP-20c's and is not implemented, so nothing here
//!   asserts it.
//!
//! VER-23 is therefore `partial`, and this file is the evidence for which part.

use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use liyasa_components::Registry;
use liyasa_core::conformance::block_on;
use liyasa_core::document::{DepTarget, Edge, EdgeKind, EdgeOrigin};
use liyasa_core::ids::{BuildId, FactId, Fingerprint, PageId, Route};
use liyasa_core::markdown::{Expanded, ExpansionRecord, ParseOptions, SpanMap};
use liyasa_core::net::{BoxFut, HttpClient, HttpPolicy, HttpRequest, HttpResponse, NetError};
use liyasa_core::verify::{ChangeKind, DependencyExtractor, FactValue, GraphStore};
use liyasa_verify::graph::{MemoryGraph, PageExtractor};
use liyasa_verify::sources::impact::{PathImpact, routes_of};
use liyasa_verify::sources::kinds::{BuildTrust, DeclaredSource};
use liyasa_verify::sources::refresh::Refresher;
use liyasa_verify::sources::snapshot::SnapshotLog;
use liyasa_verify::sources::spec::{SourceSet, SourceSpec};
use serde_json::json;

const PRICING: &str = "\
# Pricing

Pro costs {{ fact(\"plan.pro.price\") }} a month.
";

const INDEX: &str = "\
# Liyasa

Start with [pricing](/pricing).
";

const TRUTH: &str = "\
# Where our numbers come from

The [pricing API](/pricing) is the source.
";

struct Api(Mutex<serde_json::Value>);

impl HttpClient for Api {
    fn fetch<'a>(
        &'a self,
        req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        let body = self.0.lock().expect("not poisoned").to_string();
        Box::pin(std::future::ready(Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: body.into_bytes().into(),
            final_url: req.url,
        })))
    }
}

const DECLARATION: &str = r#"{
  "pricing": {
    "kind": "url",
    "url": "https://api.example.com/plans",
    "facts": { "plan.pro.price": "/price" }
  }
}"#;

fn source(at: SystemTime) -> DeclaredSource {
    let (set, problems) =
        SourceSet::parse(&serde_json::from_str(DECLARATION).expect("a declaration"));
    assert!(problems.is_empty(), "{problems:#?}");
    let spec: SourceSpec = set.get("pricing").expect("the source").clone();
    DeclaredSource::new(spec).taken_at(at)
}

fn sources() -> SourceSet {
    let (set, _) = SourceSet::parse(&serde_json::from_str(DECLARATION).expect("a declaration"));
    set
}

fn edges(text: &str, ulid: &str, facts: &[&str]) -> Vec<Edge> {
    let page = PageId::parse(ulid).expect("a ULID");
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

/// `/pricing` reads the fact through a template; `/index` links to `/pricing`;
/// `/truth` links there too and mentions the number in prose only.
fn site() -> MemoryGraph {
    let graph = MemoryGraph::new();
    let build = BuildId(Fingerprint::of("b1"));
    for (route, text, ulid, facts) in [
        (
            "/pricing",
            PRICING,
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            &["plan.pro.price"][..],
        ),
        ("/index", INDEX, "01BX5ZZKBKACTAV9WEVGEMMVRY", &[]),
        ("/truth", TRUTH, "01CZ5ZZKBKACTAV9WEVGEMMVRY", &[]),
    ] {
        graph
            .replace_page_edges(build, &Route::new(route), &edges(text, ulid, facts))
            .expect("the graph accepts a page");
    }
    graph
}

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

#[test]
fn a_snapshot_change_lists_the_blocks_it_reaches_with_the_old_and_new_values() {
    let api = Api(Mutex::new(json!({ "price": 20 })));
    let log = SnapshotLog::new();
    let refresher = Refresher::new(&log, BuildTrust::Trusted, "main");
    block_on(refresher.refresh(&[source(at(0))], &api, None, at(0)));

    *api.0.lock().expect("not poisoned") = json!({ "price": 25 });
    let second = block_on(refresher.refresh(&[source(at(3600))], &api, None, at(3600)));
    assert_eq!(second.changes.len(), 1);

    let graph = site();
    let set = sources();
    let impacts = PathImpact::with_sources(&graph, &set)
        .impact_of(&second.changes)
        .expect("the graph answers");

    assert_eq!(impacts.len(), 1);
    let impact = &impacts[0];
    assert_eq!(impact.change.fact, FactId::new("plan.pro.price"));
    assert_eq!(impact.change.kind, ChangeKind::Changed);
    assert_eq!(impact.change.old, Some(FactValue::Num(20.0)));
    assert_eq!(impact.change.new, Some(FactValue::Num(25.0)));

    // Every affected block, each with the path that proves it is affected.
    assert!(!impact.blocks.is_empty());
    for (origin, path) in &impact.blocks {
        assert!(!path.is_empty(), "{origin:?} has no evidence");
        assert_eq!(
            path.last().map(|edge| &edge.to),
            Some(&DepTarget::Fact(FactId::new("plan.pro.price"))),
            "every path ends at the fact that changed"
        );
    }
}

#[test]
fn the_pages_to_render_again_are_the_ones_the_change_reaches() {
    let graph = site();
    let change = liyasa_core::verify::FactChange {
        fact: FactId::new("plan.pro.price"),
        old: Some(FactValue::Num(20.0)),
        new: Some(FactValue::Num(25.0)),
        kind: ChangeKind::Changed,
    };
    let impacts = PathImpact::new(&graph)
        .impact_of(std::slice::from_ref(&change))
        .expect("the graph answers");
    let routes = routes_of(&graph, &impacts[0].blocks).expect("the graph answers");

    // `/pricing` reads the fact; `/index` and `/truth` link to `/pricing`, so
    // a change there reaches them too.
    assert_eq!(
        routes,
        [
            Route::new("/index"),
            Route::new("/pricing"),
            Route::new("/truth")
        ]
    );
}

#[test]
fn a_page_that_only_mentions_the_number_in_prose_has_no_template_edge() {
    // This is the gap VER-23 hands to the prose scanner: `/truth` is reached
    // here only because it links to `/pricing`. Nothing in the graph says it
    // states the number, because nothing in its source does.
    let graph = site();
    let direct = graph
        .dependents(&DepTarget::Fact(FactId::new("plan.pro.price")))
        .expect("the graph answers");
    assert_eq!(direct.len(), 1, "{direct:#?}");
    let (EdgeOrigin::Block(page, _) | EdgeOrigin::Page(page)) = &direct[0];
    assert_eq!(
        *page,
        PageId::parse("01ARZ3NDEKTSV4RRFFQ69G5FAV").expect("a ULID"),
        "only the page with the template reads the fact"
    );

    // And the link edges that carried the other two are `Links`, not `Reads`.
    let reaching = graph
        .dependents(&DepTarget::Page(Route::new("/pricing")))
        .expect("the graph answers");
    assert_eq!(reaching.len(), 2);
    for origin in &reaching {
        let edges = graph.dependencies(origin).expect("the graph answers");
        assert!(edges.iter().any(|edge| edge.kind == EdgeKind::Links
            && edge.to == DepTarget::Page(Route::new("/pricing"))));
    }
}
