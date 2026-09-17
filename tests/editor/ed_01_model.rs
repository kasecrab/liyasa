//! ED-01's Rust half.
//!
//! > Given a page with prose, a directive, a template chip, and a loop; when
//! > opened in visual mode; then each becomes the specified ProseMirror node,
//! > **the preview of any block equals the build output byte for byte**, and
//! > editing prose re-serializes only that segment.
//!
//! The middle clause is the one a Rust test can settle, and it is the one that
//! matters most: an editor whose preview is *nearly* the build's output is an
//! editor that lies about what readers will see, and the difference is only
//! ever noticed on the page where it matters.
//!
//! So this renders the same source twice — once through
//! `liyasa_wasm::Session::preview`, which is what the editor calls, and once
//! through `liyasa_build::render::page`, which is what a build calls — and
//! compares the three serializations byte for byte.
//!
//! The mapping clause is asserted on the TypeScript side, against
//! `web/editor/test/fixtures/segments.json`, which `tests/editor/segments.rs`
//! writes from the real scanner. What is asserted here instead is that the
//! segment kinds the editor's six node kinds are built from are the only kinds
//! the scanner emits — so a new one cannot appear without this failing.

use std::sync::Arc;

use liyasa_build::render;
use liyasa_components::Registry;
use liyasa_core::document::Segment;
use liyasa_core::ids::Locale;
use liyasa_core::markdown::SiteMeta;
use liyasa_core::net::Url;
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::VfsPath;
use liyasa_markdown::source::Layers;
use liyasa_wasm::api::{OpenRequest, PreviewRequest, SiteMeta as WasmSiteMeta};
use liyasa_wasm::session::Session;

/// A page with each construct ED-01 names: prose, a directive, a chip, a loop,
/// a fence, and a raw HTML block the visual editor does not model.
const PAGE: &str = "\
Every project has caps.

:::note{title=\"Heads up\"}
Raising a cap does not raise the budget it draws from.
:::

The plan allows {{ requests }} requests a month.

{% for row in plans %}
- {{ row }}
{% endfor %}

```bash
liyasa build --strict
```

<div class=\"legacy\">Raw HTML the visual editor does not model.</div>

Last paragraph.
";

fn context() -> serde_json::Value {
    serde_json::json!({ "requests": "250,000", "plans": ["Free", "Pro", "Team"] })
}

const NONCE: &str = "0123456789abcdef0123456789abcdef";

fn wasm_site() -> WasmSiteMeta {
    WasmSiteMeta {
        name: "Acme".to_owned(),
        canonical_origin: "https://example.invalid".to_owned(),
        llms_txt: "https://example.invalid/llms.txt".to_owned(),
        version: None,
        locale: "en".to_owned(),
    }
}

/// The same metadata, converted the way `liyasa_wasm::session::site` converts
/// it. If the two ever disagree the comparison below is between two different
/// sites rather than between two renders, so it is spelled out here.
fn core_site() -> SiteMeta {
    let meta = wasm_site();
    SiteMeta {
        name: meta.name.clone(),
        canonical_origin: Url::parse(&meta.canonical_origin).expect("an absolute origin"),
        llms_txt: Url::parse(&meta.llms_txt).expect("an absolute llms.txt"),
        version: None,
        locale: Locale::new(&meta.locale),
    }
}

/// What the editor shows.
fn previewed(source: &str) -> liyasa_wasm::api::PreviewResponse {
    let session = Session::sealed(&OpenRequest {
        nonce: NONCE.to_owned(),
        site: wasm_site(),
        seed: Vec::new(),
    })
    .expect("a session with a well-formed nonce opens");
    session.preview(&PreviewRequest {
        path: "guides/limits.md".to_owned(),
        source: source.to_owned(),
        context: context(),
        options: Default::default(),
    })
}

/// What a build writes.
///
/// `resolve: None` is the same choice the preview makes and the render
/// module's own documentation names: "renders the AST as written, which is
/// what a preview of a single page wants". A build of a whole site resolves
/// links against its route table, which a single-page preview has no way to
/// know — and that difference is the subject of its own test below.
fn built(source: &str) -> render::Page {
    let registry = Registry::builtins();
    let site = core_site();
    let mut map = SourceMap::new();
    let id = map.intern(VfsPath::new("guides/limits.md"), Arc::from(source));
    let (document, _) = liyasa_markdown::scan(source, id);

    let mut nonce = [0u8; 16];
    for (at, byte) in nonce.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&NONCE[at * 2..at * 2 + 2], 16).expect("hex");
    }

    let mut options = render::Options::new(&registry, &site);
    options.parse.build_nonce = nonce;
    // Built through `Layers` rather than by naming `minijinja` here: a flat
    // object at `site_variables` lands at the root, which is what
    // `Session::expand` does with the context it is handed.
    let template = Layers {
        site_variables: context(),
        ..Layers::default()
    }
    .build();
    render::page(&map, &document, &template, &options)
}

