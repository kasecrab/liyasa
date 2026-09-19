//! AST-10: what the server supplies to the assistant.
//!
//! `Tools` had one implementor in the workspace and it was WP-18's test fake,
//! so a reader could not ask anything. These exercise the real one against a
//! real built bundle.
//!
//! **Read the deflection note before reading a failure here.** A correct
//! `search` over a site with no index returns an empty result, the assistant
//! scores that at 0.0 and hedges, and every answer deflects. That is the
//! expected behaviour of correct tools over an empty index, not a broken
//! implementation (RFC 1804, and defect 20 for why the index is empty).

use std::sync::Arc;

use liyasa_ai::assistant::tools::Tools;
use liyasa_ai::index::ChunkQuery;
use liyasa_core::ids::Route;
use liyasa_server::routes::bundle::Bundle;
use liyasa_server::routes::tools::ServerTools;

/// A built fixture site, and tools over it for a reader with no entitlements.
fn tools(name: &str) -> (liyasa_tests::hosting::Site, ServerTools) {
    let site = liyasa_tests::hosting::Site::build(
        name,
        r#"{
          "name": "Acme docs",
          "description": "How Acme works",
          "seo": { "canonicalOrigin": "https://docs.acme.com" }
        }"#,
        liyasa_build::engine::Options::default(),
    );
    let bundle = Arc::new(Bundle::open(&site.dist()).expect("the fixture bundle"));
    let tools = ServerTools::new(bundle, ChunkQuery::default());
    (site, tools)
}

#[tokio::test]
async fn a_page_is_read_out_of_the_bundle_the_build_wrote() {
    let (_site, tools) = tools("tools-page");
    let page = tools
        .get_page(&Route::new("/guides/install"), None)
        .await
        .expect("a read")
        .expect("the page");

    assert_eq!(page.route, "/guides/install");
    assert_eq!(
        page.title, "Install",
        "the title is the page's first heading"
    );
    assert_eq!(page.anchor, "", "no anchor means the whole page");
    assert!(page.markdown.contains("Run it."), "{}", page.markdown);

    assert!(
        tools
            .get_page(&Route::new("/no/such/page"), None)
            .await
            .expect("a read")
            .is_none(),
        "a route that is not in the bundle is None rather than an error"
    );
}

#[tokio::test]
async fn a_section_is_the_heading_and_what_follows_it() {
    let site = liyasa_tests::hosting::Site::build(
        "tools-section",
        r#"{"name":"Acme docs","description":"d","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
        liyasa_build::engine::Options::default(),
    );
    // The fixture's pages are short, so write the shape this is about.
    let dist = site.dist();
    std::fs::write(
        dist.join("guides/install.md"),
        "# Install\n\nLead.\n\n## Debian\n\nApt line.\n\n## macOS\n\nBrew line.\n",
    )
    .expect("a twin");
    let bundle = Arc::new(Bundle::open(&dist).expect("a bundle"));
    let tools = ServerTools::new(bundle, ChunkQuery::default());

    let debian = tools
        .get_page(&Route::new("/guides/install"), Some("debian"))
        .await
        .expect("a read")
        .expect("the section");
    assert_eq!(debian.anchor, "debian");
    assert!(debian.markdown.contains("Apt line."));
    assert!(
        !debian.markdown.contains("Brew line."),
        "the next sibling heading ends the section: {}",
        debian.markdown
    );
    assert!(!debian.markdown.contains("Lead."));

    assert!(
        tools
            .get_page(&Route::new("/guides/install"), Some("windows"))
            .await
            .expect("a read")
            .is_none(),
        "an anchor that is not on the page is None"
    );
}

