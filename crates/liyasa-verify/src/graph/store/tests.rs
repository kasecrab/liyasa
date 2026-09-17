use liyasa_core::ids::{BlockId, FactId, Fingerprint};

use super::*;

fn build(tag: &str) -> BuildId {
    BuildId(Fingerprint::of(tag))
}

fn page(byte: u8) -> PageId {
    PageId(ulid::Ulid::from_bytes([byte; 16]))
}

fn block(page_byte: u8, id: &str) -> EdgeOrigin {
    EdgeOrigin::Block(page(page_byte), BlockId::explicit(id))
}

fn reads(from: EdgeOrigin, fact: &str) -> Edge {
    Edge {
        from,
        to: DepTarget::Fact(FactId::new(fact)),
        kind: EdgeKind::Reads,
    }
}

fn links(from: EdgeOrigin, route: &str) -> Edge {
    Edge {
        from,
        to: DepTarget::Page(Route::new(route)),
        kind: EdgeKind::Links,
    }
}

#[test]
fn a_target_names_every_origin_that_reads_it() {
    let graph = MemoryGraph::new();
    let one = block(1, "price");
    let other = block(2, "price");
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/pricing"),
            &[reads(one.clone(), "plan.pro.price")],
        )
        .expect("write");
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/plans"),
            &[reads(other.clone(), "plan.pro.price")],
        )
        .expect("write");

    assert_eq!(
        graph
            .dependents(&DepTarget::Fact(FactId::new("plan.pro.price")))
            .expect("query"),
        vec![one, other]
    );
}

#[test]
fn an_unknown_target_has_no_dependents() {
    let graph = MemoryGraph::new();
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/a"),
            &[reads(block(1, "x"), "a.b")],
        )
        .expect("write");

    assert!(
        graph
            .dependents(&DepTarget::Fact(FactId::new("nothing.reads.this")))
            .expect("query")
            .is_empty()
    );
}

#[test]
fn an_empty_store_answers_rather_than_failing() {
    let graph = MemoryGraph::new();

    assert!(
        graph
            .dependents(&DepTarget::Asset("a.png".to_owned()))
            .expect("query")
            .is_empty()
    );
    assert!(
        graph
            .dependencies(&block(1, "x"))
            .expect("query")
            .is_empty()
    );
    assert_eq!(graph.current_build().expect("current"), None);
}

#[test]
fn an_origin_names_every_edge_it_owns() {
    let graph = MemoryGraph::new();
    let origin = block(1, "intro");
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/pricing"),
            &[
                links(origin.clone(), "/plans"),
                reads(origin.clone(), "plan.pro.price"),
                reads(block(1, "other"), "plan.free.price"),
            ],
        )
        .expect("write");

    assert_eq!(
        graph.dependencies(&origin).expect("query"),
        vec![
            reads(origin.clone(), "plan.pro.price"),
            links(origin, "/plans"),
        ]
    );
}

#[test]
fn rewriting_a_page_drops_the_edges_it_no_longer_has() {
    let graph = MemoryGraph::new();
    let origin = block(1, "intro");
    let route = Route::new("/pricing");
    graph
        .replace_page_edges(
            build("b1"),
            &route,
            &[reads(origin.clone(), "plan.pro.price")],
        )
        .expect("write");
    graph
        .replace_page_edges(build("b1"), &route, &[links(origin.clone(), "/plans")])
        .expect("write");

    assert_eq!(
        graph.dependencies(&origin).expect("query"),
        vec![links(origin, "/plans")]
    );
}

#[test]
fn a_page_that_is_rewritten_does_not_disturb_another_page() {
    let graph = MemoryGraph::new();
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/a"),
            &[reads(block(1, "x"), "a.b")],
        )
        .expect("write");
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/b"),
            &[reads(block(2, "x"), "a.b")],
        )
        .expect("write");
    graph
        .replace_page_edges(build("b1"), &Route::new("/a"), &[])
        .expect("write");

    assert_eq!(
        graph
            .dependents(&DepTarget::Fact(FactId::new("a.b")))
            .expect("query"),
        vec![block(2, "x")]
    );
}