#[test]
fn the_editors_preview_is_the_builds_html_byte_for_byte() {
    let preview = previewed(PAGE);
    let page = built(PAGE);

    assert!(
        preview.diagnostics.is_empty(),
        "the fixture does not preview cleanly: {:?}",
        preview
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
    // The fixture is the defect if it cannot reach the state the assertion is
    // about: a page that rendered to nothing would compare equal to nothing.
    assert!(
        preview.html.contains("Every project has caps"),
        "prose rendered"
    );
    assert!(preview.html.contains("Heads up"), "the directive rendered");
    assert!(preview.html.contains("250,000"), "the chip resolved");
    assert!(preview.html.contains("Team"), "the loop expanded");
    assert!(
        preview.html.contains("liyasa build --strict"),
        "the fence rendered"
    );

    assert_eq!(preview.html, page.html, "html");
}

#[test]
fn the_editors_markdown_and_text_are_the_builds_too() {
    // §11.7's Markdown and the plain text come from the same parse as the
    // HTML, on both sides. If one of the three drifted, an agent fetching
    // `<route>.md` would read a different page from the one a reader sees.
    let preview = previewed(PAGE);
    let page = built(PAGE);
    assert_eq!(preview.markdown, page.markdown, "markdown");
    assert_eq!(preview.text, page.text, "text");
}

#[test]
fn the_preview_and_the_build_agree_on_every_fixture_page() {
    let root = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../web/editor/test/fixtures/pages"
    );
    let mut seen = 0usize;
    for entry in std::fs::read_dir(root).expect("the fixture pages are readable") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_none_or(|extension| extension != "md") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("a fixture page is readable");
        let preview = previewed(&source);
        let page = built(&source);
        assert_eq!(preview.html, page.html, "html for {}", path.display());
        assert_eq!(
            preview.markdown,
            page.markdown,
            "markdown for {}",
            path.display()
        );
        seen += 1;
    }
    assert!(seen >= 3, "the fixture pages were not read");
}

#[test]
fn a_page_with_a_link_previews_as_written_because_the_editor_has_no_route_table() {
    // The one place the preview and a *site* build legitimately differ. The
    // build resolves `[a](/guides/limits)` against its route table (CM-35,
    // CM-36); a single-page preview has no table, and `render::Options` says
    // so: `resolve: None` "renders the AST as written, which is what a preview
    // of a single page wants".
    //
    // Asserting it here rather than leaving it implicit is the point. If the
    // editor is ever expected to show resolved links, this is the test that
    // says what has to change, and it is a decision rather than a discovery
    // somebody makes from a wrong preview.
    let source = "See [the limits](/guides/limits) for more.\n";
    let preview = previewed(source);
    let page = built(source);
    assert_eq!(preview.html, page.html);
    assert!(
        preview.html.contains("/guides/limits"),
        "the href is the one the author wrote: {}",
        preview.html
    );
}

#[test]
fn the_scanner_emits_only_the_kinds_the_editors_six_node_kinds_are_built_from() {
    // `web/editor/src/model.ts` dispatches on six segment kinds. A seventh
    // would fall through to `opaque`, which is a silent loss of modelling
    // rather than a failure, so it is asserted here instead.
    let mut map = SourceMap::new();
    let id = map.intern(VfsPath::new("guides/limits.md"), Arc::from(PAGE));
    let (document, _) = liyasa_markdown::scan(PAGE, id);

    let mut kinds = std::collections::BTreeSet::new();
    for segment in &document.segments {
        kinds.insert(match segment {
            Segment::Markdown { .. } => "markdown",
            Segment::Code { .. } => "code",
            Segment::Template { .. } => "template",
            Segment::DirectiveOpen { .. } => "directiveOpen",
            Segment::DirectiveClose { .. } => "directiveClose",
            Segment::DirectiveLeaf { .. } => "directiveLeaf",
        });
    }
    // Every kind in the fixture is one the editor models.
    for kind in &kinds {
        assert!(
            [
                "markdown",
                "code",
                "template",
                "directiveOpen",
                "directiveClose",
                "directiveLeaf"
            ]
            .contains(kind),
            "`{kind}` is a segment kind `web/editor/src/model.ts` does not dispatch on"
        );
    }
    // And the fixture really does exercise most of them, or the loop above
    // asserted almost nothing.
    assert!(kinds.len() >= 5, "the fixture produced only {kinds:?}");
}