#[tokio::test]
async fn the_navigation_is_the_one_the_build_published() {
    let (_site, tools) = tools("tools-nav");
    let nav = tools.list_navigation().await.expect("a navigation");
    assert!(!nav.is_empty(), "the fixture writes llms.txt");

    let install = nav
        .iter()
        .find(|entry| entry.route == "/guides/install")
        .unwrap_or_else(|| panic!("no install entry in {nav:?}"));
    assert_eq!(install.title, "Install");
    assert_eq!(install.depth, 1);
    assert!(
        nav.iter()
            .any(|entry| entry.depth == 0 && entry.route.is_empty()),
        "sections are depth 0 with no route: {nav:?}"
    );
    // Routes, not URLs, and no `.md` suffix: the tool answers in the terms the
    // other tools take.
    for entry in nav.iter().filter(|e| e.depth > 0) {
        assert!(entry.route.starts_with('/'), "{entry:?}");
        assert!(!entry.route.ends_with(".md"), "{entry:?}");
        assert!(!entry.route.contains("://"), "{entry:?}");
    }
}

#[tokio::test]
async fn every_navigation_route_can_be_read_back() {
    // The two tools have to agree: a route the assistant is told about and
    // then cannot open is worse than not being told.
    let (_site, tools) = tools("tools-roundtrip");
    let nav = tools.list_navigation().await.expect("a navigation");
    for entry in nav.iter().filter(|e| e.depth > 0) {
        let page = tools
            .get_page(&Route::new(&entry.route), None)
            .await
            .expect("a read");
        assert!(
            page.is_some(),
            "`{}` is in the navigation and get_page cannot open it",
            entry.route
        );
    }
}

#[tokio::test]
async fn search_over_a_site_with_no_index_finds_nothing_and_does_not_fail() {
    // This is the behaviour WP-18's acceptance signal depends on. An
    // `Unavailable` here would make an unindexed site look broken and would
    // change how the assistant answers.
    let (_site, tools) = tools("tools-search");
    let hits = tools
        .search("how do I install it", &ChunkQuery::default())
        .await
        .expect("an empty index is not a failure");
    assert!(hits.is_empty());
}

#[tokio::test]
async fn the_current_page_is_the_one_the_reader_is_on() {
    let (site, _) = tools("tools-current");
    let bundle = Arc::new(Bundle::open(&site.dist()).expect("a bundle"));

    let nowhere = ServerTools::new(bundle.clone(), ChunkQuery::default());
    assert!(
        nowhere.get_current_page().await.expect("a read").is_none(),
        "a request that did not say where the reader is has no current page"
    );

    let somewhere =
        ServerTools::new(bundle, ChunkQuery::default()).on_page(Route::new("/guides/install"));
    let page = somewhere
        .get_current_page()
        .await
        .expect("a read")
        .expect("the page");
    assert_eq!(page.route, "/guides/install");
}

#[tokio::test]
async fn a_route_outside_the_readers_filter_is_not_readable() {
    // RFC 1807: entitlement is applied here and never by the model. A page
    // the filter excludes is not there as far as this reader is concerned.
    let (site, _) = tools("tools-filter");
    let bundle = Arc::new(Bundle::open(&site.dist()).expect("a bundle"));
    let filter = ChunkQuery {
        routes: vec![Route::new("/guides/install")],
        ..ChunkQuery::default()
    };
    let tools = ServerTools::new(bundle, filter);

    assert!(
        tools
            .get_page(&Route::new("/guides/install"), None)
            .await
            .expect("a read")
            .is_some(),
        "the allowed route is readable"
    );
    assert!(
        tools
            .get_page(&Route::new("/"), None)
            .await
            .expect("a read")
            .is_none(),
        "a route outside the filter reads as absent, not as a refusal"
    );
}

#[tokio::test]
async fn an_operation_with_no_page_is_none_rather_than_an_error() {
    // The fixture documents no API, so every operation is a miss. A site with
    // one renders each operation onto a page and this finds it by route.
    let (_site, tools) = tools("tools-openapi");
    assert!(
        tools
            .get_openapi("GET /payments")
            .await
            .expect("a read")
            .is_none()
    );
}
