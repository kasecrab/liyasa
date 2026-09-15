use std::collections::BTreeMap;

use liyasa_core::components::{
    Component, ComponentInst, EditorBlock, FormField, MdCtx, PropDef, PropSchema, RenderCtx,
    RenderError, SlotDef,
};
use liyasa_core::document::{Dep, Origin, Props, Slots};
use liyasa_core::ids::{BlockId, FactId};
use liyasa_core::markdown::ComponentKind;
use liyasa_core::span::SourceId;

use super::*;

// ---- fixture builders ----

fn block(kind: BlockKind, children: Vec<Node>) -> Block {
    Block {
        id: BlockId::implicit("b", &format!("{kind:?}"), "", 0),
        explicit_id: None,
        kind,
        origin: Origin::at(Span::new(SourceId(0), 0, 1)),
        children,
    }
}

fn doc(children: Vec<Node>) -> Block {
    block(BlockKind::Document, children)
}

fn para(inlines: Vec<Inline>) -> Node {
    Node::Block(block(
        BlockKind::Paragraph,
        inlines.into_iter().map(Node::Inline).collect(),
    ))
}

fn heading(anchor: &str) -> Node {
    Node::Block(block(
        BlockKind::Heading {
            level: 2,
            anchor: anchor.to_owned(),
        },
        Vec::new(),
    ))
}

fn link(href: &str) -> Inline {
    Inline::Link {
        href: href.to_owned(),
        title: None,
        children: vec![Inline::Text("text".to_owned())],
        resolved: None,
    }
}

fn image(src: &str, alt: &str) -> Inline {
    Inline::Image {
        src: src.to_owned(),
        alt: alt.to_owned(),
        title: None,
        dark: None,
    }
}

fn frontmatter(description: Option<&str>) -> FrontmatterFields {
    FrontmatterFields {
        description: description.map(str::to_owned),
        ..FrontmatterFields::default()
    }
}

fn page<'a>(
    route: &str,
    source: &'a str,
    root: &'a Block,
    frontmatter: Option<&'a FrontmatterFields>,
) -> PageView<'a> {
    PageView {
        route: Route::new(route),
        source,
        frontmatter,
        root,
        expansion: None,
    }
}

fn codes(diagnostics: &Diagnostics) -> Vec<&'static str> {
    diagnostics.iter().map(|d| d.code.as_str()).collect()
}

// ---- a minimal component registry ----

struct Callout;

impl Component for Callout {
    fn name(&self) -> &'static str {
        "callout"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["Callout"]
    }
    fn schema(&self) -> &PropSchema {
        static SCHEMA: std::sync::LazyLock<PropSchema> = std::sync::LazyLock::new(|| PropSchema {
            props: vec![
                PropDef {
                    name: "type",
                    ty: PropType::Enum(vec!["note".to_owned(), "warning".to_owned()]),
                    required: true,
                    default: None,
                    doc: "",
                },
                PropDef {
                    name: "collapsible",
                    ty: PropType::Bool,
                    required: false,
                    default: None,
                    doc: "",
                },
            ],
            slots: vec![SlotDef {
                name: "default",
                required: false,
                doc: "",
            }],
        });
        &SCHEMA
    }
    fn kind(&self) -> ComponentKind {
        ComponentKind::Container
    }
    fn render_html(&self, _: &ComponentInst, _: &mut RenderCtx) -> Result<(), RenderError> {
        Ok(())
    }
    fn render_markdown(&self, _: &ComponentInst, _: &mut MdCtx) -> Result<(), RenderError> {
        Ok(())
    }
    fn render_text(&self, _: &ComponentInst) -> String {
        String::new()
    }
    fn editor_block(&self) -> EditorBlock {
        EditorBlock {
            icon: String::new(),
            category: String::new(),
            form: Vec::<FormField>::new(),
            inline: false,
        }
    }
    fn deps(&self, _: &ComponentInst) -> Vec<Dep> {
        Vec::new()
    }
}

struct OneComponent(Callout);

impl ComponentRegistry for OneComponent {
    fn get(&self, name: &str) -> Option<&dyn Component> {
        (name == "callout").then_some(&self.0 as &dyn Component)
    }
    fn names(&self) -> Vec<&str> {
        vec!["callout"]
    }
}

fn component(name: &str, props: &[(&str, PropValue)]) -> Node {
    Node::Block(block(
        BlockKind::Component {
            name: name.to_owned(),
            props: Props(
                props
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), v.clone()))
                    .collect::<BTreeMap<_, _>>(),
            ),
            slots: Slots::default(),
        },
        Vec::new(),
    ))
}

// ---- VER-60's acceptance criterion ----

