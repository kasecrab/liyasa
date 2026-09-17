//! The §34.10 acceptance criteria for the Markdown routes, as assertions.
//!
//! `tests/golden/cm_140_markdown.rs` and `tests/golden/cm_141.rs` name files
//! that no crate compiles; the assertions live here instead, one test per
//! criterion, named for its requirement
//! (`plan/rfcs/1000-agents-crate-seam.md`).

use liyasa_components::{inst, nodes};
use liyasa_core::document::{Block, BlockKind, Document, Inline, Node, Origin, PropValue};
use liyasa_core::frontmatter::AiSetting;
use liyasa_core::ids::BlockId;
use liyasa_core::net::Url;

use super::*;

fn site() -> SiteMeta {
    SiteMeta {
        name: "Liyasa".to_owned(),
        canonical_origin: Url::parse("https://example.com/docs").expect("a valid origin"),
        llms_txt: Url::parse("https://example.com/docs/llms.txt").expect("a valid URL"),
        version: None,
        locale: liyasa_core::ids::Locale::new("en"),
    }
}

fn document(children: Vec<Node>) -> Document {
    Document {
        root: Block {
            id: BlockId::implicit("document", "", "", 0),
            explicit_id: None,
            kind: BlockKind::Document,
            origin: Origin::default(),
            children,
        },
        deps: Default::default(),
        diagnostics: Diagnostics::new(),
    }
}

fn frontmatter(title: &str, description: Option<&str>) -> FrontmatterFields {
    FrontmatterFields {
        title: Some(title.to_owned()),
        description: description.map(str::to_owned),
        ..FrontmatterFields::default()
    }
}

/// The fixture owns what `Options` borrows.
struct Fixture {
    site: SiteMeta,
    registry: Registry,
    route: Route,
    routes: BTreeSet<Route>,
    frontmatter: Option<FrontmatterFields>,
    site_instructions: Option<String>,
    openapi_schema: Option<String>,
}

impl Fixture {
    fn new() -> Self {
        Self {
            site: site(),
            registry: Registry::builtins(),
            route: Route::new("/guide/install"),
            routes: ["/guide/install", "/guide/faq"]
                .into_iter()
                .map(Route::new)
                .collect(),
            frontmatter: Some(frontmatter("Install", Some("Install Liyasa in a minute."))),
            site_instructions: None,
            openapi_schema: None,
        }
    }

    fn render(&self, children: Vec<Node>) -> Page {
        render_page(
            &document(children),
            &Options {
                site: &self.site,
                registry: &self.registry,
                route: &self.route,
                frontmatter: self.frontmatter.as_ref(),
                routes: &self.routes,
                site_instructions: self.site_instructions.as_deref(),
                openapi_schema: self.openapi_schema.as_deref(),
            },
        )
    }
}

#[test]
fn cm_141_the_discovery_directive_is_the_first_line() {
    let page = Fixture::new().render(vec![nodes::paragraph("Run the installer.")]);
    let first = page.markdown.lines().next().expect("a first line");
    assert_eq!(
        first,
        "> For AI agents: a documentation index is available at https://example.com/docs/llms.txt"
    );
}

#[test]
fn cm_140_front_matter_is_reduced_to_a_heading_and_a_summary() {
    let page = Fixture::new().render(vec![nodes::paragraph("Run the installer.")]);
    assert_eq!(
        page.markdown,
        "> For AI agents: a documentation index is available at https://example.com/docs/llms.txt\n\
         \n\
         # Install\n\
         \n\
         Install Liyasa in a minute.\n\
         \n\
         Run the installer.\n"
    );
}

#[test]
fn cm_140_the_heading_follows_the_directive_and_precedes_the_content() {
    let page = Fixture::new().render(vec![nodes::paragraph("Run the installer.")]);
    let directive = page.markdown.find("> For AI agents").expect("a directive");
    let heading = page.markdown.find("# Install").expect("a heading");
    let content = page.markdown.find("Run the installer").expect("content");
    assert!(
        directive < heading && heading < content,
        "{}",
        page.markdown
    );
}

#[test]
fn cm_140_a_page_without_a_description_has_no_summary_line() {
    let mut fixture = Fixture::new();
    fixture.frontmatter = Some(frontmatter("Changelog", None));
    let page = fixture.render(vec![nodes::heading(2, "2026-09-15")]);
    assert!(
        page.markdown.contains("# Changelog\n\n## 2026-09-15"),
        "{}",
        page.markdown
    );
}

#[test]
fn cm_140_a_leading_heading_that_repeats_the_title_is_not_written_twice() {
    let page = Fixture::new().render(vec![
        nodes::heading(1, "Install"),
        nodes::paragraph("Run the installer."),
    ]);
    assert_eq!(
        page.markdown.matches("# Install").count(),
        1,
        "{}",
        page.markdown
    );
}

#[test]
fn cm_140_a_page_without_front_matter_keeps_its_own_heading() {
    let mut fixture = Fixture::new();
    fixture.frontmatter = None;
    let page = fixture.render(vec![
        nodes::heading(1, "Install"),
        nodes::paragraph("Run the installer."),
    ]);
    assert_eq!(
        page.markdown.matches("# Install").count(),
        1,
        "{}",
        page.markdown
    );
    assert!(
        page.markdown.contains("Run the installer."),
        "{}",
        page.markdown
    );
}

