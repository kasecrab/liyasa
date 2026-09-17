//! The router subtree (ANA-70, RFC 1704), driven as HTTP.
//!
//! Every request here goes through the real `axum` router into the real
//! queries against a real migrated database written by `liyasa-store`'s own
//! writer. Nothing is stubbed, so what these assert is the thing that can
//! actually drift: the query string the dashboard sends and the JSON shape it
//! reads back.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use http::{Request, StatusCode};
use liyasa_analytics::api::{self, Auth, ServedBy};
use liyasa_analytics::retention::Policy;
use liyasa_analytics::serve::{Analytics, mount, schema_router};
use liyasa_analytics::{props, schema};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::support::{DAY, Event, HOUR, T0, analytics, app};

/// A window that contains the fixtures below, whatever today is: the handlers
/// resolve "days" against the real clock, so a test that said `days=28` would
/// pass in September and fail in November.
fn window() -> String {
    format!("from={}&to={}", T0, T0 + DAY)
}

async fn state(
    name: &str,
    events: Vec<liyasa_store::records::EventRecord>,
) -> (Arc<Analytics>, Vec<crate::support::TempDir>) {
    let (adir, writer) = analytics(name, events).await;
    let (ddir, pool) = app(&format!("{name}-app")).await;
    let state = Arc::new(Analytics {
        analytics: writer.pool().clone(),
        app: pool,
        site: "acme-docs".to_owned(),
        project: None,
        retention: Policy::default(),
        integrations: json!({ "ga4": "G-ABC123", "fathom": { "id": "AB", "consent": "none" } }),
        pages: Vec::new(),
    });
    // The writer owns the pool; keeping it alive is what keeps the database
    // file open for the duration of the test.
    std::mem::forget(writer);
    (state, vec![adir, ddir])
}

async fn get(router: &Router, uri: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("a body");
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn post(router: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("a request"),
        )
        .await
        .expect("a response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("a body");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn views(route: &str, n: u32, at: i64) -> Vec<liyasa_store::records::EventRecord> {
    (0..n)
        .map(|i| Event::new("page_view", route, at).session(i).build())
        .collect()
}

#[tokio::test]
async fn every_endpoint_this_crate_claims_to_serve_is_routed() {
    // The test that keeps `api.rs` and the router from drifting apart. A row
    // marked `wp-17` with no route behind it is a 404 the dashboard would
    // report as "not served yet" while the list said otherwise.
    let (state, _dirs) = state("serve-routed", Vec::new()).await;
    let router = mount(state);
    let public = schema_router();

    for endpoint in api::ENDPOINTS
        .iter()
        .filter(|e| e.served_by == ServedBy::Wp17)
    {
        let target = if endpoint.auth == Auth::Public {
            &public
        } else {
            &router
        };
        let (status, _) = if endpoint.method == "POST" {
            post(
                target,
                endpoint.path,
                json!({ "kind": "create_page", "target": "x" }),
            )
            .await
        } else {
            get(target, &format!("{}?{}", endpoint.path, window())).await
        };
        assert_ne!(
            status,
            StatusCode::NOT_FOUND,
            "{} {} is marked wp-17 and nothing answers it",
            endpoint.method,
            endpoint.path
        );
        assert!(
            status.is_success() || status == StatusCode::BAD_REQUEST,
            "{} {} answered {status}",
            endpoint.method,
            endpoint.path
        );
    }
}

#[tokio::test]
async fn a_path_this_crate_does_not_serve_is_a_404_rather_than_an_empty_answer() {
    let (state, _dirs) = state("serve-404", Vec::new()).await;
    let router = mount(state);
    // `drift.open` belongs to the verification store, which is not this
    // package's. The list says `unbuilt` and the router agrees.
    let (status, _) = get(&router, "/_liyasa/api/v1/drift").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        api::endpoint("drift.open").expect("listed").served_by,
        ServedBy::Unbuilt
    );
}

