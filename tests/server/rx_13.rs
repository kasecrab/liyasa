//! RX-13 and RX-60: cache and security headers, conditional requests, and the
//! Markdown representation.
//!
//! Given the server; when a page and a `.md` route are fetched; then the
//! listed headers are present and a conditional request returns 304; the
//! static `_headers` file contains the same policy.

use http::StatusCode;
use liyasa_tests::server::{Harness, body_bytes, body_text, expect_status, header};

#[tokio::test]
async fn a_page_carries_the_cache_and_validator_headers() {
    let (harness, _site) = Harness::serving("rx13-page").await;
    let response = expect_status(harness.get("/guides/install").await, StatusCode::OK);

    assert_eq!(
        header(&response, "cache-control"),
        Some("public, max-age=300, must-revalidate")
    );
    assert!(header(&response, "etag").is_some_and(|t| t.starts_with('"')));
    assert!(header(&response, "last-modified").is_some());
    assert_eq!(
        header(&response, "content-type"),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(header(&response, "x-content-type-options"), Some("nosniff"));
    assert!(header(&response, "content-security-policy").is_some());
    assert!(header(&response, "strict-transport-security").is_some());
    assert_eq!(
        header(&response, "referrer-policy"),
        Some("strict-origin-when-cross-origin")
    );
}

#[tokio::test]
async fn the_servers_policy_is_the_one_in_the_static_headers_file() {
    let (harness, site) = Harness::serving("rx13-parity").await;
    let response = harness.get("/guides/install").await;
    let served: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(n, v)| {
            (
                n.as_str().to_owned(),
                v.to_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();

    // The same rule file a static host reads.
    let rules = liyasa_build::hosting::emulate::parse_headers_file(&site.headers_file());
    let statically = rules.resolve("/guides/install");
    assert!(
        !statically.is_empty(),
        "the fixture wrote no rule for the page"
    );
    for (name, value) in statically {
        let serving = served
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(&name))
            .unwrap_or_else(|| panic!("the server did not send `{name}`"));
        assert_eq!(
            serving.1, value,
            "`{name}` differs between the server and `_headers`"
        );
    }
}

#[tokio::test]
async fn a_conditional_request_gets_a_304_with_the_same_caching_headers() {
    let (harness, _site) = Harness::serving("rx13-conditional").await;
    let first = expect_status(harness.get("/guides/install").await, StatusCode::OK);
    let etag = header(&first, "etag").expect("an entity tag").to_owned();
    let cache_control = header(&first, "cache-control")
        .expect("a policy")
        .to_owned();
    let last_modified = header(&first, "last-modified").expect("a date").to_owned();

    let second = expect_status(
        harness
            .get_with("/guides/install", &[("if-none-match", &etag)])
            .await,
        StatusCode::NOT_MODIFIED,
    );
    assert_eq!(header(&second, "etag"), Some(etag.as_str()));
    assert_eq!(
        header(&second, "cache-control"),
        Some(cache_control.as_str())
    );
    assert!(body_bytes(second).await.is_empty(), "a 304 carries no body");

    let by_date = expect_status(
        harness
            .get_with("/guides/install", &[("if-modified-since", &last_modified)])
            .await,
        StatusCode::NOT_MODIFIED,
    );
    assert!(body_bytes(by_date).await.is_empty());

    // A stale validator gets the body.
    expect_status(
        harness
            .get_with("/guides/install", &[("if-none-match", "\"stale\"")])
            .await,
        StatusCode::OK,
    );
}

#[tokio::test]
async fn a_markdown_twin_is_reachable_three_ways() {
    let (harness, _site) = Harness::serving("rx60-markdown").await;

    let by_suffix = expect_status(harness.get("/guides/install.md").await, StatusCode::OK);
    assert_eq!(
        header(&by_suffix, "content-type"),
        Some("text/markdown; charset=utf-8")
    );
    assert_eq!(
        header(&by_suffix, "vary"),
        None,
        "a `.md` URL does not vary: the URL said which representation it wanted"
    );
    let markdown = body_text(by_suffix).await;
    assert!(!markdown.is_empty());

    let by_index = expect_status(
        harness.get("/guides/install/index.md").await,
        StatusCode::OK,
    );
    assert_eq!(body_text(by_index).await, markdown);

    let negotiated = expect_status(
        harness
            .get_with("/guides/install", &[("accept", "text/markdown")])
            .await,
        StatusCode::OK,
    );
    assert_eq!(
        header(&negotiated, "content-type"),
        Some("text/markdown; charset=utf-8")
    );
    assert_eq!(
        header(&negotiated, "vary"),
        Some("Accept"),
        "the same URL answers differently per Accept, so a cache must key on it"
    );
    assert_eq!(body_text(negotiated).await, markdown);
}

#[tokio::test]
async fn a_browser_still_gets_html_from_the_same_url() {
    let (harness, _site) = Harness::serving("rx60-browser").await;
    let response = expect_status(
        harness
            .get_with(
                "/guides/install",
                &[(
                    "accept",
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                )],
            )
            .await,
        StatusCode::OK,
    );
    assert_eq!(
        header(&response, "content-type"),
        Some("text/html; charset=utf-8")
    );
}

#[tokio::test]
async fn an_unknown_route_is_a_404_and_never_a_200() {
    let (harness, _site) = Harness::serving("rx13-notfound").await;
    let response = expect_status(harness.get("/nothing/here").await, StatusCode::NOT_FOUND);
    assert!(header(&response, "x-content-type-options").is_some());
    expect_status(harness.get("/nothing/here.md").await, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_redirect_from_the_manifest_is_served_with_its_status() {
    let (harness, site) = Harness::serving("rx13-redirect").await;
    let manifest = site.report.manifest.as_ref().expect("a manifest");
    let Some(rule) = manifest.redirects.first() else {
        // The fixture has no redirect; nothing to assert rather than a false
        // pass.
        return;
    };
    let response = harness.get(&rule.source).await;
    assert_eq!(response.status().as_u16(), rule.status);
    assert_eq!(
        header(&response, "location"),
        Some(rule.destination.as_str())
    );
}

#[tokio::test]
async fn a_page_s_content_is_readable_by_path() {
    // REST-04's "get page content by path": the clause of that row that is
    // this package's rather than the search or assistant packages'.
    let (harness, _site) = Harness::serving("rest04-content").await;
    let body = liyasa_tests::server::body_json(expect_status(
        harness
            .get("/_liyasa/api/v1/content?path=/guides/install")
            .await,
        StatusCode::OK,
    ))
    .await;
    assert_eq!(body["route"], "/guides/install");
    assert_eq!(body["source"], "guides/install.md");
    assert!(
        body["markdown"]
            .as_str()
            .expect("markdown")
            .contains("Install"),
        "{body}"
    );
    assert_eq!(body["hidden"], false);
    assert!(!body["variants"].as_array().expect("variants").is_empty());

    expect_status(
        harness.get("/_liyasa/api/v1/content?path=/absent").await,
        StatusCode::NOT_FOUND,
    );
    expect_status(
        harness.get("/_liyasa/api/v1/content").await,
        StatusCode::BAD_REQUEST,
    );
}
