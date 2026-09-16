//! MIG-05. Every importer carries sidebars, versions, locales, assets, OpenAPI
//! specs, and redirects; and when the structure changes, it generates a
//! redirect from the old URL to the new one.

use liyasa_config::vfs::MemVfs;
use liyasa_core::vfs::VfsPath;
use liyasa_import::stubs::Stubs;
use liyasa_import::{docusaurus, mdx, mintlify};

use crate::support::{Builtins, config, paths, validate};

const MINTLIFY: &str = r##"{
  "name": "Acme",
  "redirects": [{ "source": "/old", "destination": "/new" }],
  "openapi": ["openapi/acme.yaml"],
  "navigation": {
    "versions": [
      { "version": "v2", "groups": [{ "group": "Start", "pages": ["v2/index"] }] },
      { "version": "v1", "groups": [{ "group": "Start", "pages": ["v1/index"] }] }
    ],
    "languages": [
      { "language": "en", "pages": ["index"] },
      { "language": "de", "pages": ["de/index"] }
    ]
  }
}
"##;

fn mintlify_project() -> MemVfs {
    MemVfs::new()
        .with("docs.json", MINTLIFY.as_bytes().to_vec())
        .with("index.mdx", b"---\ntitle: Home\n---\n\nHi.\n".to_vec())
        .with("v1/index.mdx", b"---\ntitle: V1\n---\n\nOld.\n".to_vec())
        .with("v2/index.mdx", b"---\ntitle: V2\n---\n\nNew.\n".to_vec())
        .with(
            "de/index.mdx",
            b"---\ntitle: Start\n---\n\nHallo.\n".to_vec(),
        )
        .with(
            "openapi/acme.yaml",
            b"openapi: 3.1.0\ninfo:\n  title: Acme\n  version: '1'\npaths: {}\n".to_vec(),
        )
        .with("images/logo.svg", b"<svg/>".to_vec())
        .with("assets/brochure.pdf", b"%PDF".to_vec())
}

fn mintlify_plan(vfs: &MemVfs) -> liyasa_import::Plan {
    mintlify::import(
        vfs,
        &VfsPath::new(""),
        &mintlify::Options {
            components: &Builtins::default(),
            directives: false,
            mapping: &Stubs,
        },
    )
}

#[test]
fn mintlify_carries_versions_locales_specs_assets_and_redirects() {
    let source = mintlify_project();
    let plan = mintlify_plan(&source);
    let config = config(&plan);

    assert_eq!(config["versions"][0]["name"], "v2");
    assert_eq!(config["versions"][0]["default"], true);
    assert_eq!(config["versions"][1]["name"], "v1");
    assert_eq!(config["locales"][0]["code"], "en");
    assert_eq!(config["locales"][1]["code"], "de");
    assert_eq!(config["openapi"][0], "openapi/acme.yaml");
    assert_eq!(config["redirects"][0]["source"], "/old");

    let paths = paths(&plan);
    assert!(paths.contains(&"openapi/acme.yaml"), "{paths:?}");
    assert!(paths.contains(&"images/logo.svg"));
    assert!(paths.contains(&"assets/brochure.pdf"));
    assert!(validate(&plan, &source).is_empty());
}

#[test]
fn a_navigation_entry_with_no_page_is_named_rather_than_left_to_the_build() {
    let source = mintlify_project();
    let mut broken = source.clone();
    broken.insert(
        VfsPath::new("docs.json"),
        MINTLIFY
            .replace("\"v1/index\"", "\"v1/missing\"")
            .into_bytes(),
    );
    let plan = mintlify_plan(&broken);

    let named: Vec<&str> = plan
        .report
        .attention
        .iter()
        .map(|item| item.what.as_str())
        .collect();
    assert!(named.contains(&"v1/missing"), "{named:?}");
}

#[test]
fn a_page_that_keeps_its_route_gets_no_redirect() {
    let source = mintlify_project();
    let plan = mintlify_plan(&source);
    assert!(
        plan.report.redirects.is_empty(),
        "nothing moved, so nothing needs a redirect: {:?}",
        plan.report.redirects
    );
}

#[test]
fn docusaurus_carries_its_sidebar_and_generates_redirects_for_what_moved() {
    let source = MemVfs::new()
        .with(
            "docusaurus.config.js",
            b"module.exports = { title: 'Acme', presets: [['classic', { docs: { routeBasePath: 'manual' } }]] };"
                .to_vec(),
        )
        .with(
            "sidebars.js",
            b"module.exports = { docs: ['intro'] };".to_vec(),
        )
        .with("docs/intro.md", b"---\ntitle: Intro\n---\n\nHi.\n".to_vec())
        .with(
            "versioned_docs/version-1.0/intro.md",
            b"---\ntitle: Intro\n---\n\nOld.\n".to_vec(),
        )
        .with("static/img/logo.svg", b"<svg/>".to_vec());

    let plan = docusaurus::import(
        &source,
        &VfsPath::new(""),
        &docusaurus::Options {
            components: &Builtins::default(),
            directives: false,
            mapping: &Stubs,
        },
    );

    // The page follows `routeBasePath`, so its URL is unchanged and it needs no
    // redirect; the archived version's URL does change, so it gets one.
    assert!(
        paths(&plan).contains(&"manual/intro.md"),
        "{:?}",
        paths(&plan)
    );
    assert_eq!(config(&plan)["navigation"][0], "manual/intro");
    let redirects: Vec<&str> = plan
        .report
        .redirects
        .iter()
        .map(|rule| rule.source.as_str())
        .collect();
    assert_eq!(redirects, ["/manual/1.0/intro"], "{redirects:?}");
    assert!(validate(&plan, &source).is_empty());
}

#[test]
fn the_generic_importer_carries_specs_and_assets_too() {
    let source = MemVfs::new()
        .with("index.mdx", b"# Hi\n".to_vec())
        .with("openapi/acme.yaml", b"openapi: 3.1.0\n".to_vec())
        .with("images/logo.svg", b"<svg/>".to_vec());
    let plan = mdx::import(
        &source,
        &VfsPath::new(""),
        &mdx::Options {
            components: &Builtins::default(),
            mapping: &Stubs,
            directives: false,
            name: "Acme",
        },
    );
    let paths = paths(&plan);
    assert!(paths.contains(&"openapi/acme.yaml"), "{paths:?}");
    assert!(paths.contains(&"images/logo.svg"));
}
