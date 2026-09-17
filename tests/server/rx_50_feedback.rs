//! RX-50 to RX-53, ANA-01, ANA-08, and ANA-09: feedback and the ingest path.
//!
//! No acceptance test is named for these P1 rows in §34.10, so this is the
//! one the packet asks for.

use http::StatusCode;
use liyasa_server::routes::{ServerConfig, feedback};
use liyasa_tests::server::{Harness, Setup, body_json, expect_status, header};
use serde_json::json;

#[tokio::test]
async fn a_reader_rates_a_page_and_the_ratio_follows() {
    let (harness, _site) = Harness::serving("rx50-rating").await;

    for rating in [1, 1, -1] {
        let response = expect_status(
            harness
                .post_json(
                    "/_liyasa/feedback",
                    json!({ "route": "/guides/install", "rating": rating }),
                )
                .await,
            StatusCode::CREATED,
        );
        let body = body_json(response).await;
        assert!(body["id"].as_str().expect("an id").starts_with("fb_"));
        assert_eq!(body["status"], "open");
    }

    let summary = body_json(expect_status(
        harness
            .get("/_liyasa/feedback/summary?route=/guides/install")
            .await,
        StatusCode::OK,
    ))
    .await;
    assert_eq!(summary["up"], 2);
    assert_eq!(summary["down"], 1);
}

#[tokio::test]
async fn a_written_comment_carries_a_category_and_an_unknown_one_is_refused() {
    let (harness, _site) = Harness::serving("rx50-category").await;
    for category in feedback::CATEGORIES {
        expect_status(
            harness
                .post_json(
                    "/_liyasa/feedback",
                    json!({
                        "route": "/guides/install",
                        "rating": -1,
                        "category": category,
                        "text": "the version number is wrong"
                    }),
                )
                .await,
            StatusCode::CREATED,
        );
    }
    let refused = expect_status(
        harness
            .post_json(
                "/_liyasa/feedback",
                json!({ "route": "/guides/install", "category": "shouting" }),
            )
            .await,
        StatusCode::BAD_REQUEST,
    );
    let problem = body_json(refused).await;
    assert!(
        problem["detail"]
            .as_str()
            .expect("a detail")
            .contains("inaccurate"),
        "the refusal names the categories it accepts: {problem}"
    );

    expect_status(
        harness
            .post_json(
                "/_liyasa/feedback",
                json!({ "route": "/guides/install", "rating": 7 }),
            )
            .await,
        StatusCode::BAD_REQUEST,
    );
    expect_status(
        harness
            .post_json("/_liyasa/feedback", json!({ "route": "not-a-path" }))
            .await,
        StatusCode::BAD_REQUEST,
    );
}

#[tokio::test]
async fn a_code_block_rating_names_the_block_it_was_on() {
    let (harness, _site) = Harness::serving("rx51-code").await;
    expect_status(
        harness
            .post_json(
                "/_liyasa/feedback",
                json!({
                    "route": "/guides/install",
                    "kind": "code",
                    "rating": -1,
                    "blockId": "a1b2c3d4e5f60718293a4b5c"
                }),
            )
            .await,
        StatusCode::CREATED,
    );
    let listed = body_json(harness.get("/_liyasa/feedback?kind=code").await).await;
    let items = listed["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"], "code");
    assert_eq!(items[0]["blockId"], "a1b2c3d4e5f60718293a4b5c");
}

#[tokio::test]
async fn an_agent_reports_a_page_that_failed_it() {
    let (harness, _site) = Harness::serving("rx52-agent").await;
    let created = body_json(expect_status(
        harness
            .post_json(
                "/_liyasa/feedback",
                json!({
                    "route": "/guides/install",
                    "kind": "agent",
                    "task": "install the CLI on Debian",
                    "text": "the apt repository line 404s"
                }),
            )
            .await,
        StatusCode::CREATED,
    ))
    .await;
    let id = created["id"].as_str().expect("an id").to_owned();

    // The dashboard sees it as a separate stream, and as plain text.
    let listed = body_json(harness.get("/_liyasa/feedback?kind=agent").await).await;
    let item = &listed["items"][0];
    assert_eq!(item["task"], "install the CLI on Debian");
    assert_eq!(
        item["textIsPlain"], true,
        "the dashboard is told never to render this as Markdown"
    );

    // An agent report with neither a task nor text is refused: it would be a
    // row nobody can act on.
    expect_status(
        harness
            .post_json(
                "/_liyasa/feedback",
                json!({ "route": "/guides/install", "kind": "agent" }),
            )
            .await,
        StatusCode::BAD_REQUEST,
    );

    // RX-53's workflow.
    let triaged = body_json(expect_status(
        harness
            .send(
                http::Request::builder()
                    .method("PATCH")
                    .uri(format!("/_liyasa/feedback/{id}"))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        json!({ "status": "triaged", "note": "assigned to docs" }).to_string(),
                    ))
                    .expect("a request"),
            )
            .await,
        StatusCode::OK,
    ))
    .await;
    assert_eq!(triaged["status"], "triaged");
    assert!(
        triaged["notes"]
            .as_str()
            .expect("notes")
            .contains("assigned to docs")
    );

    let open_only = body_json(harness.get("/_liyasa/feedback?status=open").await).await;
    assert!(
        open_only["items"]
            .as_array()
            .expect("items")
            .iter()
            .all(|i| i["id"] != id.as_str()),
        "a triaged row is out of the open list"
    );
}

