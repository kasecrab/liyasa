use liyasa_core::document::EdgeKind;
use liyasa_core::ids::{BlockId, BuildId, FactId, Fingerprint, PageId, Route};
use liyasa_core::verify::ChangeKind;
use serde_json::json;

use super::*;

fn build_id() -> BuildId {
    BuildId(Fingerprint::of("b1"))
}

fn page(byte: u8) -> PageId {
    PageId(ulid::Ulid::from_bytes([byte; 16]))
}

fn block(byte: u8, id: &str) -> EdgeOrigin {
    EdgeOrigin::Block(page(byte), BlockId::explicit(id))
}

fn edge(from: EdgeOrigin, to: DepTarget, kind: EdgeKind) -> Edge {
    Edge { from, to, kind }
}

fn changed(fact: &str) -> FactChange {
    FactChange {
        fact: FactId::new(fact),
        old: None,
        new: None,
        kind: ChangeKind::Changed,
    }
}

/// `/pricing` reads the fact; `/index` links to `/pricing`; `/unrelated`
/// reads a different fact and links nowhere.
fn site() -> MemoryGraph {
    let graph = MemoryGraph::new();
    for (route, edges) in [
        (
            "/pricing",
            vec![edge(
                block(1, "price"),
                DepTarget::Fact(FactId::new("plan.pro.price")),
                EdgeKind::Reads,
            )],
        ),
        (
            "/index",
            vec![edge(
                block(0, "lead"),
                DepTarget::Page(Route::new("/pricing")),
                EdgeKind::Links,
            )],
        ),
        (
            "/unrelated",
            vec![edge(
                block(2, "other"),
                DepTarget::Fact(FactId::new("plan.free.price")),
                EdgeKind::Reads,
            )],
        ),
    ] {
        graph
            .replace_page_edges(build_id(), &Route::new(route), &edges)
            .expect("the graph accepts a page");
    }
    graph
}

fn origins(blocks: &[(EdgeOrigin, Vec<Edge>)]) -> Vec<&EdgeOrigin> {
    blocks.iter().map(|(origin, _)| origin).collect()
}

#[test]
fn a_fact_change_reaches_the_block_that_reads_it_and_the_page_that_links_there() {
    let graph = site();
    let impacts = PathImpact::new(&graph)
        .impact_of(&[changed("plan.pro.price")])
        .expect("the graph answers");

    assert_eq!(impacts.len(), 1);
    let blocks = &impacts[0].blocks;
    assert_eq!(origins(blocks), [&block(0, "lead"), &block(1, "price")]);

    let direct = &blocks[1].1;
    assert_eq!(direct.len(), 1, "one hop: the block reads the fact");
    assert_eq!(direct[0].kind, EdgeKind::Reads);

    // The evidence reads outward from the origin: this block links to
    // /pricing, and /pricing reads plan.pro.price.
    let indirect = &blocks[0].1;
    assert_eq!(indirect.len(), 2);
    assert_eq!(indirect[0].kind, EdgeKind::Links);
    assert_eq!(indirect[0].to, DepTarget::Page(Route::new("/pricing")));
    assert_eq!(indirect[1].kind, EdgeKind::Reads);
}

#[test]
fn a_page_that_reads_another_fact_is_not_reached() {
    let graph = site();
    let impacts = PathImpact::new(&graph)
        .impact_of(&[changed("plan.pro.price")])
        .expect("the graph answers");
    assert!(
        !origins(&impacts[0].blocks).contains(&&block(2, "other")),
        "{:#?}",
        impacts[0].blocks
    );
}

#[test]
fn a_fact_that_nothing_reads_is_answered_with_no_blocks() {
    let graph = site();
    let impacts = PathImpact::new(&graph)
        .impact_of(&[changed("plan.enterprise.price")])
        .expect("the graph answers");
    assert_eq!(impacts.len(), 1, "the change is still answered");
    assert!(impacts[0].blocks.is_empty());
}

#[test]
fn every_change_is_answered_in_the_order_it_was_given() {
    let graph = site();
    let changes = [changed("plan.free.price"), changed("plan.pro.price")];
    let impacts = PathImpact::new(&graph)
        .impact_of(&changes)
        .expect("the graph answers");
    let facts: Vec<&str> = impacts.iter().map(|i| i.change.fact.as_str()).collect();
    assert_eq!(facts, ["plan.free.price", "plan.pro.price"]);
}