#[test]
fn the_point_queries_read_the_build_most_recently_written() {
    let graph = MemoryGraph::new();
    let route = Route::new("/pricing");
    graph
        .replace_page_edges(
            build("old"),
            &route,
            &[reads(block(1, "x"), "plan.pro.price")],
        )
        .expect("write");
    graph
        .replace_page_edges(
            build("new"),
            &route,
            &[reads(block(1, "x"), "plan.free.price")],
        )
        .expect("write");

    assert_eq!(graph.current_build().expect("current"), Some(build("new")));
    assert!(
        graph
            .dependents(&DepTarget::Fact(FactId::new("plan.pro.price")))
            .expect("query")
            .is_empty()
    );
}

#[test]
fn a_diff_names_what_the_new_build_added_and_what_it_dropped() {
    let graph = MemoryGraph::new();
    let route = Route::new("/pricing");
    let origin = block(1, "intro");
    graph
        .replace_page_edges(
            build("old"),
            &route,
            &[
                reads(origin.clone(), "plan.pro.price"),
                links(origin.clone(), "/plans"),
            ],
        )
        .expect("write");
    graph
        .replace_page_edges(
            build("new"),
            &route,
            &[
                links(origin.clone(), "/plans"),
                links(origin.clone(), "/support"),
            ],
        )
        .expect("write");

    assert_eq!(
        graph.diff(build("old"), build("new")).expect("diff"),
        GraphDiff {
            added: vec![links(origin.clone(), "/support")],
            removed: vec![reads(origin, "plan.pro.price")],
        }
    );
}

#[test]
fn a_diff_against_a_build_the_store_never_saw_is_not_a_full_rebuild() {
    let graph = MemoryGraph::new();
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/a"),
            &[reads(block(1, "x"), "a.b")],
        )
        .expect("write");

    assert_eq!(
        graph.diff(build("never"), build("b1")),
        Err(StoreError::NotFound)
    );
}

#[test]
fn a_fact_reaches_a_page_that_only_links_to_the_page_that_reads_it() {
    let graph = MemoryGraph::new();
    let reader = block(1, "price");
    let linker = block(2, "see-also");
    let read = reads(reader.clone(), "plan.pro.price");
    let link = links(linker.clone(), "/pricing");
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/pricing"),
            std::slice::from_ref(&read),
        )
        .expect("write");
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/plans"),
            std::slice::from_ref(&link),
        )
        .expect("write");

    let paths = graph
        .paths_to(&DepTarget::Fact(FactId::new("plan.pro.price")))
        .expect("walk");

    assert_eq!(
        paths,
        vec![(reader, vec![read.clone()]), (linker, vec![link, read]),],
        "the evidence path reads from the block outward to the fact"
    );
}

#[test]
fn a_cycle_between_two_pages_terminates() {
    let graph = MemoryGraph::new();
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/a"),
            &[reads(block(1, "x"), "a.b"), links(block(1, "x"), "/b")],
        )
        .expect("write");
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/b"),
            &[links(block(2, "y"), "/a")],
        )
        .expect("write");

    let paths = graph
        .paths_to(&DepTarget::Fact(FactId::new("a.b")))
        .expect("walk");

    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|(_, path)| path.len() <= 3));
}

#[test]
fn the_table_carries_the_build_and_the_page_of_every_edge() {
    let graph = MemoryGraph::new();
    let edge = reads(block(1, "x"), "a.b");
    graph
        .replace_page_edges(build("b1"), &Route::new("/a"), std::slice::from_ref(&edge))
        .expect("write");

    assert_eq!(
        graph.rows().expect("rows"),
        vec![DependencyRecord {
            build: build("b1"),
            page: Route::new("/a"),
            edge,
        }]
    );
}

#[test]
fn a_page_bound_to_a_route_by_the_caller_wins_over_what_the_edges_implied() {
    let graph = MemoryGraph::new();
    // The edge originates on a page other than the one being written, which is
    // what `Document::deps` carries when a component names another page.
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/plans"),
            &[reads(block(1, "x"), "a.b")],
        )
        .expect("write");
    graph.bind(page(1), Route::new("/pricing")).expect("bind");
    graph
        .replace_page_edges(
            build("b1"),
            &Route::new("/index"),
            &[links(block(2, "y"), "/pricing")],
        )
        .expect("write");

    let paths = graph
        .paths_to(&DepTarget::Fact(FactId::new("a.b")))
        .expect("walk");

    assert_eq!(
        paths.len(),
        2,
        "the bound route is the one the walk follows"
    );
}
