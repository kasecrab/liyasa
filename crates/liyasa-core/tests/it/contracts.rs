//! The frozen contract types' own behaviour (PRD §7.3.1, §7.5.1, §7.16).

use liyasa_core::document::{
    Align, Block, BlockKind, DepTarget, Edge, EdgeKind, EdgeOrigin, Frame, Inline, Node, Origin,
    PropValue, Props, Segment, SourceDocument, TemplateKind,
};
use liyasa_core::markdown::{RewriteMap, SpanMap};
use liyasa_core::{BlockId, Diagnostics, PageId, Route, SourceId, Span};

fn span(start: u32, end: u32) -> Span {
    Span::new(SourceId(0), start, end)
}

#[test]
fn span_map_finds_the_run_covering_a_byte() {
    let map = SpanMap(vec![
        (0, 5, Origin::at(span(0, 5))),
        (5, 9, Origin::generated_by(span(20, 32))),
    ]);
    assert!(map.is_well_formed());
    assert_eq!(map.origin_at(0).and_then(|o| o.span), Some(span(0, 5)));
    assert_eq!(map.origin_at(4).and_then(|o| o.span), Some(span(0, 5)));
    assert!(map.origin_at(5).expect("run exists").is_generated());
    assert!(map.origin_at(9).is_none(), "runs are half-open");
    assert!(map.origin_at(99).is_none());
}

#[test]
fn span_map_reports_every_run_overlapping_a_range() {
    let map = SpanMap(vec![
        (0, 4, Origin::at(span(0, 4))),
        (4, 8, Origin::at(span(10, 14))),
        (8, 12, Origin::at(span(20, 24))),
    ]);
    let covered: Vec<_> = map
        .origins_in(3, 9)
        .map(|(start, end, _)| (*start, *end))
        .collect();
    assert_eq!(covered, [(0, 4), (4, 8), (8, 12)]);
    assert_eq!(map.origins_in(4, 4).count(), 0);
}

#[test]
fn an_overlapping_span_map_is_rejected() {
    let map = SpanMap(vec![(0, 6, Origin::default()), (4, 9, Origin::default())]);
    assert!(!map.is_well_formed());
}

#[test]
fn rewrite_map_converts_rewritten_offsets_to_expanded_ones() {
    // Line 1 unchanged; line 2's directive became a marker 8 bytes longer.
    let map = RewriteMap(vec![(0, 0), (10, 8)]);
    assert_eq!(map.to_expanded(0), 0);
    assert_eq!(map.to_expanded(9), 9);
    assert_eq!(map.to_expanded(10), 2);
    assert_eq!(map.to_expanded(15), 7);
}

#[test]
fn rewrite_map_handles_markers_shorter_than_the_directive() {
    let map = RewriteMap(vec![(0, 0), (20, -6)]);
    assert_eq!(map.to_expanded(20), 26);
    assert_eq!(map.to_expanded(24), 30);
}

/// The discriminating input for the underflow WP-03 escalated: a map whose
/// first entry starts above zero, queried below it. No other test reaches the
/// `else` arm with a non-empty map, because every map in the suite starts at
/// offset 0 — WP-03 worked around the bug by always emitting an entry there.
///
/// Below the first rewritten line nothing has changed length yet, so expanded
/// and rewritten are the same offset. Getting this wrong is quiet: in release
/// `to_expanded(9)` used to double-wrap to 1, a small plausible offset nothing
/// downstream can tell from a real one.
#[test]
fn a_map_that_does_not_start_at_zero_is_the_identity_below_its_first_entry() {
    let map = RewriteMap(vec![(10, 8)]);
    assert_eq!(map.to_expanded(9), 9);
    assert_eq!(map.to_expanded(0), 0);
    // The entry itself still applies from its own line start onward.
    assert_eq!(map.to_expanded(10), 2);
}

#[test]
fn an_empty_rewrite_map_is_the_identity() {
    assert_eq!(RewriteMap::default().to_expanded(42), 42);
}

#[test]
fn origin_frames_are_innermost_last() {
    let origin = Origin {
        span: None,
        frames: vec![
            Frame::Include {
                file: SourceId(1),
                at: span(0, 10),
            },
            Frame::Snippet {
                name: "plans".into(),
                at: span(4, 8),
            },
            Frame::Loop {
                at: span(5, 7),
                index: 3,
            },
        ],
    };
    assert!(origin.is_generated());
    assert!(matches!(
        origin.frames.last(),
        Some(Frame::Loop { index: 3, .. })
    ));
}

