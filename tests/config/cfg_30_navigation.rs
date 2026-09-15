//! CFG-30: every navigation node form validates, a page nobody navigates to is
//! `W0130`, and a path that does not exist is `E0104`.

use liyasa_config::Mode;
use liyasa_tests::config::{checked, codes, one, project};

/// One of every node form in §8.4, in one tree.
const CONFIG: &str = r##"{
  "name": "Acme",
  "seo": { "canonicalOrigin": "https://acme.dev" },
  "versions": [{ "name": "v2", "default": true }, { "name": "v1" }],
  "locales": [{ "code": "en", "default": true }, { "code": "de" }],
  "dimensions": [{ "name": "product", "values": ["cloud", "self-hosted"], "default": "cloud" }],
  "openapi": [{ "id": "api", "source": "openapi/api.yaml" }],
  "asyncapi": [{ "id": "events", "source": "asyncapi/events.yaml" }],
  "graphql": [{ "id": "graph", "source": "graph/schema.graphql" }],
  "navigation": {
    "breadcrumbs": "eyebrow",
    "pages": [
      "index",
      { "divider": "Product" },
      { "tab": "Guides", "icon": "book", "pages": [
        { "group": "Get started", "icon": "rocket", "tag": "new", "expanded": true,
          "root": "guides/index", "pages": ["guides/install"] },
        { "directory": "reference" },
        { "group": "Runbooks", "groups": ["staff"], "hidden": true, "pages": ["internal/runbook"] }
      ]},
      { "tab": "API", "pages": [
        { "openapi": "api", "groupBy": "tag", "operations": "all" },
        { "asyncapi": "events" },
        { "graphql": "graph" },
        { "sdk": "typescript", "source": "sdk/ts" }
      ]},
      { "menu": "More", "items": ["menu/overview"] },
      { "dropdown": "Products", "items": [{ "product": "cloud", "pages": ["products/cloud"] }] },
      { "version": "v1", "pages": ["v1/legacy"] },
      { "language": "de", "pages": ["de/willkommen"] },
      { "anchor": "Community", "href": "https://acme.dev/community", "icon": "users", "color": "#4338CA" },
      { "link": "Status", "href": "https://status.acme.dev", "icon": "activity" }
    ]
  }
}"##;

const PAGES: &[(&str, &str)] = &[
    ("index.md", "# Home"),
    ("guides/index.md", "# Guides"),
    ("guides/install.md", "# Install"),
    ("reference/cli.md", "# CLI"),
    ("internal/runbook.md", "# Runbook"),
    ("menu/overview.md", "# Overview"),
    ("products/cloud.md", "# Cloud"),
    ("v1/legacy.md", "# Legacy"),
    ("de/willkommen.md", "# Willkommen"),
];

fn project_with(config: &str) -> liyasa_config::Checked {
    let mut files = vec![("liyasa.json", config)];
    files.extend_from_slice(PAGES);
    project(&files)
}

#[test]
fn every_node_form_validates() {
    let checked = project_with(CONFIG);
    assert_eq!(codes(&checked), Vec::<&str>::new());
    assert_eq!(checked.pages.len(), PAGES.len());
}

#[test]
fn a_page_outside_the_navigation_is_w0130() {
    let config = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev" },
                       "navigation": ["index"] }"##;
    let checked = project(&[
        ("liyasa.json", config),
        ("index.md", "# Home"),
        ("orphan.md", "# Orphan"),
    ]);
    assert_eq!(codes(&checked), ["W0130"]);
    assert!(
        one(&checked, "W0130").message.contains("orphan"),
        "the warning names the page"
    );
}

#[test]
fn autofill_answers_the_warning_during_dev() {
    let config = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev" },
                       "navigation": { "autofill": true, "pages": ["index"] } }"##;
    let files = [
        ("liyasa.json", config),
        ("index.md", "# Home"),
        ("orphan.md", "# Orphan"),
    ];
    assert_eq!(codes(&checked(&files, Mode::Dev)), Vec::<&str>::new());
    assert_eq!(codes(&checked(&files, Mode::Build)), ["W0130"]);
}

#[test]
fn a_page_that_does_not_exist_is_e0104() {
    let config = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev" },
                       "navigation": ["index", "guides/missing"] }"##;
    let checked = project(&[("liyasa.json", config), ("index.md", "# Home")]);
    assert_eq!(codes(&checked), ["E0104"]);
}

#[test]
fn a_subtree_bound_to_an_undeclared_axis_is_e0133() {
    let config = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev" },
                       "navigation": [{ "version": "v9", "pages": ["index"] }] }"##;
    let checked = project(&[("liyasa.json", config), ("index.md", "# Home")]);
    assert_eq!(codes(&checked), ["E0133"]);
}
