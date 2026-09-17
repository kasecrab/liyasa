//! The editor session: the four calls `web/editor` makes, and what it is told
//! when it cannot have them.

use std::collections::BTreeMap;

use liyasa_core::vfs::{Bytes, VfsPath};
use liyasa_wasm::api::{
    OpenRequest, ParseRequest, PreviewRequest, SeedEntry, SerializeRequest, SiteMeta,
    ValidateRequest,
};
use liyasa_wasm::session::Session;
use liyasa_wasm::vfs::{Fetch, PRELOAD_LIMIT};

const NONCE: &str = "0123456789abcdef0123456789abcdef";

struct Draft(BTreeMap<VfsPath, Bytes>);

impl Draft {
    fn with(files: &[(&str, &str)]) -> Box<Self> {
        Box::new(Self(
            files
                .iter()
                .map(|(path, text)| {
                    (VfsPath::new(path), Bytes::from(text.as_bytes().to_vec()))
                })
                .collect(),
        ))
    }
}

impl Fetch for Draft {
    fn fetch(&self, path: &VfsPath) -> Option<Bytes> {
        self.0.get(path).cloned()
    }
}

fn site() -> SiteMeta {
    SiteMeta {
        name: "Acme docs".to_owned(),
        canonical_origin: "https://docs.acme.com".to_owned(),
        llms_txt: "https://docs.acme.com/llms.txt".to_owned(),
        version: None,
        locale: "en".to_owned(),
    }
}

fn request(seed: &[(&str, &str)]) -> OpenRequest {
    OpenRequest {
        nonce: NONCE.to_owned(),
        site: site(),
        seed: seed
            .iter()
            .map(|(path, text)| SeedEntry {
                path: (*path).to_owned(),
                text: (*text).to_owned(),
            })
            .collect(),
    }
}

fn session() -> Session {
    Session::sealed(&request(&[])).expect("the request is complete")
}

fn codes(diagnostics: &liyasa_core::diagnostics::Diagnostics) -> Vec<&str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

fn preview(session: &Session, source: &str) -> liyasa_wasm::api::PreviewResponse {
    session.preview(&PreviewRequest {
        path: "install.md".to_owned(),
        source: source.to_owned(),
        context: serde_json::json!({
            "site": { "name": "Acme" },
            "page": { "title": "Install" },
        }),
        ..PreviewRequest::default()
    })
}

#[test]
fn a_page_becomes_html_markdown_and_text() {
    let response = preview(
        &session(),
        "---\ntitle: Install\n---\n# Install\n\nRun the installer.\n",
    );
    assert!(response.html.contains("<h1"), "{}", response.html);
    assert!(response.html.contains("Run the installer."));
    assert!(response.markdown.contains("Run the installer."));
    assert!(response.text.contains("Run the installer."));
    assert!(response.diagnostics.is_empty(), "{:?}", response.diagnostics);
    assert!(!response.server_render);
}

#[test]
fn a_template_expression_is_expanded_before_the_markdown_is_parsed() {
    let response = preview(&session(), "Welcome to {{ site.name }}.\n");
    assert!(response.html.contains("Welcome to Acme."), "{}", response.html);
    assert!(!response.html.contains("{{"));
}

#[test]
fn a_directive_renders_through_the_component_registry() {
    let response = preview(&session(), ":::note\nMind the gap.\n:::\n");
    assert!(response.html.contains("Mind the gap."), "{}", response.html);
    assert!(response.html.contains("ly-callout"), "{}", response.html);
}

#[test]
fn an_undefined_name_is_reported_rather_than_rendered_as_nothing() {
    let response = preview(&session(), "Hello {{ nobody.at.all }}.\n");
    assert!(
        response.diagnostics.iter().any(|d| d.code.as_str() == "E0201"),
        "{:?}",
        response.diagnostics
    );
    assert!(response.html.is_empty());
}

/// The guard on `session::includes_in`: the file is not seeded, so the preview
/// can only contain its text if the session read the include statement the same
/// way `liyasa_markdown::source::expand` does and fetched it before expanding.
#[test]
fn an_include_outside_the_seed_is_fetched_before_expansion() {
    let session = Session::open(
        &request(&[]),
        Draft::with(&[("snippets/note.md", "Mind the gap.\n")]),
    )
    .expect("the request is complete");

    let response = session.preview(&PreviewRequest {
        path: "install.md".to_owned(),
        source: "{% include \"snippets/note.md\" %}\n".to_owned(),
        ..PreviewRequest::default()
    });
    assert!(
        response.html.contains("Mind the gap."),
        "the include was not resolved: {} {:?}",
        response.html,
        response.diagnostics
    );
    assert_eq!(session.status().fetched, 1);
}

#[test]
fn parse_returns_both_representations_of_the_draft() {
    let response = session().parse(&ParseRequest {
        path: "install.md".to_owned(),
        source: "---\ntitle: Install\n---\n# Install\n".to_owned(),
        ..ParseRequest::default()
    });
    assert!(response.source.frontmatter.is_some(), "the source mode's");
    assert!(!response.source.segments.is_empty());
    let document = response.document.expect("the visual mode's");
    assert!(!document.root.children.is_empty());
    assert!(response.diagnostics.is_empty(), "{:?}", response.diagnostics);
}