#[test]
fn the_source_document_json_form_round_trips() {
    let doc = SourceDocument {
        source: SourceId(0),
        frontmatter: None,
        segments: vec![
            Segment::Markdown { span: span(0, 12) },
            Segment::Template {
                span: span(12, 26),
                kind: TemplateKind::Statement {
                    name: "for".into(),
                    matching: Some(3),
                },
            },
            Segment::DirectiveOpen {
                span: span(26, 60),
                name: "card".into(),
                props: Props(
                    [
                        ("title".to_owned(), PropValue::Str("Install".into())),
                        ("columns".to_owned(), PropValue::Num(2.0)),
                        ("href".to_owned(), PropValue::Expr("page.url".into())),
                    ]
                    .into_iter()
                    .collect(),
                ),
                colons: 3,
                matching: Some(3),
            },
            Segment::DirectiveClose { span: span(60, 64) },
        ],
    };
    let json = serde_json::to_string(&doc).expect("serializes");
    let back: SourceDocument = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(back, doc);
    assert_eq!(back.segments[3].span(), span(60, 64));
}

#[test]
fn the_rendered_ast_json_form_round_trips() {
    let heading = Block {
        id: BlockId::explicit("install"),
        explicit_id: Some("install".into()),
        kind: BlockKind::Heading {
            level: 2,
            anchor: "install".into(),
        },
        origin: Origin::at(span(0, 12)),
        children: vec![Node::Inline(Inline::Text("Install".into()))],
    };
    let table = Block {
        id: BlockId::implicit("table", "", "install", 0),
        explicit_id: None,
        kind: BlockKind::Table {
            align: vec![Align::Left, Align::Right],
        },
        origin: Origin::default(),
        children: Vec::new(),
    };
    let root = Block {
        id: BlockId::implicit("document", "", "", 0),
        explicit_id: None,
        kind: BlockKind::Document,
        origin: Origin::default(),
        children: vec![Node::Block(heading), Node::Block(table)],
    };
    let json = serde_json::to_string(&root).expect("serializes");
    let back: Block = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(back, root);
}

#[test]
fn every_inline_kind_survives_the_json_form() {
    let inlines = vec![
        Inline::Text("plain".into()),
        Inline::Emph(vec![Inline::Text("emphasis".into())]),
        Inline::Strong(vec![Inline::Text("strong".into())]),
        Inline::Strike(vec![Inline::Text("struck".into())]),
        Inline::Code("code".into()),
        Inline::Link {
            href: "/install".into(),
            title: None,
            children: vec![Inline::Text("Install".into())],
            resolved: Some(Route::new("/install")),
        },
        Inline::Image {
            src: "/flow.png".into(),
            alt: "Request flow".into(),
            title: None,
            dark: Some("/flow.dark.png".into()),
        },
        Inline::HtmlInline("<br>".into()),
        Inline::FootnoteRef("1".into()),
        Inline::SoftBreak,
        Inline::HardBreak,
        Inline::InlineComponent {
            name: "kbd".into(),
            props: Props::default(),
            children: vec![Inline::Text("Ctrl+K".into())],
        },
        Inline::Math("a^2".into()),
        Inline::TemplateInline {
            expr: "page.title".into(),
            origin: span(1, 2),
        },
    ];
    let json = serde_json::to_string(&inlines).expect("serializes");
    let back: Vec<Inline> = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(back, inlines);
}

#[test]
fn edges_reference_pages_by_ulid_so_a_rename_does_not_break_them() {
    let page = PageId::parse("01J8ZZZZZZZZZZZZZZZZZZZZZZ").expect("valid ulid");
    let edge = Edge {
        from: EdgeOrigin::Block(page, BlockId::explicit("pricing")),
        to: DepTarget::Fact(liyasa_core::ids::FactId::new("plan.pro.price")),
        kind: EdgeKind::Reads,
    };
    let json = serde_json::to_string(&edge).expect("serializes");
    let back: Edge = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(back, edge);
}

#[test]
fn a_document_carries_its_own_diagnostics() {
    let doc = liyasa_core::Document {
        root: Block {
            id: BlockId::implicit("document", "", "", 0),
            explicit_id: None,
            kind: BlockKind::Document,
            origin: Origin::default(),
            children: Vec::new(),
        },
        deps: Default::default(),
        diagnostics: Diagnostics::new(),
    };
    assert!(!doc.diagnostics.has_errors());
}

#[test]
fn yaml_parsing_returns_a_diagnostic_not_a_panic() {
    let error = liyasa_core::yaml::parse_value("a: [unclosed\n", None).expect_err("must fail");
    assert_eq!(error.code.as_str(), "E0101");
    let value =
        liyasa_core::yaml::parse_value("title: Install\ndraft: true\n", None).expect("valid yaml");
    assert_eq!(value["title"], "Install");
}

#[test]
fn front_matter_deserializes_the_typed_view() {
    let (_, typed) = liyasa_core::yaml::parse_frontmatter::<liyasa_core::FrontmatterFields>(
        "title: Install\nsidebarTitle: Setup\nmode: wide\npersonalized: true\nkeywords: [cli, setup]\n",
        None,
    )
    .expect("valid front matter");
    assert_eq!(typed.title.as_deref(), Some("Install"));
    assert_eq!(typed.sidebar_title.as_deref(), Some("Setup"));
    assert_eq!(typed.mode, Some(liyasa_core::frontmatter::PageMode::Wide));
    assert_eq!(typed.personalized, Some(true));
    assert_eq!(typed.keywords, ["cli", "setup"]);
}