#[test]
fn cm_140_components_serialize_with_no_directive_syntax_left() {
    let page = Fixture::new().render(vec![nodes::component(
        inst::new("note")
            .child(nodes::paragraph("Requires Rust 1.98."))
            .build(),
    )]);
    assert!(page.markdown.contains("> **Note**"), "{}", page.markdown);
    assert!(
        page.markdown.contains("> Requires Rust 1.98."),
        "{}",
        page.markdown
    );
    assert!(!page.markdown.contains(":::"), "{}", page.markdown);
    assert!(!page.markdown.contains("{{"), "{}", page.markdown);
    assert!(!page.markdown.contains("{%"), "{}", page.markdown);
}

#[test]
fn cm_142_agent_notes_are_the_last_section() {
    let mut fixture = Fixture::new();
    fixture.site_instructions = Some("Quote the version number.".to_owned());
    fixture.frontmatter = Some(FrontmatterFields {
        ai: Some(AiSetting::Options {
            instructions: Some("This page lists supported versions only.".to_owned()),
        }),
        ..frontmatter("Releases", None)
    });
    let page = fixture.render(vec![nodes::paragraph("v2 is supported.")]);
    assert!(
        page.markdown.ends_with(
            "## Notes for agents\n\n\
             Quote the version number.\n\n\
             This page lists supported versions only.\n"
        ),
        "{}",
        page.markdown
    );
}

#[test]
fn cm_142_a_page_with_no_instructions_has_no_notes_section() {
    let page = Fixture::new().render(vec![nodes::paragraph("v2 is supported.")]);
    assert!(
        !page.markdown.contains(AGENT_NOTES_HEADING),
        "{}",
        page.markdown
    );
}

#[test]
fn cm_143_a_human_only_block_is_stripped_and_an_agent_only_block_stays() {
    let page = Fixture::new().render(vec![
        nodes::component(
            inst::new("visibility")
                .prop("humans", PropValue::Bool(true))
                .child(nodes::paragraph("Watch the walkthrough video."))
                .build(),
        ),
        nodes::component(
            inst::new("visibility")
                .prop("agents", PropValue::Bool(true))
                .child(nodes::paragraph("The installer exits 0."))
                .build(),
        ),
    ]);
    assert!(!page.markdown.contains("walkthrough"), "{}", page.markdown);
    assert!(
        page.markdown.contains("The installer exits 0."),
        "{}",
        page.markdown
    );
}

#[test]
fn cm_143_a_block_naming_neither_audience_stays() {
    let page = Fixture::new().render(vec![nodes::component(
        inst::new("visibility")
            .child(nodes::paragraph("Internal rates."))
            .build(),
    )]);
    assert!(
        page.markdown.contains("Internal rates."),
        "{}",
        page.markdown
    );
}

#[test]
fn rx_63_the_operation_schema_is_appended_when_it_is_asked_for() {
    let mut fixture = Fixture::new();
    fixture.openapi_schema = Some("{\n  \"operationId\": \"listPets\"\n}".to_owned());
    let page = fixture.render(vec![nodes::paragraph("Lists pets.")]);
    assert!(
        page.markdown.contains("## OpenAPI schema"),
        "{}",
        page.markdown
    );
    assert!(
        page.markdown.contains("\"operationId\": \"listPets\""),
        "{}",
        page.markdown
    );
}

#[test]
fn rx_63_no_schema_is_appended_when_the_key_is_off() {
    let page = Fixture::new().render(vec![nodes::paragraph("Lists pets.")]);
    assert!(
        !page.markdown.contains("OpenAPI schema"),
        "{}",
        page.markdown
    );
}

#[test]
fn rx_65_a_relative_link_is_rewritten_and_warned_about() {
    let page = Fixture::new().render(vec![nodes::paragraph_of(vec![Inline::Link {
        href: "../guide/faq".to_owned(),
        title: None,
        children: vec![Inline::Text("Questions".to_owned())],
        resolved: None,
    }])]);
    assert!(
        page.markdown
            .contains("[Questions](https://example.com/docs/guide/faq.md)"),
        "{}",
        page.markdown
    );
    let codes: Vec<&str> = page.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["W0406"]);
}

#[test]
fn rx_65_an_absolute_link_raises_nothing() {
    let page = Fixture::new().render(vec![nodes::paragraph_of(vec![Inline::Link {
        href: "https://example.org/spec".to_owned(),
        title: None,
        children: vec![Inline::Text("Spec".to_owned())],
        resolved: None,
    }])]);
    assert!(page.diagnostics.is_empty(), "{:?}", page.diagnostics);
}

#[test]
fn rx_65_an_image_source_is_absolute_too() {
    let page = Fixture::new().render(vec![nodes::paragraph_of(vec![Inline::Image {
        src: "/assets/diagram.png".to_owned(),
        alt: "Diagram".to_owned(),
        title: None,
        dark: None,
    }])]);
    assert!(
        page.markdown
            .contains("![Diagram](https://example.com/docs/assets/diagram.png)"),
        "{}",
        page.markdown
    );
}