#[test]
fn ver_60_every_structural_problem_raises_its_code() {
    let broken = doc(vec![
        heading("known"),
        para(vec![
            link("/nowhere"),        // E0401
            link("/target#missing"), // E0402
            Inline::Image {
                // E0403
                src: "/assets/absent.png".to_owned(),
                alt: "a".to_owned(),
                title: None,
                dark: None,
            },
            image("/assets/present.png", ""), // E0305
        ]),
        component("callout", &[]), // E0314 (no `type`)
        component("callout", &[("type", PropValue::Str("loud".to_owned()))]), // E0315
        component(
            "callout",
            &[
                ("type", PropValue::Str("note".to_owned())),
                ("colour", PropValue::Str("red".to_owned())),
            ],
        ), // W0316
        component("banner", &[]),  // E0313
    ]);
    let target = doc(vec![heading("present")]);
    let orphan_root = doc(Vec::new());

    let unclosed = "# Title\n\n```rust\nfn main() {}\n";
    let long: String = "x".repeat(120_000);
    let fm = frontmatter(Some("has one"));

    let mut site = SiteView::new(vec![
        page("/broken", "# Broken\n", &broken, Some(&fm)),
        page("/target", "# Target\n", &target, Some(&fm)),
        page("/fences", unclosed, &orphan_root, Some(&fm)),
        page("/huge", &long, &orphan_root, Some(&fm)),
        page("/nodesc", "# No description\n", &orphan_root, None),
        page("/orphan", "# Orphan\n", &orphan_root, Some(&fm)),
        // The same route twice: E0105.
        page("/target", "# Again\n", &target, Some(&fm)),
    ]);
    site.assets = ["/assets/present.png".to_owned()].into_iter().collect();
    site.reachable = ["/broken", "/target", "/fences", "/huge", "/nodesc"]
        .into_iter()
        .map(Route::new)
        .collect();

    let registry = OneComponent(Callout);
    let found = check_site(&site, Some(&registry));
    let found = codes(&found);

    // E0201 needs an expansion record and has a fixture of its own below.
    for code in [
        "E0105", "E0301", "E0305", "E0307", "E0313", "E0314", "E0315", "E0401", "E0402", "E0403",
        "W0130", "W0316", "W0630",
    ] {
        assert!(found.contains(&code), "{code} is missing from {found:?}");
    }

    // VER-60: the build fails, which is exit code 1 (CLI-31).
    let diagnostics = check_site(&site, Some(&registry));
    assert!(diagnostics.has_errors());
}

#[test]
fn a_site_with_no_problems_produces_no_diagnostics() {
    let root = doc(vec![heading("intro"), para(vec![link("/other#intro")])]);
    let other = doc(vec![heading("intro")]);
    let fm = frontmatter(Some("a description"));
    let mut site = SiteView::new(vec![
        page("/here", "# Here\n", &root, Some(&fm)),
        page("/other", "# Other\n", &other, Some(&fm)),
    ]);
    site.reachable = ["/here", "/other"].into_iter().map(Route::new).collect();
    assert_eq!(codes(&check_site(&site, None)), Vec::<&str>::new());
}

// ---- one check at a time ----

#[test]
fn an_anchor_on_the_same_page_resolves_against_that_page() {
    let root = doc(vec![
        heading("intro"),
        para(vec![link("#intro"), link("#gone")]),
    ]);
    let fm = frontmatter(Some("d"));
    let site = SiteView::new(vec![page("/p", "", &root, Some(&fm))]);
    assert_eq!(codes(&check_site(&site, None)), ["E0402"]);
}

#[test]
fn an_explicit_block_id_is_an_anchor() {
    let mut target = block(BlockKind::Paragraph, Vec::new());
    target.explicit_id = Some("pricing".to_owned());
    let root = doc(vec![Node::Block(target), para(vec![link("#pricing")])]);
    let fm = frontmatter(Some("d"));
    let site = SiteView::new(vec![page("/p", "", &root, Some(&fm))]);
    assert_eq!(codes(&check_site(&site, None)), Vec::<&str>::new());
}

#[test]
fn a_relative_link_resolves_against_the_pages_directory() {
    let root = doc(vec![para(vec![
        link("install.md"),
        link("../api/tokens"),
        link("./install"),
    ])]);
    let install = doc(Vec::new());
    let fm = frontmatter(Some("d"));
    let site = SiteView::new(vec![
        page("/guide/start", "", &root, Some(&fm)),
        page("/guide/install", "", &install, Some(&fm)),
        page("/api/tokens", "", &install, Some(&fm)),
    ]);
    assert_eq!(codes(&check_site(&site, None)), Vec::<&str>::new());
}