#[tokio::test]
async fn feedback_text_is_scrubbed_before_it_is_stored() {
    let (harness, _site) = Harness::new(Setup::new("rx52-scrub")).await;
    // The scrubber's pattern rules cover what a reader might paste without
    // meaning to.
    expect_status(
        harness
            .post_json(
                "/_liyasa/feedback",
                json!({
                    "route": "/guides/install",
                    "kind": "agent",
                    "task": "call the API",
                    "text": "it failed with my key and my address is someone@example.com"
                }),
            )
            .await,
        StatusCode::CREATED,
    );
    let listed = body_json(harness.get("/_liyasa/feedback").await).await;
    let text = listed["items"][0]["text"].as_str().expect("text");
    assert!(
        !text.contains("someone@example.com"),
        "an address reached the table: {text}"
    );
}

#[tokio::test]
async fn an_oversized_body_is_refused_rather_than_truncated() {
    let (harness, _site) = Harness::serving("rx52-cap").await;
    let huge = "x".repeat(feedback::MAX_BODY * 2);
    let response = harness
        .post_json(
            "/_liyasa/feedback",
            json!({ "route": "/guides/install", "kind": "agent", "task": "t", "text": huge }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let problem = body_json(response).await;
    assert_eq!(problem["limit"], feedback::MAX_BODY);
}

#[tokio::test]
async fn a_retried_submission_with_an_idempotency_key_creates_one_row() {
    let (harness, _site) = Harness::serving("rest11-idempotency").await;
    let mut ids = Vec::new();
    for _ in 0..3 {
        let response = harness
            .send(
                http::Request::builder()
                    .method("POST")
                    .uri("/_liyasa/feedback")
                    .header("content-type", "application/json")
                    .header("idempotency-key", "client-generated-1")
                    .body(axum::body::Body::from(
                        json!({ "route": "/guides/install", "rating": 1 }).to_string(),
                    ))
                    .expect("a request"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        ids.push(
            body_json(response).await["id"]
                .as_str()
                .expect("an id")
                .to_owned(),
        );
    }
    assert_eq!(ids[0], ids[1]);
    assert_eq!(ids[1], ids[2]);
    let listed = body_json(harness.get("/_liyasa/feedback").await).await;
    assert_eq!(
        listed["items"].as_array().expect("items").len(),
        1,
        "a retry after a timeout must not double-count a rating"
    );
}

#[tokio::test]
async fn the_beacon_takes_client_events_and_refuses_server_side_ones() {
    let (harness, _site) = Harness::serving("ana01-beacon").await;
    let response = harness
        .post_json(
            "/_liyasa/e",
            json!([
                { "type": "scroll_depth", "route": "/guides/install", "props": { "percent": 75 } },
                { "type": "copy_code", "route": "/guides/install" },
                { "type": "deployment", "route": "/" }
            ]),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        harness.state.ingest.depth(),
        2,
        "a client cannot forge a deployment event"
    );

    let events = harness.state.ingest.drain(10);
    for event in &events {
        assert!(
            event.session_key.starts_with("k1:"),
            "the server assigns the session key"
        );
        assert_eq!(event.site, harness.state.config.site);
        assert!(
            !event.session_key.contains("127.0.0.1"),
            "no address reaches the event"
        );
    }
}

#[tokio::test]
async fn a_query_string_is_stripped_down_to_the_allowed_parameters() {
    let (harness, _site) = Harness::serving("ana03-query").await;
    harness
        .post_json(
            "/_liyasa/e",
            json!({
                "type": "page_load",
                "route": "/guides/install?utm_source=news&token=secret&q=install"
            }),
        )
        .await;
    let events = harness.state.ingest.drain(10);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].route, "/guides/install?utm_source=news&q=install");
}

#[tokio::test]
async fn a_collector_serves_no_site_and_answers_only_its_own_origins() {
    let (harness, _) = Harness::new(Setup {
        config: ServerConfig {
            collector_only: true,
            collector_origins: vec!["https://docs.example.com".to_owned()],
            // A collector accepts only the sites it was configured for; an
            // open one lets any client write into any site's aggregates.
            collector_sites: vec!["static-site".to_owned()],
            ..ServerConfig::default()
        },
        ..Setup::new("ana09-collector")
    })
    .await;

    // ANA-09: it accepts events.
    let accepted = harness
        .send(
            http::Request::builder()
                .method("POST")
                .uri("/_liyasa/e")
                .header("content-type", "application/json")
                .header("origin", "https://docs.example.com")
                .body(axum::body::Body::from(
                    json!({ "type": "page_load", "route": "/", "site": "static-site" }).to_string(),
                ))
                .expect("a request"),
        )
        .await;
    assert_eq!(accepted.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        header(&accepted, "access-control-allow-origin"),
        Some("https://docs.example.com")
    );
    assert_eq!(harness.state.ingest.depth(), 1);
    assert_eq!(
        harness.state.ingest.drain(1)[0].site,
        "static-site",
        "a static site says which site it is"
    );

    // An origin it was not given is refused.
    let refused = harness
        .send(
            http::Request::builder()
                .method("POST")
                .uri("/_liyasa/e")
                .header("content-type", "application/json")
                .header("origin", "https://someone-elses.example")
                .body(axum::body::Body::from(
                    json!({ "type": "page_load", "route": "/" }).to_string(),
                ))
                .expect("a request"),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);

    // And it serves nothing.
    let page = harness.get("/guides/install").await;
    assert_eq!(page.status(), StatusCode::NOT_FOUND);
    let problem = body_json(page).await;
    assert_eq!(problem["title"], "Collector only");
    // The probes still answer: a collector is still a process to monitor.
    expect_status(harness.get("/_liyasa/health").await, StatusCode::OK);
}

#[tokio::test]
async fn the_agent_channel_is_documented_where_an_agent_will_read_it() {
    // RX-52 says the endpoint is documented in llms.txt; the fragment and the
    // route come from one place so they cannot drift.
    let fragment = feedback::llms_txt_fragment("https://docs.example.com");
    assert!(fragment.contains("/_liyasa/feedback"));

    let (harness, _site) = Harness::serving("rx52-llms").await;
    let response = harness
        .post_json(
            "/_liyasa/feedback",
            json!({ "route": "/x", "kind": "agent", "task": "t" }),
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "the route the fragment advertises must exist"
    );
}

#[tokio::test]
async fn a_served_instance_ignores_the_site_a_body_claims() {
    // The body's `site` reaches the daily session key, so a client that could
    // choose it would choose its own session key — which is the one thing the
    // beacon's contract says it cannot do. It also writes into `agg_hour`,
    // which is the thirteen-month record rather than the ninety-day one, so a
    // wrong row here cannot be undone by waiting.
    let (harness, _site) = Harness::serving("ana09-attribution").await;
    let response = harness
        .send(
            http::Request::builder()
                .method("POST")
                .uri("/_liyasa/e")
                .header("content-type", "application/json")
                .header("origin", "https://unrelated.example")
                .body(axum::body::Body::from(
                    json!({
                        "type": "page_load",
                        "route": "/pricing",
                        "site": "some-other-site"
                    })
                    .to_string(),
                ))
                .expect("a request"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let events = harness.state.ingest.drain(10);
    assert_eq!(events.len(), 1, "the event is still recorded");
    assert_eq!(
        events[0].site, harness.state.config.site,
        "a served instance attributes to the site it serves, whatever the body says"
    );
    assert_ne!(events[0].site, "some-other-site");
}

#[tokio::test]
async fn a_collector_refuses_a_site_it_was_not_configured_for() {
    let (harness, _) = Harness::new(Setup {
        config: ServerConfig {
            collector_only: true,
            collector_origins: vec!["https://docs.example.com".to_owned()],
            collector_sites: vec!["acme-docs".to_owned()],
            ..ServerConfig::default()
        },
        ..Setup::new("ana09-collector-sites")
    })
    .await;

    let post = |site: &str| {
        let body = json!({ "type": "page_load", "route": "/", "site": site }).to_string();
        harness.send(
            http::Request::builder()
                .method("POST")
                .uri("/_liyasa/e")
                .header("content-type", "application/json")
                .header("origin", "https://docs.example.com")
                .body(axum::body::Body::from(body))
                .expect("a request"),
        )
    };

    assert_eq!(post("acme-docs").await.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        harness.state.ingest.depth(),
        1,
        "a configured site is accepted"
    );
    assert_eq!(harness.state.ingest.drain(1)[0].site, "acme-docs");

    // A beacon is fire and forget, so the refusal is silent to the client by
    // design; what matters is that nothing is stored.
    assert_eq!(
        post("someone-elses-docs").await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        harness.state.ingest.depth(),
        0,
        "a site the collector was not configured for must not reach the queue"
    );
}

#[tokio::test]
async fn a_collector_with_no_sites_configured_accepts_only_its_own() {
    // An open collector lets any client write into any site's aggregates, so
    // an unconfigured one is closed rather than permissive.
    let (harness, _) = Harness::new(Setup {
        config: ServerConfig {
            collector_only: true,
            site: "the-only-site".to_owned(),
            ..ServerConfig::default()
        },
        ..Setup::new("ana09-collector-closed")
    })
    .await;

    let post = |site: &str| {
        let body = json!({ "type": "page_load", "route": "/", "site": site }).to_string();
        harness.send(
            http::Request::builder()
                .method("POST")
                .uri("/_liyasa/e")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body))
                .expect("a request"),
        )
    };
    post("anything-at-all").await;
    assert_eq!(harness.state.ingest.depth(), 0);
    post("the-only-site").await;
    assert_eq!(harness.state.ingest.depth(), 1);
}
