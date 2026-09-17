//! VER-12: a spec change to one operation flags exactly the pages that depend
//! on that operation, with the diff.
//!
//! The pages are parsed by the parser the build uses and their edges come from
//! the extractor the build calls, so "depends on that operation" means the edge
//! a build records — not one this test wrote by hand.

use liyasa_components::Registry;
use liyasa_core::components::ComponentInst;
use liyasa_core::document::{Block, BlockKind, DepTarget, Deps, Edge, EdgeOrigin, Node};
use liyasa_core::ids::{BuildId, Fingerprint, PageId, Route};
use liyasa_core::markdown::{Expanded, ExpansionRecord, ParseOptions, SpanMap};
use liyasa_core::verify::{DependencyExtractor, GraphStore};
use liyasa_verify::graph::{MemoryGraph, PageExtractor};
use liyasa_verify::sources::impact::PathImpact;
use liyasa_verify::sources::openapi::operation_changes;
use serde_json::json;

/// The `endpoint` component is what makes a page depend on an operation: it is
/// the component that carries `spec` and `operation`, and its `deps` hook mints
/// the `DepTarget::Operation` edge the build records.
const LIST_PETS: &str = "\
# List pets

:::endpoint{method=\"get\" path=\"/pets\" spec=\"petstore\" operation=\"listPets\"}
Takes a `limit`.
:::
";

const LIST_USERS: &str = "\
# List users

:::endpoint{method=\"get\" path=\"/users\" spec=\"petstore\" operation=\"listUsers\"}
Takes nothing.
:::
";

/// Collecting each component instance's `deps` into `Document::deps`.
///
/// `liyasa-markdown` leaves `deps` empty and says "attached by the build"
/// (`ast/mod.rs`), and nothing in `liyasa-build` attaches them —
/// `render/mod.rs` clones the empty default and `Component::deps` has no
/// caller outside `liyasa-components`' own tests. So this is the build step
/// that is missing, written out here: without it no `DepTarget::Operation`
/// edge exists anywhere and VER-12 has nothing to query. The edges themselves
/// come from the component, not from this test.
fn collect_component_deps(block: &Block, registry: &Registry, out: &mut Vec<Edge>) {
    if let BlockKind::Component { name, props, slots } = &block.kind
        && let Some(component) = registry.resolve(name)
    {
        out.extend(component.deps(&ComponentInst {
            name: name.clone(),
            props: props.clone(),
            children: Vec::new(),
            slots: slots.clone(),
            id: block.id,
            origin: block.origin.clone(),
        }));
    }
    for node in &block.children {
        if let Node::Block(child) = node {
            collect_component_deps(child, registry, out);
        }
    }
}

fn page(text: &str, ulid: &str) -> (PageId, Vec<Edge>) {
    let id = PageId::parse(ulid).expect("a ULID");
    let expanded = Expanded {
        text: text.to_owned(),
        map: SpanMap::default(),
        record: ExpansionRecord::default(),
    };
    let registry = Registry::builtins();
    let mut document = liyasa_markdown::parse(&expanded, &registry, &ParseOptions::default());
    assert!(
        !document.diagnostics.has_errors(),
        "{:#?}",
        document.diagnostics
    );
    let mut deps = Vec::new();
    collect_component_deps(&document.root, &registry, &mut deps);
    assert!(
        !deps.is_empty(),
        "the page declares an `endpoint`, so the component owes edges"
    );
    document.deps = Deps(deps);
    (
        id,
        PageExtractor::for_page(id).extract(&document, &expanded.record),
    )
}

/// `/pets` documents `listPets`; `/users` documents `listUsers`.
fn site() -> (MemoryGraph, PageId, PageId) {
    let graph = MemoryGraph::new();
    let build = BuildId(Fingerprint::of("b1"));
    let (pets, pet_edges) = page(LIST_PETS, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
    let (users, user_edges) = page(LIST_USERS, "01BX5ZZKBKACTAV9WEVGEMMVRY");
    graph
        .replace_page_edges(build, &Route::new("/pets"), &pet_edges)
        .expect("the graph accepts a page");
    graph
        .replace_page_edges(build, &Route::new("/users"), &user_edges)
        .expect("the graph accepts a page");
    (graph, pets, users)
}

fn spec(limit_required: bool) -> serde_json::Value {
    json!({
        "openapi": "3.1.0",
        "paths": {
            "/pets": { "get": {
                "operationId": "listPets",
                "parameters": [{ "name": "limit", "in": "query", "required": limit_required }],
                "responses": { "200": { "description": "ok" } }
            }},
            "/users": { "get": {
                "operationId": "listUsers",
                "responses": { "200": { "description": "ok" } }
            }}
        }
    })
}

fn owner(origin: &EdgeOrigin) -> PageId {
    let (EdgeOrigin::Block(page, _) | EdgeOrigin::Page(page)) = origin;
    *page
}

#[test]
fn the_extractor_records_the_operation_a_page_documents() {
    let (graph, pets, _) = site();
    let dependents = graph
        .dependents(&DepTarget::Operation {
            spec: "petstore".to_owned(),
            op: "listPets".to_owned(),
        })
        .expect("the graph answers");
    assert_eq!(dependents.len(), 1, "{dependents:#?}");
    assert_eq!(owner(&dependents[0]), pets);
}

#[test]
fn a_change_to_one_operation_flags_exactly_the_pages_that_depend_on_it() {
    let (graph, pets, users) = site();
    let changes = operation_changes("petstore", &spec(false), &spec(true));
    assert_eq!(changes.len(), 1, "one operation moved: {changes:#?}");
    assert_eq!(changes[0].op, "listPets");
    assert_eq!(changes[0].diff, ["parameters"]);

    let impacts = PathImpact::new(&graph)
        .operation_impact(&changes)
        .expect("the graph answers");
    assert_eq!(impacts.len(), 1);

    let flagged: Vec<PageId> = impacts[0]
        .blocks
        .iter()
        .map(|(origin, _)| owner(origin))
        .collect();
    assert_eq!(flagged, [pets], "the page documenting listPets");
    assert!(!flagged.contains(&users), "and not the other one");

    // The diff travels with the flag: a page is flagged *with* what moved.
    assert_eq!(impacts[0].change.diff, ["parameters"]);
    assert_eq!(impacts[0].change.spec, "petstore");
}

#[test]
fn a_spec_that_did_not_move_flags_nothing() {
    let (graph, _, _) = site();
    let changes = operation_changes("petstore", &spec(true), &spec(true));
    assert!(changes.is_empty());
    assert!(
        PathImpact::new(&graph)
            .operation_impact(&changes)
            .expect("the graph answers")
            .is_empty()
    );
}

#[test]
fn the_evidence_path_says_why_the_page_was_flagged() {
    let (graph, _, _) = site();
    let impacts = PathImpact::new(&graph)
        .operation_impact(&operation_changes("petstore", &spec(false), &spec(true)))
        .expect("the graph answers");
    let (_, path) = &impacts[0].blocks[0];
    assert_eq!(path.len(), 1, "the page documents the operation directly");
    assert_eq!(
        path[0].to,
        DepTarget::Operation {
            spec: "petstore".to_owned(),
            op: "listPets".to_owned()
        }
    );
}