#[test]
fn an_index_file_resolves_to_its_directory() {
    let root = doc(vec![para(vec![link("/guide/index.md")])]);
    let guide = doc(Vec::new());
    let fm = frontmatter(Some("d"));
    let site = SiteView::new(vec![
        page("/p", "", &root, Some(&fm)),
        page("/guide", "", &guide, Some(&fm)),
    ]);
    assert_eq!(codes(&check_site(&site, None)), Vec::<&str>::new());
}

#[test]
fn an_external_link_is_not_an_internal_one() {
    let root = doc(vec![para(vec![
        link("https://example.com"),
        link("mailto:a@b.test"),
        link("tel:+15550100"),
    ])]);
    let fm = frontmatter(Some("d"));
    let site = SiteView::new(vec![page("/p", "", &root, Some(&fm))]);
    assert_eq!(codes(&check_site(&site, None)), Vec::<&str>::new());
}

#[test]
fn a_data_uri_image_needs_no_asset() {
    let root = doc(vec![para(vec![image(
        "data:image/png;base64,iVBORw0KGgo=",
        "a dot",
    )])]);
    let fm = frontmatter(Some("d"));
    let site = SiteView::new(vec![page("/p", "", &root, Some(&fm))]);
    assert_eq!(codes(&check_site(&site, None)), Vec::<&str>::new());
}

#[test]
fn a_dark_image_variant_is_checked_too() {
    let root = doc(vec![para(vec![Inline::Image {
        src: "/a/light.png".to_owned(),
        alt: "a".to_owned(),
        title: None,
        dark: Some("/a/dark.png".to_owned()),
    }])]);
    let fm = frontmatter(Some("d"));
    let mut site = SiteView::new(vec![page("/p", "", &root, Some(&fm))]);
    site.assets = ["/a/light.png".to_owned()].into_iter().collect();
    assert_eq!(codes(&check_site(&site, None)), ["E0403"]);
}

#[test]
fn a_closed_fence_is_not_reported() {
    let fm = frontmatter(Some("d"));
    let root = doc(Vec::new());
    let site = SiteView::new(vec![page(
        "/p",
        "```rust\nfn main() {}\n```\n\n~~~\ntext\n~~~\n",
        &root,
        Some(&fm),
    )]);
    assert_eq!(codes(&check_site(&site, None)), Vec::<&str>::new());
}

#[test]
fn a_longer_fence_may_contain_a_shorter_one() {
    let fm = frontmatter(Some("d"));
    let root = doc(Vec::new());
    let site = SiteView::new(vec![page(
        "/p",
        "````markdown\n```rust\nfn main() {}\n```\n````\n",
        &root,
        Some(&fm),
    )]);
    assert_eq!(codes(&check_site(&site, None)), Vec::<&str>::new());
}

#[test]
fn a_tilde_fence_does_not_close_a_backtick_one() {
    let fm = frontmatter(Some("d"));
    let root = doc(Vec::new());
    let site = SiteView::new(vec![page("/p", "```\ntext\n~~~\n", &root, Some(&fm))]);
    assert_eq!(codes(&check_site(&site, None)), ["E0301"]);
}

#[test]
fn the_size_bands_are_a_warning_then_an_error() {
    let fm = frontmatter(Some("d"));
    let root = doc(Vec::new());
    let limits = SizeLimits {
        warn_chars: 10,
        error_chars: 20,
    };

    for (chars, want) in [(5, vec![]), (15, vec!["W0308"]), (25, vec!["E0307"])] {
        let source = "x".repeat(chars);
        let mut site = SiteView::new(vec![page("/p", &source, &root, Some(&fm))]);
        site.limits = limits;
        assert_eq!(codes(&check_site(&site, None)), want, "{chars} characters");
    }
}

#[test]
fn a_size_is_counted_in_characters_not_bytes() {
    let fm = frontmatter(Some("d"));
    let root = doc(Vec::new());
    // 15 characters, 30 bytes: under a 20-character error band.
    let source = "é".repeat(15);
    let mut site = SiteView::new(vec![page("/p", &source, &root, Some(&fm))]);
    site.limits = SizeLimits {
        warn_chars: 20,
        error_chars: 25,
    };
    assert_eq!(codes(&check_site(&site, None)), Vec::<&str>::new());
}

#[test]
fn an_empty_description_counts_as_none() {
    let root = doc(Vec::new());
    let fm = frontmatter(Some("   "));
    let site = SiteView::new(vec![page("/p", "", &root, Some(&fm))]);
    assert_eq!(codes(&check_site(&site, None)), ["W0630"]);
}