#[tokio::test]
async fn a_series_comes_back_in_the_shape_the_dashboard_declares() {
    let mut events = views("/a", 3, T0 + HOUR);
    events.push(
        Event::new("page_view", "/a", T0 + HOUR)
            .agent("claudebot")
            .session(99)
            .build(),
    );
    let (state, _dirs) = state("serve-series", events).await;
    let router = mount(state);

    let (status, body) = get(
        &router,
        &format!("/_liyasa/api/v1/analytics/series?{}&grain=hour", window()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // The keys `web/dashboard/src/pages.ts` reads off `SeriesPayload`. The
    // camelCase one is the point: `sampled_by_client` would render as
    // `undefined` and the chart would silently lose ANA-10's label.
    assert!(body.get("sampledByClient").is_some(), "got {body}");
    assert_eq!(body["sampledByClient"], false);
    assert_eq!(body["grain"], "hour");
    assert_eq!(body["source"], "rollup");

    let points = body["points"].as_array().expect("points");
    assert_eq!(points.len(), 24, "one per hour of the range");
    let first = points
        .iter()
        .find(|p| p["bucket"] == T0 + HOUR)
        .expect("the hour the events are in");
    assert_eq!(first["human"], 3);
    assert_eq!(first["agent"], 1);
    assert_eq!(first["bot"], 0);
    assert_eq!(first["integration"], 0);
}

#[tokio::test]
async fn a_client_measured_series_carries_ana_10s_label() {
    let events = vec![Event::new("scroll_depth", "/a", T0 + HOUR).build()];
    let (state, _dirs) = state("serve-sampled", events).await;
    let router = mount(state);
    let (_, body) = get(
        &router,
        &format!(
            "/_liyasa/api/v1/analytics/series?{}&type=scroll_depth",
            window()
        ),
    )
    .await;
    assert_eq!(body["sampledByClient"], true);
}

#[tokio::test]
async fn the_delivery_ratio_is_measured_and_absent_rather_than_zero() {
    let mut events = views("/a", 10, T0 + HOUR);
    for i in 0..7 {
        events.push(Event::new("page_load", "/a", T0 + HOUR).session(i).build());
    }
    let (state, _dirs) = state("serve-delivery", events).await;
    let router = mount(state);

    let (status, body) = get(
        &router,
        &format!("/_liyasa/api/v1/analytics/delivery?{}", window()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["serverPageViews"], 10);
    assert_eq!(body["clientPageLoads"], 7);
    assert_eq!(body["ratio"], 0.7);

    // A window with no traffic has no ratio. `null`, not `0`: the dashboard
    // prints "—" for the first and "0%" for the second, and they mean
    // different things.
    let (_, empty) = get(
        &router,
        &format!(
            "/_liyasa/api/v1/analytics/delivery?from={}&to={}",
            T0 - DAY,
            T0
        ),
    )
    .await;
    assert_eq!(empty["ratio"], Value::Null);
    assert_eq!(empty["serverPageViews"], 0);
}

#[tokio::test]
async fn the_filters_on_the_query_string_reach_the_query() {
    let mut events = views("/guides/a", 4, T0 + HOUR);
    events.extend(views("/reference/b", 6, T0 + HOUR));
    let (state, _dirs) = state("serve-filters", events).await;
    let router = mount(state);

    let (_, all) = get(
        &router,
        &format!("/_liyasa/api/v1/analytics/pages?{}", window()),
    )
    .await;
    assert_eq!(all["pages"].as_array().expect("pages").len(), 2);

    let (_, filtered) = get(
        &router,
        &format!(
            "/_liyasa/api/v1/analytics/pages?{}&route=%2Fguides",
            window()
        ),
    )
    .await;
    let pages = filtered["pages"].as_array().expect("pages");
    assert_eq!(pages.len(), 1, "the route prefix filtered, got {filtered}");
    assert_eq!(pages[0]["route"], "/guides/a");
    assert_eq!(pages[0]["human"], 4);

    // And a filter that matches nothing returns nothing rather than everything.
    let (_, none) = get(
        &router,
        &format!("/_liyasa/api/v1/analytics/pages?{}&version=v9", window()),
    )
    .await;
    assert_eq!(none["pages"].as_array().expect("pages").len(), 0);
}

#[tokio::test]
async fn a_window_that_makes_no_sense_falls_back_rather_than_failing_the_page() {
    let (state, _dirs) = state("serve-bad-window", views("/a", 1, T0 + HOUR)).await;
    let router = mount(state);
    for query in [
        "from=nonsense&to=also",
        "days=-7",
        "from=9&to=1",
        "grain=fortnight",
        "",
    ] {
        let (status, _) = get(
            &router,
            &format!("/_liyasa/api/v1/analytics/series?{query}"),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "a bookmarked URL from an older release must still draw: {query}"
        );
    }
}

#[tokio::test]
async fn the_search_reports_come_back_in_the_shape_the_dashboard_reads() {
    let mut events = Vec::new();
    for i in 0..5 {
        events.push(
            Event::new("search", "/?q=x", T0 + HOUR)
                .session(i)
                .props(
                    serde_json::to_value(props::Search {
                        q: "webhooks".to_owned(),
                        results: 2,
                        shown: vec!["/webhooks".to_owned()],
                    })
                    .expect("props"),
                )
                .build(),
        );
    }
    events.push(
        Event::new("search_click", "/?q=x", T0 + HOUR)
            .props(
                serde_json::to_value(props::SearchClick {
                    q: "webhooks".to_owned(),
                    target: "/webhooks".to_owned(),
                    position: 1,
                })
                .expect("props"),
            )
            .build(),
    );
    let (state, _dirs) = state("serve-search", events).await;
    let router = mount(state);

    let (status, body) = get(
        &router,
        &format!("/_liyasa/api/v1/analytics/search/queries?{}", window()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rows = body["queries"].as_array().expect("queries");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["q"], "webhooks");
    assert_eq!(rows[0]["searches"], 5);
    assert_eq!(rows[0]["clicks"], 1);
    assert_eq!(rows[0]["empty"], 0);
    assert_eq!(rows[0]["topResult"], "/webhooks");

    let (_, per_page) = get(
        &router,
        &format!("/_liyasa/api/v1/analytics/search/pages?{}", window()),
    )
    .await;
    let pages = per_page["pages"].as_array().expect("pages");
    assert_eq!(pages[0]["route"], "/webhooks");
    assert_eq!(pages[0]["impressions"], 5);
    assert_eq!(pages[0]["clicks"], 1);
}

#[tokio::test]
async fn the_assistant_summary_reports_what_it_could_not_answer() {
    let events = vec![
        Event::new("assistant_message", "/", T0 + HOUR)
            .props(json!({ "thread": "t1", "latency_ms": 900, "rating": 1 }))
            .build(),
        Event::new("assistant_message", "/", T0 + 2 * HOUR)
            .props(json!({ "thread": "t2", "latency_ms": 900, "rating": -1 }))
            .build(),
        Event::new("assistant_message", "/", T0 + 3 * HOUR)
            .props(json!({ "thread": "t3", "latency_ms": 900, "topic": "sso saml", "unanswered": true }))
            .build(),
    ];
    let (state, _dirs) = state("serve-assistant", events).await;
    let router = mount(state);
    let (status, body) = get(
        &router,
        &format!("/_liyasa/api/v1/analytics/assistant?{}", window()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["messages"], 3);
    assert_eq!(body["rated"], 2);
    assert_eq!(body["positive"], 1);
    assert_eq!(body["unanswered"], json!(["sso saml"]));
}

#[tokio::test]
async fn an_insight_card_arrives_with_the_action_the_dashboard_wires_a_button_to() {
    let mut events = Vec::new();
    for i in 0..6 {
        events.push(
            Event::new("search", "/?q=x", T0 + HOUR)
                .session(i)
                .props(json!({ "q": "sso saml", "results": 0 }))
                .build(),
        );
    }
    let (state, _dirs) = state("serve-insights", events).await;
    let router = mount(state);

    let (status, body) = get(
        &router,
        &format!("/_liyasa/api/v1/analytics/insights?{}", window()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cards = body["cards"].as_array().expect("cards");
    let demand = cards
        .iter()
        .find(|c| c["kind"] == "unanswered_demand")
        .expect("a no-result search is a card");
    assert_eq!(demand["metrics"]["query"], "sso saml");
    assert_eq!(demand["action"]["kind"], "create_page");
    assert_eq!(demand["action"]["target"], "sso saml");
}

#[tokio::test]
async fn pressing_a_card_queues_a_real_job_and_an_unknown_action_queues_nothing() {
    let (state, _dirs) = state("serve-act", Vec::new()).await;
    let jobs = liyasa_store::jobs::Jobs::new(state.app.clone());
    let router = mount(state);

    let (status, body) = post(
        &router,
        "/_liyasa/api/v1/analytics/insights/act",
        json!({ "kind": "create_page", "target": "sso saml" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], liyasa_analytics::actions::CREATE_PAGE);

    let queued = jobs
        .list(
            &liyasa_core::store::JobQuery {
                name: Some(liyasa_analytics::actions::CREATE_PAGE.to_owned()),
                ..liyasa_core::store::JobQuery::default()
            },
            liyasa_core::store::Page::default(),
        )
        .await
        .expect("a list");
    assert_eq!(queued.len(), 1, "the job is really in the job table");
    assert_eq!(queued[0].payload["query"], "sso saml");

    // An action the dashboard does not offer is refused, and nothing is queued
    // for it. Without the second half this would pass on a handler that
    // enqueued first and validated afterwards.
    let (status, _) = post(
        &router,
        "/_liyasa/api/v1/analytics/insights/act",
        json!({ "kind": "rm -rf", "target": "/" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let after = jobs
        .list(
            &liyasa_core::store::JobQuery::default(),
            liyasa_core::store::Page::default(),
        )
        .await
        .expect("a list");
    assert_eq!(after.len(), 1, "no job was queued for the refused action");
}

#[tokio::test]
async fn the_settings_page_gets_the_consent_gate_and_the_statement() {
    let (state, _dirs) = state("serve-settings", Vec::new()).await;
    let router = mount(state);
    let (status, body) = get(&router, "/_liyasa/api/v1/analytics/integrations").await;
    assert_eq!(status, StatusCode::OK);

    let enabled = body["enabled"].as_array().expect("enabled");
    let ga4 = enabled.iter().find(|v| v["key"] == "ga4").expect("ga4");
    assert_eq!(ga4["consent"], "required");
    assert_eq!(ga4["loadsBeforeConsent"], false);
    let fathom = enabled
        .iter()
        .find(|v| v["key"] == "fathom")
        .expect("fathom");
    assert_eq!(fathom["loadsBeforeConsent"], true);

    // No consent provider is configured, so the gated one would never load and
    // the page says so (ANA-07).
    assert_eq!(body["stuck"], json!(["ga4"]));
    assert_eq!(body["provider"], Value::Null);
    assert!(
        body["consentStatement"]
            .as_str()
            .expect("a statement")
            .contains("yours to answer")
    );
    assert!(!body["inventory"].as_array().expect("inventory").is_empty());
}

#[tokio::test]
async fn the_published_schema_is_served_outside_the_gated_router() {
    let (state, _dirs) = state("serve-schema", Vec::new()).await;

    // Not in `mount`: a collector reads this before it may post anything and
    // has no dashboard credential, so it must not be inside whatever layer
    // wraps the dashboard subtree (ANA-02, RFC 1704).
    let (status, _) = get(&mount(state), schema::PATH).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the gated router must not carry the public document"
    );

    let public = schema_router();
    let response = public
        .oneshot(
            Request::builder()
                .uri(schema::PATH)
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/schema+json")
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("a body");
    let served: Value = serde_json::from_slice(&bytes).expect("json");

    // What is served is the schema, not a copy of it: §34.6's example must
    // validate against the document that came off the wire.
    let validator = jsonschema::validator_for(&served).expect("it compiles");
    let mut example: Value = serde_json::from_str(
        r#"{"ts":"2026-09-14T10:22:31Z","site":"acme-docs","env":"production","type":"page_view",
            "route":"/payments/create","format":"markdown","country":"US","duration_ms":12}"#,
    )
    .expect("the example parses");
    example["session_key"] = json!("k1:7f3a5c1d9e4b2a8f6c0d3e7b1a9f5c2e");
    assert!(validator.is_valid(&example));
}