#[test]
fn a_fact_change_also_reaches_a_page_that_documents_its_source() {
    let graph = site();
    graph
        .replace_page_edges(
            build_id(),
            &Route::new("/truth"),
            &[edge(
                block(3, "table"),
                DepTarget::Source("pricing".to_owned()),
                EdgeKind::Documents,
            )],
        )
        .expect("the graph accepts a page");
    let (sources, problems) = SourceSet::parse(&json!({
        "pricing": {
            "kind": "url",
            "url": "https://api.example.com/plans",
            "facts": { "plan.pro.price": "/plans/pro/price" }
        }
    }));
    assert!(problems.is_empty(), "{problems:#?}");

    let without = PathImpact::new(&graph)
        .impact_of(&[changed("plan.pro.price")])
        .expect("the graph answers");
    assert!(!origins(&without[0].blocks).contains(&&block(3, "table")));

    let with = PathImpact::with_sources(&graph, &sources)
        .impact_of(&[changed("plan.pro.price")])
        .expect("the graph answers");
    assert!(
        origins(&with[0].blocks).contains(&&block(3, "table")),
        "{:#?}",
        with[0].blocks
    );
}

#[test]
fn a_spec_change_reaches_exactly_the_pages_that_document_the_operation() {
    let graph = MemoryGraph::new();
    for (route, byte, op) in [("/api/pets", 4, "listPets"), ("/api/users", 5, "listUsers")] {
        graph
            .replace_page_edges(
                build_id(),
                &Route::new(route),
                &[edge(
                    block(byte, "operation"),
                    DepTarget::Operation {
                        spec: "petstore".to_owned(),
                        op: op.to_owned(),
                    },
                    EdgeKind::Documents,
                )],
            )
            .expect("the graph accepts a page");
    }

    let impacts = PathImpact::new(&graph)
        .operation_impact(&[OperationChange {
            spec: "petstore".to_owned(),
            op: "listPets".to_owned(),
            diff: vec!["parameters".to_owned()],
        }])
        .expect("the graph answers");

    assert_eq!(origins(&impacts[0].blocks), [&block(4, "operation")]);
    assert_eq!(impacts[0].change.diff, ["parameters"]);
}

#[test]
fn the_trait_call_refuses_a_graph_it_does_not_read() {
    let graph = site();
    let query = PathImpact::new(&graph);
    let changes = [changed("plan.pro.price")];

    let same = query.impact(&changes, &graph).expect("the held graph");
    assert_eq!(same, query.impact_of(&changes).expect("the same query"));

    let other = MemoryGraph::new();
    assert_eq!(
        query.impact(&changes, &other),
        Err(StoreError::Conflict),
        "a different store would answer about a different site"
    );
}

#[test]
fn the_routes_an_impact_reaches_are_the_pages_a_build_renders_again() {
    let graph = site();
    let impacts = PathImpact::new(&graph)
        .impact_of(&[changed("plan.pro.price")])
        .expect("the graph answers");
    assert_eq!(
        routes_of(&graph, &impacts[0].blocks).expect("the graph answers"),
        [Route::new("/index"), Route::new("/pricing")]
    );

    let untouched = PathImpact::new(&graph)
        .impact_of(&[changed("plan.enterprise.price")])
        .expect("the graph answers");
    assert!(
        routes_of(&graph, &untouched[0].blocks)
            .expect("the graph answers")
            .is_empty()
    );
}

/// A page that moved route between two builds is named by the current build's
/// route.
///
/// Run for both orderings of the two `BuildId`s. `rows` returns every build in
/// `BuildId` order and a `BuildId` is a fingerprint, so which of the two comes
/// first is a property of the hash rather than of which build was newer. With
/// one pair of tags a query that reads every build happens to answer correctly
/// and the test proves nothing — checking both orderings means one of the two
/// cases always exercises the wrong answer.
#[test]
fn a_page_that_moved_route_is_named_by_the_route_of_the_build_being_rendered() {
    for (first, second) in [("b1", "b2"), ("b2", "b1")] {
        let graph = MemoryGraph::new();
        for (tag, route) in [(first, "/pricing"), (second, "/plans")] {
            graph
                .replace_page_edges(
                    BuildId(Fingerprint::of(tag)),
                    &Route::new(route),
                    &[edge(
                        block(1, "price"),
                        DepTarget::Fact(FactId::new("plan.pro.price")),
                        EdgeKind::Reads,
                    )],
                )
                .expect("the graph accepts a page");
        }
        let impacts = PathImpact::new(&graph)
            .impact_of(&[changed("plan.pro.price")])
            .expect("the graph answers");
        assert_eq!(
            routes_of(&graph, &impacts[0].blocks).expect("the graph answers"),
            [Route::new("/plans")],
            "the route of the build written last, with `{first}` before `{second}`"
        );
    }
}

#[test]
fn an_empty_graph_names_no_routes() {
    let graph = MemoryGraph::new();
    assert!(
        routes_of(&graph, &[(block(1, "price"), vec![])])
            .expect("the graph answers")
            .is_empty()
    );
}
