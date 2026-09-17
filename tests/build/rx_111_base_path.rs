//! RX-111: a site hosted under `/docs` gets host files whose every path, rule,
//! and redirect carries the base path, and pages whose asset links do too.

use liyasa_build::engine::Options;
use liyasa_build::hosting::{HEADERS_FILE, REDIRECTS_FILE};
use liyasa_core::ids::Route;
use liyasa_tests::hosting::{CONFIG, Site};

fn under_docs(name: &str) -> Site {
    Site::build(
        name,
        CONFIG,
        Options {
            base_path: Some("/docs".to_owned()),
            ..Options::default()
        },
    )
}

#[test]
fn every_rule_and_redirect_is_under_the_base_path() {
    let site = under_docs("rx111-rules");
    let headers = site.read(HEADERS_FILE);
    for line in headers.lines().filter(|line| line.starts_with('/')) {
        assert!(line.starts_with("/docs/"), "{line}");
    }
    assert!(headers.starts_with("/docs/*\n"));
    assert!(headers.contains("\n/docs/embed/widget/*\n"));
    assert_eq!(
        site.read(REDIRECTS_FILE),
        "/docs/old /docs/guides/install 301\n"
    );
    let vercel = site.vercel();
    for rule in vercel["headers"].as_array().expect("header rules") {
        let source = rule["source"].as_str().expect("a source");
        assert!(source.starts_with("/docs/"), "{source}");
    }
    assert_eq!(vercel["redirects"][0]["source"], "/docs/old");
    assert_eq!(
        vercel["redirects"][0]["destination"],
        "/docs/guides/install"
    );
    assert!(site.output.rules.resolve("/guides/install/").is_empty());
    assert!(
        !site
            .output
            .rules
            .resolve("/docs/guides/install/")
            .is_empty()
    );
}

#[test]
fn the_build_rewrites_its_own_absolute_paths() {
    let site = under_docs("rx111-pages");
    let manifest = site.report.manifest.as_ref().expect("a manifest");
    assert_eq!(manifest.base_path, "/docs");
    let install = manifest
        .route(&Route::new("/guides/install"))
        .expect("the guide");
    assert_eq!(install.markdown, "/docs/guides/install.md");
    let home = site.read("index.html");
    assert!(home.contains("href=\"/docs/_liyasa/theme."), "{home}");
    assert!(!home.contains("href=\"/_liyasa/"), "{home}");
}

/// RFC 1006: the agent surfaces publish absolute URLs, and those are built from
/// `seo.canonicalOrigin` — which carries no prefix — rather than from the route
/// the site is actually served at. A reader under `/docs` followed them to
/// nothing.
#[test]
fn every_agent_surface_url_carries_the_base_path() {
    let site = under_docs("rx111-surfaces");
    let origin = "https://docs.acme.com";
    for surface in ["llms.txt", "llms-full.txt"] {
        for line in site.read(surface).lines() {
            for at in line.match_indices(origin).map(|(at, _)| at) {
                let url = line[at..]
                    .split([' ', ')', '>', '"', ','])
                    .next()
                    .expect("a URL");
                assert!(
                    url == origin || url.starts_with("https://docs.acme.com/docs"),
                    "{surface}: {url}"
                );
            }
        }
    }
}
