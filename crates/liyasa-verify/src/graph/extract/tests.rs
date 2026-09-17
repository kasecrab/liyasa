use std::collections::BTreeMap;

use liyasa_core::diagnostics::Diagnostics;
use liyasa_core::document::{Deps, Origin, Props, Slots};
use liyasa_core::ids::{BlockId, FactId};
use liyasa_core::span::{SourceId, Span};

use super::*;

fn page_id(byte: u8) -> PageId {
    PageId(ulid::Ulid::from_bytes([byte; 16]))
}

fn block(id: &str, kind: BlockKind, children: Vec<Node>) -> Block {
    Block {
        id: BlockId::explicit(id),
        explicit_id: Some(id.to_owned()),
        kind,
        origin: Origin::default(),
        children,
    }
}

fn text(value: &str) -> Node {
    Node::Inline(Inline::Text(value.to_owned()))
}

fn link(href: &str, resolved: Option<&str>) -> Node {
    Node::Inline(Inline::Link {
        href: href.to_owned(),
        title: None,
        children: vec![Inline::Text("go".to_owned())],
        resolved: resolved.map(Route::new),
    })
}

fn document(root: Block) -> Document {
    Document {
        root,
        deps: Deps::default(),
        diagnostics: Diagnostics::new(),
    }
}

fn root(children: Vec<Node>) -> Block {
    block("root", BlockKind::Document, children)
}

fn extract(doc: &Document, record: &ExpansionRecord) -> Vec<Edge> {
    PageExtractor::for_page(page_id(1)).extract(doc, record)
}

fn targets(edges: &[Edge]) -> Vec<(EdgeKind, DepTarget)> {
    edges.iter().map(|e| (e.kind, e.to.clone())).collect()
}

#[test]
fn a_resolved_link_is_an_edge_to_the_route_it_resolved_to() {
    let doc = document(root(vec![Node::Block(block(
        "p",
        BlockKind::Paragraph,
        vec![link("install", Some("/guides/install"))],
    ))]));

    let edges = extract(&doc, &ExpansionRecord::default());

    assert_eq!(
        targets(&edges),
        vec![(
            EdgeKind::Links,
            DepTarget::Page(Route::new("/guides/install"))
        )]
    );
    assert_eq!(
        edges[0].from,
        EdgeOrigin::Block(page_id(1), BlockId::explicit("p"))
    );
}

#[test]
fn an_unresolved_link_keeps_the_page_half_of_the_href() {
    let doc = document(root(vec![Node::Block(block(
        "p",
        BlockKind::Paragraph,
        vec![
            link("/guides/install#step-2", None),
            link("/reference?tab=cli", None),
        ],
    ))]));

    assert_eq!(
        targets(&extract(&doc, &ExpansionRecord::default())),
        vec![
            (
                EdgeKind::Links,
                DepTarget::Page(Route::new("/guides/install"))
            ),
            (EdgeKind::Links, DepTarget::Page(Route::new("/reference"))),
        ]
    );
}

#[test]
fn a_link_that_names_only_a_position_on_this_page_is_not_a_dependency() {
    let doc = document(root(vec![Node::Block(block(
        "p",
        BlockKind::Paragraph,
        vec![
            link("#anchor", None),
            link("?tab=cli", None),
            link("mailto:docs@example.com", None),
            link("tel:+15550100", None),
        ],
    ))]));

    assert!(extract(&doc, &ExpansionRecord::default()).is_empty());
}

#[test]
fn an_absolute_link_is_an_external_url() {
    let doc = document(root(vec![Node::Block(block(
        "p",
        BlockKind::Paragraph,
        vec![link("https://example.com/a", None)],
    ))]));

    assert_eq!(
        targets(&extract(&doc, &ExpansionRecord::default())),
        vec![(
            EdgeKind::Links,
            DepTarget::ExternalUrl("https://example.com/a".to_owned())
        )]
    );
}

#[test]
fn an_image_embeds_both_its_light_and_its_dark_source() {
    let doc = document(root(vec![Node::Block(block(
        "p",
        BlockKind::Paragraph,
        vec![Node::Inline(Inline::Image {
            src: "img/logo.svg".to_owned(),
            alt: "logo".to_owned(),
            title: None,
            dark: Some("img/logo-dark.svg".to_owned()),
        })],
    ))]));

    assert_eq!(
        targets(&extract(&doc, &ExpansionRecord::default())),
        vec![
            (
                EdgeKind::Embeds,
                DepTarget::Asset("img/logo-dark.svg".to_owned())
            ),
            (
                EdgeKind::Embeds,
                DepTarget::Asset("img/logo.svg".to_owned())
            ),
        ]
    );
}

#[test]
fn a_component_documents_itself_and_its_slots_are_walked() {
    let doc = document(root(vec![Node::Block(Block {
        id: BlockId::explicit("card"),
        explicit_id: Some("card".to_owned()),
        kind: BlockKind::Component {
            name: "card".to_owned(),
            props: Props::default(),
            slots: Slots(BTreeMap::from([(
                "footer".to_owned(),
                vec![Node::Block(block(
                    "inner",
                    BlockKind::Paragraph,
                    vec![link("/pricing", Some("/pricing"))],
                ))],
            )])),
        },
        origin: Origin::default(),
        children: Vec::new(),
    })]));

    let edges = extract(&doc, &ExpansionRecord::default());

    assert_eq!(edges.len(), 2);
    let component = edges
        .iter()
        .find(|edge| edge.kind == EdgeKind::Documents)
        .expect("the component edge");
    assert_eq!(component.to, DepTarget::Component("card".to_owned()));
    assert_eq!(
        component.from,
        EdgeOrigin::Block(page_id(1), BlockId::explicit("card"))
    );
    let link = edges
        .iter()
        .find(|edge| edge.kind == EdgeKind::Links)
        .expect("the link inside the slot");
    assert_eq!(link.to, DepTarget::Page(Route::new("/pricing")));
    assert_eq!(
        link.from,
        EdgeOrigin::Block(page_id(1), BlockId::explicit("inner")),
        "a slot's content belongs to the block it is written in, not to the component"
    );
}