#[test]
fn serialize_is_byte_preserving_outside_the_edits() {
    let source = "# Install\n\nRun   the installer.\n";
    let response = session().serialize(&SerializeRequest {
        path: "install.md".to_owned(),
        source: source.to_owned(),
        ..SerializeRequest::default()
    });
    assert_eq!(response.text, source, "an untouched page was rewritten");
}

#[test]
fn serialize_formats_only_when_it_is_asked_to() {
    let source = "#    Install\n";
    let response = session().serialize(&SerializeRequest {
        path: "install.md".to_owned(),
        source: source.to_owned(),
        format: true,
        ..SerializeRequest::default()
    });
    assert!(response.diagnostics.is_empty(), "{:?}", response.diagnostics);
    let again = session().serialize(&SerializeRequest {
        path: "install.md".to_owned(),
        source: response.text.clone(),
        format: true,
        ..SerializeRequest::default()
    });
    assert_eq!(again.text, response.text, "the formatter is not idempotent");
}

#[test]
fn a_session_without_a_usable_nonce_is_refused() {
    let mut empty = request(&[]);
    empty.nonce = String::new();
    let refused = Session::sealed(&empty).expect_err("an empty nonce is not a nonce");
    assert!(codes(&refused).contains(&"E1200"), "{refused:?}");

    let mut not_hex = request(&[]);
    not_hex.nonce = "z".repeat(32);
    let refused = Session::sealed(&not_hex).expect_err("`z` is not hexadecimal");
    assert!(codes(&refused).contains(&"E1200"), "{refused:?}");
}

#[test]
fn site_metadata_that_is_not_a_url_is_refused() {
    let mut request = request(&[]);
    request.site.canonical_origin = "docs.acme.com".to_owned();
    let refused = Session::sealed(&request).expect_err("a bare host is not an origin");
    assert!(codes(&refused).contains(&"E1200"), "{refused:?}");
    assert!(
        refused
            .iter()
            .any(|diagnostic| diagnostic.message.contains("canonical_origin")),
        "{refused:?}"
    );
}

#[test]
fn a_page_over_the_preload_cap_previews_on_the_server() {
    let big = "x".repeat(PRELOAD_LIMIT as usize + 1);
    let session = Session::sealed(&request(&[("index.md", &big)])).expect("opens");
    assert!(session.status().server_render);

    let response = preview(&session, "# Install\n");
    assert!(response.server_render);
    assert!(response.html.is_empty(), "it rendered anyway");
    assert!(
        response.diagnostics.iter().any(|d| d.code.as_str() == "W1201"),
        "{:?}",
        response.diagnostics
    );
}

#[test]
fn validate_reports_a_config_that_is_not_json() {
    let response = session().validate(&ValidateRequest {
        config: Some("{ not json".to_owned()),
        ..ValidateRequest::default()
    });
    assert!(
        response.diagnostics.iter().any(|d| d.code.as_str() == "E0101"),
        "{:?}",
        response.diagnostics
    );
    assert!(response.config.is_none());
}

#[test]
fn validate_does_not_invent_missing_pages_when_it_has_no_routes() {
    let config = r#"{"name":"Acme","navigation":["install","upgrade"]}"#;
    let blind = session().validate(&ValidateRequest {
        config: Some(config.to_owned()),
        ..ValidateRequest::default()
    });
    assert!(
        !blind.diagnostics.iter().any(|d| d.code.as_str() == "E0104"),
        "every navigation entry was reported against an empty route set: {:?}",
        blind.diagnostics
    );

    let told = session().validate(&ValidateRequest {
        config: Some(config.to_owned()),
        routes: vec!["install".to_owned()],
        ..ValidateRequest::default()
    });
    assert!(
        told.diagnostics.iter().any(|d| d.code.as_str() == "E0104"),
        "`upgrade` is not a route and was not reported: {:?}",
        told.diagnostics
    );
}

#[test]
fn validate_reads_front_matter_through_the_scanner() {
    let response = session().validate(&ValidateRequest {
        frontmatter: Some("title: Install\nsidebarTitle: [unclosed\n".to_owned()),
        ..ValidateRequest::default()
    });
    assert!(
        !response.diagnostics.is_empty(),
        "malformed YAML was accepted"
    );
}

/// ED-07 the way the browser uses it: nothing calls back into JavaScript. The
/// response names the path, the host fetches it, hands it over, and asks again.
#[test]
fn a_path_the_session_does_not_hold_is_named_so_the_host_can_fetch_it() {
    let session = session();
    let source = "{% include \"snippets/note.md\" %}\n";

    let first = session.preview(&PreviewRequest {
        path: "install.md".to_owned(),
        source: source.to_owned(),
        ..PreviewRequest::default()
    });
    assert_eq!(first.missing, vec!["snippets/note.md".to_owned()]);
    assert!(!first.html.contains("Mind the gap."));

    session.seed("snippets/note.md", "Mind the gap.\n");

    let second = session.preview(&PreviewRequest {
        path: "install.md".to_owned(),
        source: source.to_owned(),
        ..PreviewRequest::default()
    });
    assert!(second.missing.is_empty(), "{:?}", second.missing);
    assert!(
        second.html.contains("Mind the gap."),
        "{} {:?}",
        second.html,
        second.diagnostics
    );
}

#[test]
fn a_seeded_page_needs_nothing_and_says_so() {
    let response = preview(&session(), "# Install\n");
    assert!(response.missing.is_empty(), "{:?}", response.missing);
}