#[test]
fn no_navigation_means_no_orphans() {
    let root = doc(Vec::new());
    let fm = frontmatter(Some("d"));
    // `reachable` is empty: the caller has no navigation to compare against.
    let site = SiteView::new(vec![page("/p", "", &root, Some(&fm))]);
    assert_eq!(codes(&check_site(&site, None)), Vec::<&str>::new());
}

#[test]
fn a_hidden_page_is_not_an_orphan() {
    let root = doc(Vec::new());
    let fm = FrontmatterFields {
        description: Some("d".to_owned()),
        hidden: Some(true),
        ..FrontmatterFields::default()
    };
    let mut site = SiteView::new(vec![page("/secret", "", &root, Some(&fm))]);
    site.reachable = [Route::new("/other")].into_iter().collect();
    assert_eq!(codes(&check_site(&site, None)), Vec::<&str>::new());
}

#[test]
fn a_fact_a_page_reads_and_the_site_does_not_define_is_e0201() {
    let root = doc(Vec::new());
    let fm = frontmatter(Some("d"));
    let expansion = ExpansionRecord {
        facts: [FactId::new("pricing.pro"), FactId::new("pricing.free")]
            .into_iter()
            .collect(),
        ..ExpansionRecord::default()
    };
    let mut site = SiteView::new(vec![PageView {
        expansion: Some(&expansion),
        ..page("/p", "", &root, Some(&fm))
    }]);
    site.defined = ["facts.pricing.pro".to_owned()].into_iter().collect();
    assert_eq!(codes(&check_site(&site, None)), ["E0201"]);
}

#[test]
fn an_allow_listed_environment_variable_is_checked_the_same_way() {
    let root = doc(Vec::new());
    let fm = frontmatter(Some("d"));
    let expansion = ExpansionRecord {
        env: ["BUILD_ID".to_owned()].into_iter().collect(),
        ..ExpansionRecord::default()
    };
    let site = SiteView::new(vec![PageView {
        expansion: Some(&expansion),
        ..page("/p", "", &root, Some(&fm))
    }]);
    assert_eq!(codes(&check_site(&site, None)), ["E0201"]);
}

#[test]
fn a_component_prop_written_as_an_expression_is_not_a_type_error() {
    let root = doc(vec![component(
        "callout",
        &[("type", PropValue::Expr("{{ kind }}".to_owned()))],
    )]);
    let fm = frontmatter(Some("d"));
    let site = SiteView::new(vec![page("/p", "", &root, Some(&fm))]);
    let registry = OneComponent(Callout);
    assert_eq!(
        codes(&check_site(&site, Some(&registry))),
        Vec::<&str>::new()
    );
}

#[test]
fn components_are_not_checked_when_no_registry_is_given() {
    let root = doc(vec![component("nonsense", &[])]);
    let fm = frontmatter(Some("d"));
    let site = SiteView::new(vec![page("/p", "", &root, Some(&fm))]);
    assert_eq!(codes(&check_site(&site, None)), Vec::<&str>::new());
}

#[test]
fn a_boolean_prop_given_a_string_is_e0315() {
    let root = doc(vec![component(
        "callout",
        &[
            ("type", PropValue::Str("note".to_owned())),
            ("collapsible", PropValue::Str("yes".to_owned())),
        ],
    )]);
    let fm = frontmatter(Some("d"));
    let site = SiteView::new(vec![page("/p", "", &root, Some(&fm))]);
    let registry = OneComponent(Callout);
    assert_eq!(codes(&check_site(&site, Some(&registry))), ["E0315"]);
}

#[test]
fn an_image_inside_a_link_is_still_checked() {
    let root = doc(vec![para(vec![Inline::Link {
        href: "/target".to_owned(),
        title: None,
        children: vec![Inline::Image {
            src: "/a/missing.png".to_owned(),
            alt: String::new(),
            title: None,
            dark: None,
        }],
        resolved: None,
    }])]);
    let target = doc(Vec::new());
    let fm = frontmatter(Some("d"));
    let site = SiteView::new(vec![
        page("/p", "", &root, Some(&fm)),
        page("/target", "", &target, Some(&fm)),
    ]);
    let found = codes(&check_site(&site, None));
    assert!(found.contains(&"E0305"), "{found:?}");
    assert!(found.contains(&"E0403"), "{found:?}");
}

#[test]
fn a_duplicate_route_is_reported_once_not_per_copy() {
    let root = doc(Vec::new());
    let fm = frontmatter(Some("d"));
    let site = SiteView::new(vec![
        page("/p", "", &root, Some(&fm)),
        page("/p", "", &root, Some(&fm)),
        page("/p", "", &root, Some(&fm)),
    ]);
    assert_eq!(codes(&check_site(&site, None)), ["E0105"]);
}