#[test]
fn an_inline_component_inside_emphasis_is_still_found() {
    let doc = document(root(vec![Node::Block(block(
        "p",
        BlockKind::Paragraph,
        vec![Node::Inline(Inline::Strong(vec![
            Inline::InlineComponent {
                name: "badge".to_owned(),
                props: Props::default(),
                children: vec![Inline::Text("new".to_owned())],
            },
        ]))],
    ))]));

    assert_eq!(
        targets(&extract(&doc, &ExpansionRecord::default())),
        vec![(
            EdgeKind::Documents,
            DepTarget::Component("badge".to_owned())
        )]
    );
}

#[test]
fn the_expansion_record_reads_facts_and_includes_snippets_for_the_page() {
    let record = ExpansionRecord {
        facts: ["plan.pro.price", "plan.free.price"]
            .into_iter()
            .map(FactId::new)
            .collect(),
        includes: vec![SourceId(7)],
        ..ExpansionRecord::default()
    };

    let edges = extract(&document(root(vec![text("body")])), &record);

    assert_eq!(
        targets(&edges),
        vec![
            (
                EdgeKind::Reads,
                DepTarget::Fact(FactId::new("plan.free.price"))
            ),
            (
                EdgeKind::Reads,
                DepTarget::Fact(FactId::new("plan.pro.price"))
            ),
            (EdgeKind::Includes, DepTarget::Snippet(SourceId(7))),
        ]
    );
    assert!(edges.iter().all(|e| e.from == EdgeOrigin::Page(page_id(1))));
}

#[test]
fn a_block_that_came_from_an_include_includes_it() {
    let included = Block {
        origin: Origin {
            span: None,
            frames: vec![Frame::Include {
                file: SourceId(3),
                at: Span {
                    source: SourceId(1),
                    start: 0,
                    end: 4,
                },
            }],
        },
        ..block("p", BlockKind::Paragraph, vec![text("body")])
    };

    let edges = extract(
        &document(root(vec![Node::Block(included)])),
        &ExpansionRecord::default(),
    );

    assert_eq!(
        targets(&edges),
        vec![(EdgeKind::Includes, DepTarget::Snippet(SourceId(3)))]
    );
    assert_eq!(
        edges[0].from,
        EdgeOrigin::Block(page_id(1), BlockId::explicit("p"))
    );
}

#[test]
fn component_edges_minted_without_a_page_are_rebased_onto_this_one() {
    let nil = PageId(ulid::Ulid::nil());
    let doc = Document {
        deps: Deps(vec![Edge {
            from: EdgeOrigin::Block(nil, BlockId::explicit("card")),
            to: DepTarget::Asset("img/hero.png".to_owned()),
            kind: EdgeKind::Embeds,
        }]),
        ..document(root(vec![text("body")]))
    };

    let edges = extract(&doc, &ExpansionRecord::default());

    assert_eq!(
        edges[0].from,
        EdgeOrigin::Block(page_id(1), BlockId::explicit("card"))
    );
}

#[test]
fn an_edge_that_already_names_another_page_is_left_alone() {
    let doc = Document {
        deps: Deps(vec![Edge {
            from: EdgeOrigin::Page(page_id(9)),
            to: DepTarget::Source("pricing-api".to_owned()),
            kind: EdgeKind::Reads,
        }]),
        ..document(root(vec![text("body")]))
    };

    assert_eq!(
        extract(&doc, &ExpansionRecord::default())[0].from,
        EdgeOrigin::Page(page_id(9))
    );
}

#[test]
fn the_same_edge_from_the_ast_and_from_the_component_appears_once() {
    let card = Edge {
        from: EdgeOrigin::Block(PageId(ulid::Ulid::nil()), BlockId::explicit("card")),
        to: DepTarget::Component("card".to_owned()),
        kind: EdgeKind::Documents,
    };
    let doc = Document {
        deps: Deps(vec![card]),
        ..document(root(vec![Node::Block(Block {
            id: BlockId::explicit("card"),
            explicit_id: Some("card".to_owned()),
            kind: BlockKind::Component {
                name: "card".to_owned(),
                props: Props::default(),
                slots: Slots::default(),
            },
            origin: Origin::default(),
            children: Vec::new(),
        })]))
    };

    assert_eq!(extract(&doc, &ExpansionRecord::default()).len(), 1);
}

#[test]
fn the_edge_list_does_not_depend_on_the_order_the_walk_found_them() {
    let one = document(root(vec![Node::Block(block(
        "p",
        BlockKind::Paragraph,
        vec![link("/a", Some("/a")), link("/b", Some("/b"))],
    ))]));
    let other = document(root(vec![Node::Block(block(
        "p",
        BlockKind::Paragraph,
        vec![link("/b", Some("/b")), link("/a", Some("/a"))],
    ))]));

    assert_eq!(
        extract(&one, &ExpansionRecord::default()),
        extract(&other, &ExpansionRecord::default())
    );
}
